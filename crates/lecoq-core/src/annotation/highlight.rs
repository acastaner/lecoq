//! Highlight (§4.1 H, §4.2): a region whose effect is computed live from everything below it in
//! z-order. The mode, colour and magnification are style properties ([`crate::HighlightMode`]).

use super::Shape;
use crate::geometry::{Affine, Point, Rect, Vector};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Highlight {
    /// The highlighted region.
    pub rect: Rect,
}

impl Highlight {
    /// §4.3: mode, colour (text mode), magnification (magnify mode). The property bar shows the
    /// colour and the magnification only in the mode they apply to.
    pub const PROPERTIES: &[PropertyKind] = &[
        PropertyKind::HighlightMode,
        PropertyKind::HighlightColour,
        PropertyKind::Magnification,
    ];

    pub fn new(rect: Rect) -> Self {
        Self { rect }
    }
}

impl Shape for Highlight {
    fn bounds(&self) -> Rect {
        self.rect
    }

    fn hit(&self, p: Point, _style: &Style, tolerance: f32) -> bool {
        // A region: anywhere inside it.
        self.rect.expand(tolerance).contains(p)
    }

    fn transform(&mut self, t: &Affine) {
        self.rect = t.apply_rect(self.rect);
    }

    fn image_transform(&mut self, t: &ImageTransform, size: Vector) {
        // A region follows the Image like a rectangle; the magnification is a factor, unscaled.
        self.transform(&t.affine(size));
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }

    fn needs_pixels_below(&self) -> bool {
        // An effect on what's beneath: nothing to show without the Image.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region() -> Highlight {
        Highlight::new(Rect::from_corners(
            Point::new(0.0, 0.0),
            Point::new(100.0, 50.0),
        ))
    }

    #[test]
    fn a_region_is_hit_anywhere_inside_and_misses_outside() {
        let h = region();
        let style = Style::default();
        assert!(h.hit(Point::new(50.0, 25.0), &style, 0.0), "the middle");
        assert!(!h.hit(Point::new(120.0, 25.0), &style, 0.0), "outside");
    }

    #[test]
    fn tolerance_widens_the_region() {
        let h = region();
        let p = Point::new(103.0, 25.0);
        assert!(!h.hit(p, &Style::default(), 0.0));
        assert!(h.hit(p, &Style::default(), 5.0));
    }
}
