//! Highlight (§4.1 H, §4.2): a region whose effect is computed live from everything below it in
//! z-order. The mode, colour and magnification are style properties ([`crate::HighlightMode`]).

use super::Shape;
use crate::geometry::{Affine, Point, Rect};
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

    fn image_transform(&mut self, t: &ImageTransform) {
        // A region follows the Image like a rectangle; the magnification is a factor, unscaled.
        self.transform(&t.affine());
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }

    fn needs_pixels_below(&self) -> bool {
        // An effect on what's beneath: nothing to show without the Image.
        true
    }
}
