//! Step counter (I): a click places the next number (§4.5). Dragging before the release moves the
//! new counter; placing it (and any such drag) is one History step.

use lecoq_core::{AnnotationId, Point, Rgba, StepCounter, Style};

use super::{Pointer, Tool, ToolContext};

/// The tool's own defaults (§4.3): a red disc with a shadow.
pub fn default_style() -> Style {
    Style {
        fill: Some(Rgba::RED),
        ..Style::default()
    }
}

#[derive(Default)]
pub struct PlaceCounter {
    placing: Option<AnnotationId>,
}

impl Tool for PlaceCounter {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        cx.doc.begin_gesture();
        let id = cx.doc.add(StepCounter::new(pointer.pos), cx.style.clone());
        cx.doc.select([id]);
        self.placing = Some(id);
    }

    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        if let Some(id) = self.placing {
            move_to(cx, id, pointer.pos);
        }
    }

    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        if let Some(id) = self.placing.take() {
            move_to(cx, id, pointer.pos);
            cx.doc.commit_gesture();
        }
    }

    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if self.placing.take().is_some() && cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}

fn move_to(cx: &mut ToolContext<'_>, id: AnnotationId, to: Point) {
    if let Some(a) = cx.doc.annotation_mut(id) {
        a.shape = StepCounter::new(to).into();
    }
}
