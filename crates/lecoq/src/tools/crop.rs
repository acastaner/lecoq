//! Crop (C, §4.1): drag a region, adjust it with the handles or move it by dragging inside it.
//! Enter (or Apply) crops, Esc (or Cancel) drops the region; the editor does both (§4.7).
//!
//! The region is tool state, not a Document edit: it lives in [`ToolContext::crop`], in whole
//! Image pixels, always inside the Image. Only applying it makes a History step.

use lecoq_core::{Handle, PixelRect, Point, Rect, Vector};

use super::select::handle_at;
use super::{Pointer, Tool, ToolContext};

#[derive(Default)]
pub struct Crop {
    action: Option<Action>,
}

enum Action {
    New { start: Point },
    Resize { handle: Handle, from: Rect },
    Move { start: Point, from: Rect },
}

/// `r` in whole pixels inside the Image, `None` when empty.
fn clamped(cx: &ToolContext<'_>, r: Rect) -> Option<Rect> {
    PixelRect::within(r, cx.doc.width(), cx.doc.height()).map(PixelRect::to_rect)
}

/// `r` shifted (not shrunk) back inside the Image where possible.
fn kept_inside(cx: &ToolContext<'_>, r: Rect) -> Rect {
    let (w, h) = (cx.doc.width() as f32, cx.doc.height() as f32);
    let dx = (-r.min.x).max(0.0) + (w - r.max.x).min(0.0);
    let dy = (-r.min.y).max(0.0) + (h - r.max.y).min(0.0);
    let shift = Vector::new(dx, dy);
    Rect {
        min: r.min + shift,
        max: r.max + shift,
    }
}

impl Tool for Crop {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let p = pointer.pos;
        self.action = Some(match *cx.crop {
            Some(region) => match handle_at(region, p, cx.image_per_point) {
                Some(handle) => Action::Resize {
                    handle,
                    from: region,
                },
                None if region.contains(p) => Action::Move {
                    start: p,
                    from: region,
                },
                None => Action::New { start: p },
            },
            None => Action::New { start: p },
        });
    }

    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let p = pointer.pos;
        let region = match self.action {
            Some(Action::New { start }) => clamped(cx, Rect::from_corners(start, p)),
            Some(Action::Resize { handle, from }) => clamped(cx, from.with_handle_at(handle, p)),
            Some(Action::Move { start, from }) => {
                let by = p - start;
                let moved = Rect {
                    min: from.min + by,
                    max: from.max + by,
                };
                clamped(cx, kept_inside(cx, moved))
            }
            None => return,
        };
        // A resize or move never loses the region; a new drag replaces it as it goes.
        if region.is_some() || matches!(self.action, Some(Action::New { .. })) {
            *cx.crop = region;
        }
    }

    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        if self.action.is_some() {
            self.drag(cx, pointer);
            self.action = None;
        }
    }

    fn cancel(&mut self, _cx: &mut ToolContext<'_>) {
        self.action = None;
    }
}
