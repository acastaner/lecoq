//! A tool that creates an Annotation by dragging from one end to the other: line and arrow.
//! Shift snaps the angle to 15° steps (§4.1).

use lecoq_core::{AnnotationId, AnnotationShape, Arrow, Line, Point};

use super::{Pointer, Tool, ToolContext};

/// The angle step Shift snaps to.
const SNAP_DEGREES: f32 = 15.0;

pub struct DragSegment {
    /// Builds the Annotation's shape from the dragged ends.
    make: fn(Point, Point) -> AnnotationShape,
    drag: Option<Drag>,
}

struct Drag {
    start: Point,
    id: AnnotationId,
    moved: bool,
}

/// Line (L): drag; Shift snaps to 15° steps.
pub fn line() -> DragSegment {
    DragSegment::new(|a, b| Line::new(a, b).into())
}

/// Arrow (A): drag; Shift snaps to 15° steps.
pub fn arrow() -> DragSegment {
    DragSegment::new(|a, b| Arrow::new(a, b).into())
}

impl DragSegment {
    pub fn new(make: fn(Point, Point) -> AnnotationShape) -> Self {
        Self { make, drag: None }
    }
}

/// The end of a segment dragged from `start` to `to`; with Shift, at the same length but on the
/// nearest multiple of 15°. Shared with the select tool, whose endpoint handles (#50) snap the
/// dragged end around the fixed one with exactly this rule.
pub(super) fn dragged_end(start: Point, to: Pointer) -> Point {
    if !to.modifiers.shift {
        return to.pos;
    }
    let d = to.pos - start;
    let step = SNAP_DEGREES.to_radians();
    let angle = (d.y.atan2(d.x) / step).round() * step;
    let length = d.length();
    Point::new(
        start.x + length * angle.cos(),
        start.y + length * angle.sin(),
    )
}

impl Tool for DragSegment {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        cx.doc.begin_gesture();
        let id = cx
            .doc
            .add((self.make)(pointer.pos, pointer.pos), cx.style.clone());
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
        let shape = (self.make)(drag.start, dragged_end(drag.start, pointer));
        if let Some(a) = cx.doc.annotation_mut(drag.id) {
            a.shape = shape;
        }
    }

    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        let end = dragged_end(drag.start, pointer);
        if !drag.moved || drag.start.distance(end) < 1.0 {
            // A click draws nothing.
            cx.doc.cancel_gesture();
            return;
        }
        if let Some(a) = cx.doc.annotation_mut(drag.id) {
            a.shape = (self.make)(drag.start, end);
        }
        cx.doc.commit_gesture();
    }

    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if self.drag.take().is_some() && cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}
