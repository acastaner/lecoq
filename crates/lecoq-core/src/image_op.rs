//! Image operations (§4.7, ADR 0001): crop, rotate 90°, flip and resize.
//!
//! An Image operation is two things:
//! - new pixels, computed from the full-resolution [`Pixels`] (`Pixels::crop`, `rotate90` and
//!   `flip` here; resize resamples with Skia, so its pixels come from `lecoq-render` through
//!   [`crate::Document::resize`]);
//! - an [`ImageTransform`] saying how old Image pixel coordinates map to new ones, which every
//!   Annotation follows through [`crate::Shape::image_transform`].
//!
//! [`crate::Document::image_operation`] applies both as one History step (pixels and Annotations
//! restored on undo, the pixel cap enforced). A new operation only needs its pixel function and its
//! `ImageTransform` variant.

use crate::document::{PixelData, Pixels};
use crate::geometry::{Affine, Point, Rect, Vector};

/// How an Image operation moves Image pixels: what Annotations follow (ADR 0001).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageTransform {
    /// Crop: the new Image starts at `origin` (old Image pixels). Annotations keep their position
    /// in the image, so they move by `-origin`; the canvas clips what lies past the new edges.
    Crop { origin: Point },
    /// A quarter turn of an Image that was `width`×`height` before it.
    Rotate90 {
        clockwise: bool,
        width: f32,
        height: f32,
    },
    /// A mirror image of a `width`×`height` Image: left-right when `horizontal`, else top-bottom.
    Flip {
        horizontal: bool,
        width: f32,
        height: f32,
    },
    /// The Image scaled by `sx` horizontally and `sy` vertically (new size / old size).
    Resize { sx: f32, sy: f32 },
}

impl ImageTransform {
    /// The map from old Image pixels to new ones. Shapes whose geometry is plain points and axis
    /// aligned rects implement [`crate::Shape::image_transform`] by applying it.
    pub fn affine(&self) -> Affine {
        match *self {
            ImageTransform::Crop { origin } => Affine::translate(Vector::new(-origin.x, -origin.y)),
            // (x, y) → (H − y, x): the old left edge becomes the top.
            ImageTransform::Rotate90 {
                clockwise: true,
                height,
                ..
            } => Affine {
                a: 0.0,
                b: 1.0,
                c: -1.0,
                d: 0.0,
                e: height,
                f: 0.0,
            },
            // (x, y) → (y, W − x): the old top edge becomes the left.
            ImageTransform::Rotate90 {
                clockwise: false,
                width,
                ..
            } => Affine {
                a: 0.0,
                b: -1.0,
                c: 1.0,
                d: 0.0,
                e: 0.0,
                f: width,
            },
            ImageTransform::Flip {
                horizontal: true,
                width,
                ..
            } => Affine {
                a: -1.0,
                e: width,
                ..Affine::IDENTITY
            },
            ImageTransform::Flip {
                horizontal: false,
                height,
                ..
            } => Affine {
                d: -1.0,
                f: height,
                ..Affine::IDENTITY
            },
            ImageTransform::Resize { sx, sy } => Affine {
                a: sx,
                d: sy,
                ..Affine::IDENTITY
            },
        }
    }

    /// How much sizes given in Image pixels scale: line thickness, font size, the Step counter's
    /// size (§4.7: resize scales everything). 1 except for a resize, where it is the geometric
    /// mean of the two axes' scales (a stretch thickens lines by its average).
    pub fn size_scale(&self) -> f32 {
        match *self {
            ImageTransform::Resize { sx, sy } => (sx * sy).sqrt(),
            _ => 1.0,
        }
    }

    /// Where an upright box (text, Step counter: ADR 0001 keeps them unrotated) goes: its centre
    /// moves through [`Self::affine`], it keeps its orientation, and its size scales by
    /// [`Self::size_scale`].
    pub fn upright_rect(&self, r: Rect) -> Rect {
        let c = self.affine().apply(r.center());
        let s = self.size_scale();
        let (w, h) = (r.width() * s, r.height() * s);
        Rect::from_min_size(Point::new(c.x - w / 2.0, c.y - h / 2.0), w, h)
    }
}

