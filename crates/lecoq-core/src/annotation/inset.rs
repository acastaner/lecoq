//! Inset (§4.6, GLOSSARY): an image placed on the Image, pasted from the clipboard or Ctrl+dropped.
//! Its pixels are drawn stretched over `rect`; it has a shadow and an optional border.

use std::sync::Arc;

use super::Shape;
use crate::document::Pixels;
use crate::geometry::{Affine, Point, Rect};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Inset {
    /// Where its pixels are drawn, in Image pixels.
    pub rect: Rect,
    /// Its own pixels (sRGB, straight alpha), shared by the History's snapshots and the copies.
    #[serde(with = "crate::clipboard::pixels_serde")]
    pub pixels: Arc<Pixels>,
}

/// Two Insets are the same when they cover the same rect with the same pixels (the same `Arc`):
/// comparing the pixels themselves would cost a pass over them per History step.
impl PartialEq for Inset {
    fn eq(&self, other: &Self) -> bool {
        self.rect == other.rect && Arc::ptr_eq(&self.pixels, &other.pixels)
    }
}

impl Inset {
    /// §4.3: shadow and border (none, or a colour and a thickness).
    pub const PROPERTIES: &[PropertyKind] = &[
        PropertyKind::BorderColour,
        PropertyKind::Thickness,
        PropertyKind::Shadow,
    ];

    /// An Inset of `pixels` placed as §4.6 says: at 100%, centred on `centre`, scaled down (keeping
    /// its aspect ratio) to fit inside `image` if it is larger, and moved inside `image` when it
    /// would stick out.
    pub fn placed(pixels: Arc<Pixels>, centre: Point, image: Rect) -> Self {
        let (w, h) = (pixels.width.max(1) as f32, pixels.height.max(1) as f32);
        let scale = (image.width() / w).min(image.height() / h).min(1.0);
        let (w, h) = (w * scale, h * scale);
        let x = (centre.x - w / 2.0).clamp(image.min.x, (image.max.x - w).max(image.min.x));
        let y = (centre.y - h / 2.0).clamp(image.min.y, (image.max.y - h).max(image.min.y));
        Self {
            rect: Rect::from_min_size(Point::new(x, y), w, h),
            pixels,
        }
    }
}

impl Shape for Inset {
    fn bounds(&self) -> Rect {
        self.rect
    }

    fn hit(&self, p: Point, _style: &Style, tolerance: f32) -> bool {
        self.rect.expand(tolerance).contains(p)
    }

    fn transform(&mut self, t: &Affine) {
        self.rect = t.apply_rect(self.rect);
    }

    fn image_transform(&mut self, t: &ImageTransform) {
        // An Inset turns and mirrors with the Image (§4.7): its box and its pixels.
        self.transform(&t.affine());
        match *t {
            ImageTransform::Rotate90 { clockwise, .. } => {
                self.pixels = Arc::new(self.pixels.rotate90(clockwise));
            }
            ImageTransform::Flip { horizontal, .. } => {
                self.pixels = Arc::new(self.pixels.flip(horizontal));
            }
            ImageTransform::Crop { .. } | ImageTransform::Resize { .. } => {}
        }
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }
}
