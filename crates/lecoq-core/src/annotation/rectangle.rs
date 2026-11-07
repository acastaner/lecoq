//! Rectangle (§4.1 R): an axis-aligned box with a line and an optional fill.

use super::Shape;
use crate::geometry::{Affine, Point, Rect, Vector};
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

    fn image_transform(&mut self, t: &ImageTransform, size: Vector) {
        // Axis-aligned rects stay axis-aligned through crops, 90° rotations, flips and resizes.
        self.transform(&t.affine(size));
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Rgba;

    fn style(filled: bool) -> Style {
        Style {
            thickness: 4.0,
            fill: filled.then_some(Rgba::WHITE),
            ..Style::default()
        }
    }

    fn rect() -> Rectangle {
        Rectangle::new(Rect::from_corners(
            Point::new(0.0, 0.0),
            Point::new(100.0, 50.0),
        ))
    }

    #[test]
    fn unfilled_is_hit_on_the_border_but_not_in_the_hollow_middle() {
        let r = rect();
        assert!(
            r.hit(Point::new(0.0, 25.0), &style(false), 0.0),
            "left edge"
        );
        assert!(
            !r.hit(Point::new(50.0, 25.0), &style(false), 0.0),
            "hollow centre"
        );
    }

    #[test]
    fn filled_is_hit_anywhere_inside() {
        assert!(rect().hit(Point::new(50.0, 25.0), &style(true), 0.0));
    }

    #[test]
    fn a_point_beyond_the_stroke_and_tolerance_misses() {
        assert!(!rect().hit(Point::new(-5.0, 25.0), &style(false), 0.0));
    }

    #[test]
    fn tolerance_widens_the_hittable_border() {
        // 4 px left of the edge: outside the 2 px stroke half-width, inside a 5 px tolerance.
        let p = Point::new(-4.0, 25.0);
        assert!(!rect().hit(p, &style(false), 0.0));
        assert!(rect().hit(p, &style(false), 5.0));
    }
}
