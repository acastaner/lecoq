//! Obfuscate (§4.1 O, §4.2): a region pixelated or blurred, computed live from everything below it
//! in z-order. The mode and strength are style properties ([`crate::ObfuscateMode`]).

use super::Shape;
use crate::geometry::{Affine, Point, Rect, Vector};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Obfuscate {
    /// The obfuscated region.
    pub rect: Rect,
}

impl Obfuscate {
    /// §4.3: mode, strength.
    pub const PROPERTIES: &[PropertyKind] = &[PropertyKind::ObfuscateMode, PropertyKind::Strength];

    pub fn new(rect: Rect) -> Self {
        Self { rect }
    }
}

impl Shape for Obfuscate {
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
        // A region follows the Image like a rectangle; the strength (Image px) scales with a
        // resize in `Annotation::image_transform`.
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

    fn region() -> Obfuscate {
        Obfuscate::new(Rect::from_corners(
            Point::new(0.0, 0.0),
            Point::new(100.0, 50.0),
        ))
    }

    #[test]
    fn a_region_is_hit_anywhere_inside_and_misses_outside() {
        let o = region();
        let style = Style::default();
        assert!(o.hit(Point::new(50.0, 25.0), &style, 0.0), "the middle");
        assert!(!o.hit(Point::new(120.0, 25.0), &style, 0.0), "outside");
    }

    #[test]
    fn tolerance_widens_the_region() {
        let o = region();
        let p = Point::new(103.0, 25.0);
        assert!(!o.hit(p, &Style::default(), 0.0));
        assert!(o.hit(p, &Style::default(), 5.0));
    }
}
