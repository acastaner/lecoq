//! Obfuscate (O): drag a region; Shift makes a square. The mode and strength are style properties.

use lecoq_core::{AnnotationShape, Obfuscate, Rect};

use super::drag_box::DragBox;

pub fn new() -> DragBox {
    DragBox::new(|r: Rect| AnnotationShape::from(Obfuscate::new(r)))
}
