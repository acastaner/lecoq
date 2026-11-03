//! Ellipse (E): drag; Shift makes a circle.

use lecoq_core::{AnnotationShape, Ellipse, Rect};

use super::drag_box::DragBox;

pub fn new() -> DragBox {
    DragBox::new(|r: Rect| AnnotationShape::from(Ellipse::new(r)))
}
