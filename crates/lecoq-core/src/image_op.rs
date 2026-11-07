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
///
/// A variant describes only the geometry change; the size it turns or mirrors within is the
/// Image's own size before the operation, which [`crate::Document::image_operation`] already holds
/// and passes to [`Self::affine`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageTransform {
    /// Crop: the new Image starts at `origin` (old Image pixels). Annotations keep their position
    /// in the image, so they move by `-origin`; the canvas clips what lies past the new edges.
    Crop { origin: Point },
    /// A quarter turn, clockwise or not.
    Rotate90 { clockwise: bool },
    /// A mirror image: left-right when `horizontal`, else top-bottom.
    Flip { horizontal: bool },
    /// The Image scaled by `sx` horizontally and `sy` vertically (new size / old size).
    Resize { sx: f32, sy: f32 },
}

impl ImageTransform {
    /// The map from old Image pixels to new ones, for an Image that was `size` (width, height)
    /// before the operation. Shapes whose geometry is plain points and axis aligned rects implement
    /// [`crate::Shape::image_transform`] by applying it. Crop and resize ignore `size`; rotate and
    /// flip need it to place the old edges.
    pub fn affine(&self, size: Vector) -> Affine {
        match *self {
            ImageTransform::Crop { origin } => Affine::translate(Vector::new(-origin.x, -origin.y)),
            // (x, y) → (H − y, x): the old left edge becomes the top.
            ImageTransform::Rotate90 { clockwise: true } => Affine {
                a: 0.0,
                b: 1.0,
                c: -1.0,
                d: 0.0,
                e: size.y,
                f: 0.0,
            },
            // (x, y) → (y, W − x): the old top edge becomes the left.
            ImageTransform::Rotate90 { clockwise: false } => Affine {
                a: 0.0,
                b: -1.0,
                c: 1.0,
                d: 0.0,
                e: 0.0,
                f: size.x,
            },
            ImageTransform::Flip { horizontal: true } => Affine {
                a: -1.0,
                e: size.x,
                ..Affine::IDENTITY
            },
            ImageTransform::Flip { horizontal: false } => Affine {
                d: -1.0,
                f: size.y,
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

    /// Where an upright box (text, Step counter: ADR 0001 keeps them unrotated) goes on an Image
    /// that was `size` (width, height) before the operation: its centre moves through
    /// [`Self::affine`], it keeps its orientation, and its size scales by [`Self::size_scale`].
    pub fn upright_rect(&self, size: Vector, r: Rect) -> Rect {
        let c = self.affine(size).apply(r.center());
        let s = self.size_scale();
        let (w, h) = (r.width() * s, r.height() * s);
        Rect::from_min_size(Point::new(c.x - w / 2.0, c.y - h / 2.0), w, h)
    }
}

impl Pixels {
    /// Bytes held: what counts towards the History's pixel cap (§4.8).
    pub fn byte_len(&self) -> usize {
        match &self.data {
            PixelData::Rgb(d) | PixelData::Rgba(d) | PixelData::RgbaPremultiplied(d) => d.len(),
        }
    }

    fn channels(&self) -> usize {
        match self.data {
            PixelData::Rgb(_) => 3,
            PixelData::Rgba(_) | PixelData::RgbaPremultiplied(_) => 4,
        }
    }

    fn bytes(&self) -> &[u8] {
        match &self.data {
            PixelData::Rgb(d) | PixelData::Rgba(d) | PixelData::RgbaPremultiplied(d) => d,
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
            PixelData::Rgb(d) | PixelData::Rgba(d) | PixelData::RgbaPremultiplied(d) => d,
        };
        let mut out = Vec::with_capacity(row * region.height as usize);
        for y in region.y..region.y + region.height {
            let start = y as usize * stride + region.x as usize * n;
            out.extend_from_slice(&src[start..start + row]);
        }
        Pixels {
            width: region.width,
            height: region.height,
            data: self.data.same_layout(out),
        }
    }
}

impl Pixels {
    /// The Image turned a quarter, clockwise or not. The result is `height`×`width`; its pixels
    /// are a blocked transpose of this one's, written one output row at a time (#59). Bit-for-bit
    /// the old per-pixel remap, for every channel layout.
    pub fn rotate90(&self, clockwise: bool) -> Pixels {
        let (w, h) = (self.width as usize, self.height as usize);
        let n = self.channels();
        let out = rotate90_bytes(self.bytes(), w, h, n, clockwise);
        Pixels {
            width: self.height,
            height: self.width,
            data: self.data.same_layout(out),
        }
    }

    /// The mirror image: left-right when `horizontal`, else top-bottom. Horizontal reverses the
    /// pixels of each row; vertical copies whole rows in reverse order (one memcpy each) (#59).
    pub fn flip(&self, horizontal: bool) -> Pixels {
        let (w, h) = (self.width as usize, self.height as usize);
        let n = self.channels();
        let out = if horizontal {
            flip_horizontal_bytes(self.bytes(), w, h, n)
        } else {
            flip_vertical_bytes(self.bytes(), w, h, n)
        };
        Pixels {
            width: self.width,
            height: self.height,
            data: self.data.same_layout(out),
        }
    }
}

/// Flip left-right: for each row, append its `n`-byte pixels in reverse.
fn flip_horizontal_bytes(src: &[u8], w: usize, h: usize, n: usize) -> Vec<u8> {
    let stride = w * n;
    let mut out = Vec::with_capacity(src.len());
    for row in src.chunks_exact(stride).take(h) {
        for px in row.chunks_exact(n).rev() {
            out.extend_from_slice(px);
        }
    }
    out
}

/// Flip top-bottom: copy whole rows from the bottom up, one `copy_from_slice` (memcpy) each.
fn flip_vertical_bytes(src: &[u8], w: usize, h: usize, n: usize) -> Vec<u8> {
    let stride = w * n;
    let mut out = vec![0u8; src.len()];
    for y in 0..h {
        let dst = y * stride;
        let from = (h - 1 - y) * stride;
        out[dst..dst + stride].copy_from_slice(&src[from..from + stride]);
    }
    out
}

/// A quarter turn as a cache-blocked transpose. The result is `h`×`w` pixels; within a block the
/// inner loop writes an output row contiguously (`copy_from_slice`), reading a column band of the
/// source. Output pixel (ox, oy) comes from source (oy, h-1-ox) clockwise, (w-1-oy, ox) otherwise
/// — the same map the old per-pixel remap used.
fn rotate90_bytes(src: &[u8], w: usize, h: usize, n: usize, clockwise: bool) -> Vec<u8> {
    // Cache-friendly square in pixels (tens of pixels, per the ticket).
    const BLOCK: usize = 32;
    let (nw, nh) = (h, w); // new width, new height
    let src_stride = w * n;
    let dst_stride = nw * n;
    let mut out = vec![0u8; src.len()];
    let mut oy0 = 0;
    while oy0 < nh {
        let oy1 = (oy0 + BLOCK).min(nh);
        let mut ox0 = 0;
        while ox0 < nw {
            let ox1 = (ox0 + BLOCK).min(nw);
            for oy in oy0..oy1 {
                let di_row = oy * dst_stride;
                for ox in ox0..ox1 {
                    let (sx, sy) = if clockwise {
                        (oy, h - 1 - ox)
                    } else {
                        (w - 1 - oy, ox)
                    };
                    let si = sy * src_stride + sx * n;
                    let di = di_row + ox * n;
                    out[di..di + n].copy_from_slice(&src[si..si + n]);
                }
            }
            ox0 = ox1;
        }
        oy0 = oy1;
    }
    out
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

#[cfg(test)]
mod tests {
    //! Row-wise rotate/flip proved equal, pixel for pixel, to a naive per-pixel reference
    //! (issue #59). The reference is the independent source of truth: a straightforward
    //! coordinate remap, deliberately unlike the row-wise/blocked production code.

    use crate::document::{PixelData, Pixels};
    use crate::geometry::{Affine, Vector};
    use crate::image_op::ImageTransform;

    /// The affine for a quarter turn and a flip is exactly the old matrix, now that the Image size
    /// is supplied to [`ImageTransform::affine`] instead of carried in the variant. A 40×24 Image:
    /// width 40, height 24.
    #[test]
    fn affine_for_rotate_and_flip_is_unchanged_for_a_given_size() {
        let size = Vector::new(40.0, 24.0);
        // Clockwise: (x, y) → (H − y, x), so e = height = 24.
        assert_eq!(
            ImageTransform::Rotate90 { clockwise: true }.affine(size),
            Affine {
                a: 0.0,
                b: 1.0,
                c: -1.0,
                d: 0.0,
                e: 24.0,
                f: 0.0,
            },
        );
        // Counter-clockwise: (x, y) → (y, W − x), so f = width = 40.
        assert_eq!(
            ImageTransform::Rotate90 { clockwise: false }.affine(size),
            Affine {
                a: 0.0,
                b: -1.0,
                c: 1.0,
                d: 0.0,
                e: 0.0,
                f: 40.0,
            },
        );
        // Flip horizontal: x mirrored about the width, so a = −1, e = width = 40.
        assert_eq!(
            ImageTransform::Flip { horizontal: true }.affine(size),
            Affine {
                a: -1.0,
                e: 40.0,
                ..Affine::IDENTITY
            },
        );
        // Flip vertical: y mirrored about the height, so d = −1, f = height = 24.
        assert_eq!(
            ImageTransform::Flip { horizontal: false }.affine(size),
            Affine {
                d: -1.0,
                f: 24.0,
                ..Affine::IDENTITY
            },
        );
    }

    fn bytes(p: &Pixels) -> &[u8] {
        match &p.data {
            PixelData::Rgb(d) | PixelData::Rgba(d) | PixelData::RgbaPremultiplied(d) => d,
        }
    }

    fn channels(p: &Pixels) -> usize {
        match p.data {
            PixelData::Rgb(_) => 3,
            PixelData::Rgba(_) | PixelData::RgbaPremultiplied(_) => 4,
        }
    }

    /// The naive per-pixel remap that `rotate90`/`flip` used before #59: output pixel (x, y)
    /// comes from source pixel `from(x, y)`. The reference every row-wise result is checked
    /// against.
    fn reference_remap(
        p: &Pixels,
        width: u32,
        height: u32,
        from: impl Fn(usize, usize) -> (usize, usize),
    ) -> Pixels {
        let n = channels(p);
        let src = bytes(p);
        let stride = p.width as usize * n;
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
            data: p.data.same_layout(out),
        }
    }

    fn ref_rotate(p: &Pixels, clockwise: bool) -> Pixels {
        let (w, h) = (p.width as usize, p.height as usize);
        reference_remap(p, p.height, p.width, |x, y| {
            if clockwise {
                (y, h - 1 - x)
            } else {
                (w - 1 - y, x)
            }
        })
    }

    fn ref_flip(p: &Pixels, horizontal: bool) -> Pixels {
        let (w, h) = (p.width as usize, p.height as usize);
        reference_remap(p, p.width, p.height, |x, y| {
            if horizontal {
                (w - 1 - x, y)
            } else {
                (x, h - 1 - y)
            }
        })
    }

    /// A deterministic pseudo-random pixel buffer (xorshift), so a failure reproduces exactly.
    fn make(width: u32, height: u32, n: usize, variant: u8, seed: u64) -> Pixels {
        let mut state = seed | 1;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state & 0xff) as u8
        };
        let data: Vec<u8> = (0..width as usize * height as usize * n)
            .map(|_| next())
            .collect();
        let data = match variant {
            3 => PixelData::Rgb(data),
            4 => PixelData::Rgba(data),
            _ => PixelData::RgbaPremultiplied(data),
        };
        Pixels {
            width,
            height,
            data,
        }
    }

    fn assert_same(a: &Pixels, b: &Pixels, what: &str) {
        assert_eq!(a.width, b.width, "{what}: width");
        assert_eq!(a.height, b.height, "{what}: height");
        assert_eq!(bytes(a), bytes(b), "{what}: pixels");
    }

    /// Sizes exercising square, non-square, odd, 1×N, N×1 and 1×1.
    const SIZES: &[(u32, u32)] = &[
        (1, 1),
        (1, 5),
        (5, 1),
        (2, 2),
        (3, 2),
        (2, 3),
        (7, 3),
        (3, 7),
        (4, 4),
        (9, 5),
        (16, 1),
        (1, 16),
        (33, 17),
    ];

    #[test]
    fn rowwise_rotate_and_flip_equal_the_reference_for_every_layout() {
        for &variant in &[3u8, 4u8, 255u8] {
            let n = if variant == 3 { 3 } else { 4 };
            for (i, &(w, h)) in SIZES.iter().enumerate() {
                let p = make(w, h, n, variant, 0x9E37_79B9_7F4A_7C15 ^ (i as u64 + 1));
                assert_same(&p.rotate90(true), &ref_rotate(&p, true), "rotate cw");
                assert_same(&p.rotate90(false), &ref_rotate(&p, false), "rotate ccw");
                assert_same(&p.flip(true), &ref_flip(&p, true), "flip horizontal");
                assert_same(&p.flip(false), &ref_flip(&p, false), "flip vertical");
            }
        }
    }

    #[test]
    fn rotate_then_rotate_back_is_identity() {
        let p = make(9, 5, 4, 4, 42);
        let round_trip = p.rotate90(true).rotate90(false);
        assert_same(&round_trip, &p, "rotate cw then ccw");
    }

    #[test]
    fn flip_is_its_own_inverse() {
        let p = make(7, 3, 3, 3, 7);
        assert_same(&p.flip(true).flip(true), &p, "double flip horizontal");
        assert_same(&p.flip(false).flip(false), &p, "double flip vertical");
    }

    #[test]
    fn rotate_cw_maps_the_corners() {
        // 2×3 RGB: top-left is 10,10,10; the clockwise turn puts it top-right of a 3×2 image.
        let p = Pixels {
            width: 2,
            height: 3,
            data: PixelData::Rgb(vec![
                10, 10, 10, 20, 20, 20, // row 0
                30, 30, 30, 40, 40, 40, // row 1
                50, 50, 50, 60, 60, 60, // row 2
            ]),
        };
        let r = p.rotate90(true);
        assert_eq!((r.width, r.height), (3, 2));
        // New top row is the old left column bottom-to-top: 50,30,10.
        assert_eq!(&bytes(&r)[0..9], &[50, 50, 50, 30, 30, 30, 10, 10, 10]);
    }

    /// Measurement for issue #59: rotate a ~100 MP RGB Image. Ignored by default; run with
    /// `cargo test -p lecoq-core --release -- --ignored rotate_100mp_measurement --nocapture`.
    #[test]
    #[ignore = "perf measurement, run explicitly in --release"]
    fn rotate_100mp_measurement() {
        let (w, h) = (12_000u32, 8_400u32); // ~100.8 MP
        let p = make(w, h, 3, 3, 1);
        let start = std::time::Instant::now();
        let r = p.rotate90(true);
        let elapsed = start.elapsed();
        assert_eq!((r.width, r.height), (h, w));
        println!(
            "rotate90 of {w}x{h} RGB ({} MP): {elapsed:?}",
            w * h / 1_000_000
        );
    }
}
