//! Rectangle (§4.1 R): an axis-aligned box with a line and an optional fill.

use super::Shape;
use crate::geometry::{Affine, Point, Rect};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rectangle {
    /// The rectangle the line is centred on.
    pub rect: Rect,
}

impl Rectangle {
    /// §4.3: line colour, fill colour (or none), thickness, shadow.
    pub const PROPERTIES: &[PropertyKind] = &[
        PropertyKind::LineColour,
        PropertyKind::FillColour,
        PropertyKind::Thickness,
        PropertyKind::Shadow,
    ];

    pub fn new(rect: Rect) -> Self {
        Self { rect }
    }
}

impl Shape for Rectangle {
    fn bounds(&self) -> Rect {
        self.rect
    }

    fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool {
        let half = style.thickness / 2.0 + tolerance;
        if !self.rect.expand(half).contains(p) {
            return false;
        }
        // Filled: anywhere inside. Unfilled: only near the line.
        style.fill.is_some() || !self.rect.expand(-half).contains(p)
    }

    fn transform(&mut self, t: &Affine) {
        self.rect = t.apply_rect(self.rect);
    }

    fn image_transform(&mut self, t: &ImageTransform) {
        // Axis-aligned rects stay axis-aligned through crops, 90° rotations, flips and resizes.
        self.transform(&t.affine());
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }
}
