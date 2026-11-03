//! Ellipse (§4.1 E): the ellipse inscribed in a box, with a line and an optional fill.

use super::Shape;
use crate::geometry::{Affine, Point, Rect};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ellipse {
    /// The box the ellipse (the centre of its line) is inscribed in.
    pub rect: Rect,
}

impl Ellipse {
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

/// Is `p` inside the ellipse centred on `c` with radii `rx`, `ry`?
fn inside(p: Point, c: Point, rx: f32, ry: f32) -> bool {
    if rx <= 0.0 || ry <= 0.0 {
        return false;
    }
    let dx = (p.x - c.x) / rx;
    let dy = (p.y - c.y) / ry;
    dx * dx + dy * dy <= 1.0
}

impl Shape for Ellipse {
    fn bounds(&self) -> Rect {
        self.rect
    }

    fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool {
        let half = style.thickness / 2.0 + tolerance;
        let c = self.rect.center();
        let rx = self.rect.width() / 2.0;
        let ry = self.rect.height() / 2.0;
        if !inside(p, c, rx + half, ry + half) {
            return false;
        }
        // Filled: anywhere inside. Unfilled: only near the line.
        style.fill.is_some() || !inside(p, c, rx - half, ry - half)
    }

    fn transform(&mut self, t: &Affine) {
        self.rect = t.apply_rect(self.rect);
    }

    fn image_transform(&mut self, t: &ImageTransform) {
        self.transform(&t.affine());
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }
}
