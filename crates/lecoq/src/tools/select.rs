//! Select (Esc): click selects, Shift+click adds to the selection (or takes a selected Annotation
//! out of it), drag moves the selection, the handles resize it, and a drag on empty space draws a
//! rubber band that selects the Annotations entirely inside it (with Shift, in addition to the
//! selection).
//!
//! Double-click to edit text (#28) goes in `press`.

use lecoq_core::{
    Affine, AnnotationId, AnnotationShape, Arrow, Document, Handle, Line, Point, Rect, Shape,
};

use super::{Pointer, Tool, ToolContext};

/// The side of a selection handle on screen, in points.
pub const HANDLE_SIZE: f32 = 9.0;
/// How far from a handle's centre a press still grabs it, in points.
const HANDLE_GRAB: f32 = 7.0;
/// How far from a thin line a press still hits it, in points. The cursor decider (#48) reuses it
/// so the pointing-hand appears exactly where a press would select.
pub const HIT_SLOP: f32 = 4.0;

/// Which end of a lone line/arrow a handle sits on (#50). A selection of exactly one line or arrow
/// is reshaped by two endpoint handles rather than the dashed bounding box, so either end can be
/// dragged on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    From,
    To,
}

/// What a press within the grab radius takes hold of: one of the 8 bounding-box handles, or an
/// endpoint of a lone line/arrow (#50). The press, the hover highlight (#49) and the cursor (#48)
/// all key off this one query so they agree on what the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    Handle(Handle),
    Endpoint(End),
}

/// The two endpoints `(from, to)` when the selection is exactly one line or arrow, which is drawn
/// and reshaped with endpoint handles instead of a bounding box (#50). `None` for any other
/// selection (a different single shape, or several shapes even if one is a line), which keeps the
/// dashed box and its 8 handles.
pub fn line_endpoints(doc: &Document) -> Option<(Point, Point)> {
    let [id] = doc.selection() else {
        return None;
    };
    endpoints_of(&doc.annotation(*id)?.shape)
}

/// The endpoints of a line or arrow shape; `None` for any other shape.
fn endpoints_of(shape: &AnnotationShape) -> Option<(Point, Point)> {
    match shape {
        AnnotationShape::Line(l) => Some((l.from, l.to)),
        AnnotationShape::Arrow(a) => Some((a.from, a.to)),
        _ => None,
    }
}

/// `shape` with its `end` moved to `p`, leaving the other end put. `shape` is a line or arrow.
fn with_endpoint(shape: &AnnotationShape, end: End, p: Point) -> AnnotationShape {
    match (shape, end) {
        (AnnotationShape::Line(l), End::From) => Line::new(p, l.to).into(),
        (AnnotationShape::Line(l), End::To) => Line::new(l.from, p).into(),
        (AnnotationShape::Arrow(a), End::From) => Arrow::new(p, a.to).into(),
        (AnnotationShape::Arrow(a), End::To) => Arrow::new(a.from, p).into(),
        _ => shape.clone(),
    }
}

#[derive(Default)]
pub struct Select {
    action: Option<Action>,
}

enum Action {
    Move {
        start: Point,
        originals: Vec<(AnnotationId, AnnotationShape)>,
        /// Shift+press on an already selected Annotation: a click (no movement) takes it out of
        /// the selection instead of moving anything.
        deselect_on_click: Option<AnnotationId>,
        moved: bool,
    },
    Resize {
        handle: Handle,
        bounds: Rect,
        originals: Vec<(AnnotationId, AnnotationShape)>,
        /// A corner handle on a selection with an Inset: the aspect ratio stays unless Shift is
        /// held (§4.6).
        keep_aspect: bool,
    },
    /// An endpoint handle of a lone line/arrow (#50): the drag moves only that end (it sets `from`
    /// or `to` directly, not a rect-to-rect scale of the whole shape). Shift snaps the segment to
    /// 15° around the other, fixed end — the creation-time snapping (`drag_segment::dragged_end`).
    MoveEndpoint {
        id: AnnotationId,
        end: End,
        /// The end that stays put: the pivot Shift snaps around.
        fixed: Point,
        /// The shape at press; each drag rebuilds from it so re-dragging doesn't compound.
        original: AnnotationShape,
        /// The press position, so a click (no movement) cancels with no undo step.
        start: Point,
        moved: bool,
    },
    /// A drag on empty space. `base` stays selected besides the enclosed Annotations (the
    /// previous selection with Shift, nothing without).
    RubberBand {
        start: Point,
        to: Point,
        base: Vec<AnnotationId>,
    },
}

