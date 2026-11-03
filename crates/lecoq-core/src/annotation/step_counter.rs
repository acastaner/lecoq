//! Step counter (§4.1 I, §4.5): a numbered disc.
//!
//! The number isn't stored: it is the counter's rank among the Document's Step counters in
//! creation order ([`crate::Document::step_number`]), so deleting one renumbers the rest, undoing
//! the deletion restores the old numbering, and no number can be set by hand.
//!
//! The geometry is only the centre: the disc's diameter is the "Size" style property
//! ([`Style::counter_size`]). Transforms (moves, handle resizes, rotate and flip) move the centre
//! and leave the disc upright and its size alone (§4.7).

use super::Shape;
use crate::geometry::{Affine, Point, Rect};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StepCounter {
    pub centre: Point,
}

impl StepCounter {
    /// §4.3: fill colour, size, shadow.
    pub const PROPERTIES: &[PropertyKind] = &[
        PropertyKind::FillColour,
        PropertyKind::CounterSize,
        PropertyKind::Shadow,
    ];

    pub fn new(centre: Point) -> Self {
        Self { centre }
    }

    /// The disc for this style.
    pub fn disc(&self, style: &Style) -> Rect {
        let r = style.counter_size / 2.0;
        Rect::from_corners(
            Point::new(self.centre.x - r, self.centre.y - r),
            Point::new(self.centre.x + r, self.centre.y + r),
        )
    }
}

impl Shape for StepCounter {
    /// Without a style, only the centre; [`Shape::styled_bounds`] is the disc.
    fn bounds(&self) -> Rect {
        Rect::from_corners(self.centre, self.centre)
    }

    fn styled_bounds(&self, style: &Style) -> Rect {
        self.disc(style)
    }

    fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool {
        p.distance(self.centre) <= style.counter_size / 2.0 + tolerance
    }

    fn transform(&mut self, t: &Affine) {
        self.centre = t.apply(self.centre);
    }

    /// The centre follows the Image; the disc stays upright (its size scales with the style on a
    /// resize, in `Annotation::image_transform`).
    fn image_transform(&mut self, t: &ImageTransform) {
        self.centre = t.affine().apply(self.centre);
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }
}