impl Pixels {
    /// Bytes held: what counts towards the History's pixel cap (§4.8).
    pub fn byte_len(&self) -> usize {
        match &self.data {
            PixelData::Rgb(d) | PixelData::Rgba(d) => d.len(),
        }
    }

    fn channels(&self) -> usize {
        match self.data {
            PixelData::Rgb(_) => 3,
            PixelData::Rgba(_) => 4,
        }
    }

    /// The pixels inside `region`, given in whole pixels and inside the Image.
    pub fn crop(&self, region: PixelRect) -> Pixels {
        assert!(
            region.x + region.width <= self.width && region.y + region.height <= self.height,
            "crop region outside the Image"
        );
        let n = self.channels();
        let stride = self.width as usize * n;
        let row = region.width as usize * n;
        let src = match &self.data {
            PixelData::Rgb(d) | PixelData::Rgba(d) => d,
        };
        let mut out = Vec::with_capacity(row * region.height as usize);
        for y in region.y..region.y + region.height {
            let start = y as usize * stride + region.x as usize * n;
            out.extend_from_slice(&src[start..start + row]);
        }
        Pixels {
            width: region.width,
            height: region.height,
            data: match self.data {
                PixelData::Rgb(_) => PixelData::Rgb(out),
                PixelData::Rgba(_) => PixelData::Rgba(out),
            },
        }
    }
}

impl Pixels {
    /// The Image turned a quarter, clockwise or not.
    pub fn rotate90(&self, clockwise: bool) -> Pixels {
        let (w, h) = (self.width as usize, self.height as usize);
        // New pixel (x, y), in a h×w Image, comes from old pixel (sx, sy).
        self.remap(self.height, self.width, |x, y| {
            if clockwise {
                (y, h - 1 - x)
            } else {
                (w - 1 - y, x)
            }
        })
    }

    /// The mirror image: left-right when `horizontal`, else top-bottom.
    pub fn flip(&self, horizontal: bool) -> Pixels {
        let (w, h) = (self.width as usize, self.height as usize);
        self.remap(self.width, self.height, |x, y| {
            if horizontal {
                (w - 1 - x, y)
            } else {
                (x, h - 1 - y)
            }
        })
    }

    /// A `width`×`height` Image whose pixel (x, y) is this one's pixel `from(x, y)`.
    fn remap(
        &self,
        width: u32,
        height: u32,
        from: impl Fn(usize, usize) -> (usize, usize),
    ) -> Pixels {
        let n = self.channels();
        let src = match &self.data {
            PixelData::Rgb(d) | PixelData::Rgba(d) => d,
        };
        let stride = self.width as usize * n;
        let mut out = Vec::with_capacity(width as usize * height as usize * n);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let (sx, sy) = from(x, y);
                let i = sy * stride + sx * n;
                out.extend_from_slice(&src[i..i + n]);
            }
        }
        Pixels {
            width,
            height,
            data: match self.data {
                PixelData::Rgb(_) => PixelData::Rgb(out),
                PixelData::Rgba(_) => PixelData::Rgba(out),
            },
        }
    }
}

/// A rectangle of whole Image pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    /// `r` rounded to whole pixels and clamped to a `width`×`height` Image. `None` when nothing
    /// of it is left.
    pub fn within(r: Rect, width: u32, height: u32) -> Option<Self> {
        let clamp = |v: f32, max: u32| v.round().clamp(0.0, max as f32) as u32;
        let (x0, x1) = (clamp(r.min.x, width), clamp(r.max.x, width));
        let (y0, y1) = (clamp(r.min.y, height), clamp(r.max.y, height));
        (x1 > x0 && y1 > y0).then_some(Self {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }

    pub fn to_rect(self) -> Rect {
        Rect::from_min_size(
            Point::new(self.x as f32, self.y as f32),
            self.width as f32,
            self.height as f32,
        )
    }
}