/// The handle of the selection box under `p`, if any.
pub fn handle_at(bounds: Rect, p: Point, image_per_point: f32) -> Option<Handle> {
    let grab = HANDLE_GRAB * image_per_point;
    Handle::ALL
        .into_iter()
        .find(|h| bounds.handle_position(*h).distance(p) <= grab)
}

/// The endpoint of a lone line/arrow under `p`, within the grab radius. `From` wins a tie (a
/// zero-length segment).
fn endpoint_at(from: Point, to: Point, p: Point, image_per_point: f32) -> Option<End> {
    let grab = HANDLE_GRAB * image_per_point;
    if p.distance(from) <= grab {
        Some(End::From)
    } else if p.distance(to) <= grab {
        Some(End::To)
    } else {
        None
    }
}

/// What a press at `p` grabs for the current selection: an endpoint of a lone line/arrow (checked
/// first, so an endpoint wins over a Move), otherwise one of the bounding-box handles. `None` means
/// a press there is not a handle grab, so it falls back to Move or select. A lone line/arrow has no
/// box, so a press off its endpoints returns `None`. The one query the Select tool's press, the
/// hover highlight (#49) and the cursor (#48) share.
pub fn grab_at(doc: &Document, p: Point, image_per_point: f32) -> Option<Grab> {
    if let Some((from, to)) = line_endpoints(doc) {
        return endpoint_at(from, to, p, image_per_point).map(Grab::Endpoint);
    }
    let bounds = doc.selection_bounds()?;
    handle_at(bounds, p, image_per_point).map(Grab::Handle)
}

fn selected_shapes(cx: &ToolContext<'_>) -> Vec<(AnnotationId, AnnotationShape)> {
    cx.doc.selected().map(|a| (a.id, a.shape.clone())).collect()
}

fn apply(cx: &mut ToolContext<'_>, originals: &[(AnnotationId, AnnotationShape)], t: &Affine) {
    for (id, shape) in originals {
        if let Some(a) = cx.doc.annotation_mut(*id) {
            a.shape = shape.transformed(t);
        }
    }
}

fn is_corner(handle: Handle) -> bool {
    let (u, v) = handle.anchor();
    u != 0.5 && v != 0.5
}

/// Where a corner `handle` of `bounds` goes when dragged to `p` with the aspect ratio kept: the
/// opposite corner stays, and the box scales by the larger of the two axes' scales.
fn keeping_aspect(bounds: Rect, handle: Handle, p: Point) -> Point {
    let (u, v) = handle.anchor();
    let fixed = Point::new(
        bounds.min.x + (1.0 - u) * bounds.width(),
        bounds.min.y + (1.0 - v) * bounds.height(),
    );
    let (w, h) = (
        bounds.width().max(f32::EPSILON),
        bounds.height().max(f32::EPSILON),
    );
    let (dx, dy) = (p.x - fixed.x, p.y - fixed.y);
    let scale = (dx.abs() / w).max(dy.abs() / h);
    Point::new(
        fixed.x + dx.signum() * scale * w,
        fixed.y + dy.signum() * scale * h,
    )
}

/// Is `inner` entirely inside the rubber band?
fn encloses(band: Rect, inner: Rect) -> bool {
    band.contains(inner.min) && band.contains(inner.max)
}

