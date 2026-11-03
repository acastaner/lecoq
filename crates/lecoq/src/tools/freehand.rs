//! Freehand (F): drag; the stroke follows the pointer and is lightly smoothed when released.

use lecoq_core::{AnnotationId, AnnotationShape, Freehand, Point};

use super::{Pointer, Tool, ToolContext};

/// Pointer moves closer than this to the last kept point are jitter, in screen points.
const MIN_STEP: f32 = 2.0;

#[derive(Default)]
pub struct FreehandTool {
    drag: Option<Drag>,
}

struct Drag {
    id: AnnotationId,
    /// The pointer's path, unsmoothed.
    path: Vec<Point>,
}

impl Tool for FreehandTool {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        cx.doc.begin_gesture();
        let id = cx.doc.add(
            AnnotationShape::from(Freehand::new(vec![pointer.pos])),
            cx.style.clone(),
        );
        cx.doc.select([id]);
        self.drag = Some(Drag {
            id,
            path: vec![pointer.pos],
        });
    }

    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        if drag.path.last() == Some(&pointer.pos) {
            return;
        }
        drag.path.push(pointer.pos);
        // While drawing, the raw path: smoothing a growing stroke would make its tail wobble.
        if let Some(a) = cx.doc.annotation_mut(drag.id) {
            a.shape = AnnotationShape::from(Freehand::new(drag.path.clone()));
        }
    }

    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        if drag.path.last() != Some(&pointer.pos) {
            drag.path.push(pointer.pos);
        }
        let start = drag.path[0];
        if drag.path.iter().all(|p| p.distance(start) < 1.0) {
            // A click draws nothing.
            cx.doc.cancel_gesture();
            return;
        }
        let stroke = Freehand::smoothed(&drag.path, MIN_STEP * cx.image_per_point);
        if let Some(a) = cx.doc.annotation_mut(drag.id) {
            a.shape = stroke.into();
        }
        cx.doc.commit_gesture();
    }

    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if self.drag.take().is_some() && cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}
