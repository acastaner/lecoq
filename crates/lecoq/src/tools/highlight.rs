//! Highlight (H): drag a region; Shift makes a square. The mode is a style property.

use lecoq_core::{AnnotationShape, Highlight, Rect};

use super::drag_box::DragBox;

pub fn new() -> DragBox {
    DragBox::new(|r: Rect| AnnotationShape::from(Highlight::new(r)))
}
