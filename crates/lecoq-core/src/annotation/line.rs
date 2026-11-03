//! Line (§4.1 L) and arrow (§4.1 A): a straight segment; the arrow adds arrowheads (§4.3).

use super::Shape;
use crate::geometry::{Affine, Point, Rect};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Line {
    pub from: Point,
    pub to: Point,
}

impl Line {
    /// §4.3: line colour, thickness, shadow.
    pub const PROPERTIES: &[PropertyKind] = &[
        PropertyKind::LineColour,
        PropertyKind::Thickness,
        PropertyKind::Shadow,
    ];

    pub fn new(from: Point, to: Point) -> Self {
        Self { from, to }
    }
}

impl Shape for Line {
    fn bounds(&self) -> Rect {
        Rect::from_corners(self.from, self.to)
    }

    fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool {
        distance_to_segment(p, self.from, self.to) <= style.thickness / 2.0 + tolerance
    }

    fn transform(&mut self, t: &Affine) {
        self.from = t.apply(self.from);
        self.to = t.apply(self.to);
    }

    fn image_transform(&mut self, t: &ImageTransform) {
        self.transform(&t.affine());
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }
}

/// An arrow from `from` to `to`; which ends carry a head is the Style's `arrowheads`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Arrow {
    pub from: Point,
    pub to: Point,
}

impl Arrow {
    /// §4.3: line colour, thickness, arrowheads, shadow.
    pub const PROPERTIES: &[PropertyKind] = &[
        PropertyKind::LineColour,
        PropertyKind::Thickness,
        PropertyKind::Arrowheads,
        PropertyKind::Shadow,
    ];

    pub fn new(from: Point, to: Point) -> Self {
        Self { from, to }
    }

    /// The length of an arrowhead for a line `thickness` wide, in Image pixels.
    pub fn head_length(thickness: f32) -> f32 {
        (thickness * 3.0 + 6.0).max(10.0)
    }
}

impl Shape for Arrow {
    fn bounds(&self) -> Rect {
        Rect::from_corners(self.from, self.to)
    }

    fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool {
        // The heads are as wide as they are long: count them as a thicker line near the ends.
        let reach = (style.thickness / 2.0).max(Self::head_length(style.thickness) / 2.0);
        let d = distance_to_segment(p, self.from, self.to);
        d <= style.thickness / 2.0 + tolerance
            || (d <= reach + tolerance
                && (p.distance(self.from).min(p.distance(self.to))
                    <= Self::head_length(style.thickness) + tolerance))
    }

    fn transform(&mut self, t: &Affine) {
        self.from = t.apply(self.from);
        self.to = t.apply(self.to);
    }

    fn image_transform(&mut self, t: &ImageTransform) {
        self.transform(&t.affine());
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }
}

/// The distance from `p` to the segment `a`–`b`.
pub(crate) fn distance_to_segment(p: Point, a: Point, b: Point) -> f32 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 == 0.0 {
        return p.distance(a);
    }
    let ap = p - a;
    let t = ((ap.x * ab.x + ap.y * ab.y) / len2).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * ab.x, a.y + t * ab.y))
}