impl Tool for Select {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let p = pointer.pos;
        let shift = pointer.modifiers.shift;
        // Handles win even with Shift (Shift+corner handle unlocks an Inset's aspect ratio, §4.6;
        // on a lone line/arrow Shift snaps the dragged endpoint, handled in `drag`).
        match grab_at(cx.doc, p, cx.image_per_point) {
            Some(Grab::Endpoint(end)) => {
                // An endpoint grab means the selection is exactly one line/arrow.
                let id = cx.doc.selection()[0];
                let original = cx
                    .doc
                    .annotation(id)
                    .expect("the lone line/arrow")
                    .shape
                    .clone();
                let (from, to) = endpoints_of(&original).expect("a line or arrow");
                let fixed = match end {
                    End::From => to,
                    End::To => from,
                };
                self.action = Some(Action::MoveEndpoint {
                    id,
                    end,
                    fixed,
                    original,
                    start: p,
                    moved: false,
                });
                cx.doc.begin_gesture();
                return;
            }
            Some(Grab::Handle(handle)) => {
                let bounds = cx
                    .doc
                    .selection_bounds()
                    .expect("a handle grab implies bounds");
                let originals = selected_shapes(cx);
                let keep_aspect = is_corner(handle)
                    && originals
                        .iter()
                        .any(|(_, shape)| matches!(shape, AnnotationShape::Inset(_)));
                self.action = Some(Action::Resize {
                    handle,
                    bounds,
                    originals,
                    keep_aspect,
                });
                cx.doc.begin_gesture();
                return;
            }
            None => {}
        }
        match cx.doc.hit_test(p, HIT_SLOP * cx.image_per_point) {
            Some(id) => {
                let already = cx.doc.is_selected(id);
                if shift && !already {
                    let mut ids = cx.doc.selection().to_vec();
                    ids.push(id);
                    cx.doc.select(ids);
                } else if !already {
                    cx.doc.select([id]);
                }
                self.action = Some(Action::Move {
                    start: p,
                    originals: selected_shapes(cx),
                    deselect_on_click: (shift && already).then_some(id),
                    moved: false,
                });
                cx.doc.begin_gesture();
            }
            None => {
                let base = if shift {
                    cx.doc.selection().to_vec()
                } else {
                    cx.doc.clear_selection();
                    Vec::new()
                };
                self.action = Some(Action::RubberBand {
                    start: p,
                    to: p,
                    base,
                });
            }
        }
    }

    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        match &mut self.action {
            Some(Action::Move {
                start,
                originals,
                moved,
                ..
            }) => {
                *moved |= pointer.pos != *start;
                let t = Affine::translate(pointer.pos - *start);
                apply(cx, originals, &t);
            }
            Some(Action::Resize {
                handle,
                bounds,
                originals,
                keep_aspect,
            }) => {
                let at = if *keep_aspect && !pointer.modifiers.shift {
                    keeping_aspect(*bounds, *handle, pointer.pos)
                } else {
                    pointer.pos
                };
                let to = bounds.with_handle_at(*handle, at);
                let t = Affine::rect_to_rect(*bounds, to);
                apply(cx, originals, &t);
            }
            Some(Action::MoveEndpoint {
                id,
                end,
                fixed,
                original,
                start,
                moved,
            }) => {
                *moved |= pointer.pos != *start;
                // Shift snaps the dragged end around the fixed one, exactly as drawing does.
                let np = super::drag_segment::dragged_end(*fixed, pointer);
                let shape = with_endpoint(original, *end, np);
                if let Some(a) = cx.doc.annotation_mut(*id) {
                    a.shape = shape;
                }
            }
            Some(Action::RubberBand { start, to, base }) => {
                *to = pointer.pos;
                let band = Rect::from_corners(*start, *to);
                let enclosed: Vec<AnnotationId> = cx
                    .doc
                    .annotations()
                    .iter()
                    .filter(|a| !base.contains(&a.id) && encloses(band, a.bounds()))
                    .map(|a| a.id)
                    .collect();
                cx.doc.select(base.iter().copied().chain(enclosed));
            }
            None => {}
        }
    }

    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        if self.action.is_none() {
            return;
        }
        self.drag(cx, pointer);
        match self.action.take() {
            Some(Action::RubberBand { .. }) | None => {}
            Some(Action::Move {
                deselect_on_click: Some(id),
                moved: false,
                ..
            }) => {
                cx.doc.cancel_gesture();
                let rest: Vec<AnnotationId> = cx
                    .doc
                    .selection()
                    .iter()
                    .copied()
                    .filter(|s| *s != id)
                    .collect();
                cx.doc.select(rest);
            }
            // A click on an endpoint (no movement) changes nothing: cancel, so no undo step.
            Some(Action::MoveEndpoint { moved: false, .. }) => {
                cx.doc.cancel_gesture();
            }
            Some(Action::Move { .. } | Action::Resize { .. } | Action::MoveEndpoint { .. }) => {
                cx.doc.commit_gesture();
            }
        }
    }

    fn rubber_band(&self) -> Option<Rect> {
        match &self.action {
            Some(Action::RubberBand { start, to, .. }) => Some(Rect::from_corners(*start, *to)),
            _ => None,
        }
    }

    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if self.action.take().is_some() && cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}
