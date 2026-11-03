//! A tool that creates an Annotation by dragging out a box: rectangle, ellipse, and the region
//! tools (highlight, obfuscate). Shift constrains the box to a square (§4.1).

use lecoq_core::{AnnotationId, AnnotationShape, Point, Rect};

use super::{Pointer, Tool, ToolContext};

pub struct DragBox {
    /// Builds the Annotation's shape from the dragged box.
    make: fn(Rect) -> AnnotationShape,
    drag: Option<Drag>,
}

struct Drag {
    start: Point,
    id: AnnotationId,
    moved: bool,
}

impl DragBox {
    pub fn new(make: fn(Rect) -> AnnotationShape) -> Self {
        Self { make, drag: None }
    }
}

/// The box from `start` to `to`; with Shift, the largest square in it on `to`'s side.
fn dragged_box(start: Point, to: Pointer) -> Rect {
    let mut end = to.pos;
    if to.modifiers.shift {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let side = dx.abs().max(dy.abs());
        end = Point::new(start.x + side * sign(dx), start.y + side * sign(dy));
    }
    Rect::from_corners(start, end)
}

fn sign(v: f32) -> f32 {
    if v < 0.0 { -1.0 } else { 1.0 }
}

impl Tool for DragBox {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        cx.doc.begin_gesture();
        let shape = (self.make)(Rect::from_corners(pointer.pos, pointer.pos));
        let id = cx.doc.add(shape, cx.style.clone());
        cx.doc.select([id]);
        self.drag = Some(Drag {
            start: pointer.pos,
            id,
            moved: false,
        });
    }

    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        drag.moved = true;
        let shape = (self.make)(dragged_box(drag.start, pointer));
        if let Some(a) = cx.doc.annotation_mut(drag.id) {
            a.shape = shape;
        }
    }

    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        let rect = dragged_box(drag.start, pointer);
        if !drag.moved || (rect.width() < 1.0 && rect.height() < 1.0) {
            // A click draws nothing.
            cx.doc.cancel_gesture();
            return;
        }
        if let Some(a) = cx.doc.annotation_mut(drag.id) {
            a.shape = (self.make)(rect);
        }
        cx.doc.commit_gesture();
    }

    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if self.drag.take().is_some() && cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}
