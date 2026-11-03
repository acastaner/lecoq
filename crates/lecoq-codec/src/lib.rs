//! Decoding and encoding (spec §5, §7.1).
//!
//! Today: display-size decode (§5.2) of every v1 read format (§5.1), with the EXIF orientation and
//! the conversion of non-sRGB ICC profiles to sRGB applied, and the frames of animated GIF and WebP
//! (§3.4, [`frames`]); encoding with EXIF/ICC write-back and the safe atomic save (§5.3,
//! [`save()`]).
//!
//! The display-size path reduces first, then orients and converts colour, so the extra work is
//! done on the small buffer.

use std::fmt;
use std::path::Path;

use fast_image_resize as fr;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

pub use animation::{Frame, Frames, frames};

mod animation;
#[cfg(feature = "avif")]
mod avif;
mod colour;
mod save;

pub use save::{
    SaveError, SaveFormat, SourceInfo, WEBP_QUALITY, encode_png_plain, save, source_info,
};

/// Pixel dimensions, width × height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    const fn transposed(self) -> Self {
        Self::new(self.height, self.width)
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}×{}", self.width, self.height)
    }
}

/// An image decoded for display: reduced to fit a box, never above its original size.
#[derive(Clone)]
pub struct DisplayImage {
    /// The size of the file's pixels, before any reduction, as displayed (after orientation).
    pub original_size: Size,
    /// The size of `pixels`.
    pub size: Size,
    pub pixels: Pixels,
    /// The file is a GIF or WebP, so it may be an animation: [`frames`] tells. The pixels are its
    /// first frame.
    pub may_animate: bool,
}

/// Tightly packed 8-bit sRGB pixels, row-major.
#[derive(Clone)]
pub enum Pixels {
    /// Opaque images (JPEG, and every image without an alpha channel).
    Rgb(Vec<u8>),
    /// Images with an alpha channel, not premultiplied.
    Rgba(Vec<u8>),
}

impl DisplayImage {
    /// The same image reduced to fit inside `max`, never enlarged (a pasted image's display-size
    /// pixels, §3.5).
    pub fn reduced(&self, max: Size) -> DisplayImage {
        let target = fit_within(self.size, max);
        DisplayImage {
            original_size: self.original_size,
            size: target,
            pixels: resize(self.pixels.clone(), self.size, target),
            may_animate: self.may_animate,
        }
    }
}

impl fmt::Debug for DisplayImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DisplayImage")
            .field("original_size", &self.original_size)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// Why an image couldn't be decoded. The `Display` text is the `<reason>` of
/// "Can't open <name>: <reason>" (§5.2).
#[derive(Debug)]
pub struct DecodeError(String);

impl DecodeError {
    fn new(reason: impl fmt::Display) -> Self {
        Self(reason.to_string())
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DecodeError {}

/// A file decoded at full resolution, not yet reduced, oriented or colour-converted. Splitting
/// [`decode_for_display`] in two lets the startup path decode before it knows the window size
/// (§1.3).
pub struct Decoded {
    /// The pixels as stored in the file.
    pixels: Pixels,
    /// The stored size of `pixels`.
    stored: Size,
    /// The EXIF orientation (§5.2), applied by [`Decoded::into_display`].
    orientation: Orientation,
    /// The embedded ICC profile, if any. Converted to sRGB by [`Decoded::into_display`] unless it
    /// already is sRGB (§5.2).
    icc: Option<Vec<u8>>,
    /// See [`DisplayImage::may_animate`].
    may_animate: bool,
}

impl Decoded {
    /// The size as displayed: after orientation.
    pub fn size(&self) -> Size {
        if swaps_axes(self.orientation) {
            self.stored.transposed()
        } else {
            self.stored
        }
    }

    /// Reduce to fit inside `max` (physical pixels), keeping the aspect ratio, never enlarging;
    /// then apply the orientation and convert to sRGB.
    pub fn into_display(self, max: Size) -> DisplayImage {
        let original_size = self.size();
        let target = fit_within(original_size, max);
        let stored_target = if swaps_axes(self.orientation) {
            target.transposed()
        } else {
            target
        };
        let mut pixels = resize(self.pixels, self.stored, stored_target);
        if self.orientation != Orientation::NoTransforms {
            pixels = orient(pixels, stored_target, self.orientation);
        }
        if let Some(icc) = &self.icc {
            colour::to_srgb(icc, &mut pixels);
        }
        DisplayImage {
            original_size,
            size: target,
            pixels,
            may_animate: self.may_animate,
        }
    }
}

/// The extensions of the formats lecoq reads (§5.1), lowercase. A file is a Sibling candidate when
/// its extension matches one of these case-insensitively (§3.2).
pub const READ_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "bmp", "ico", "tif", "tiff", "avif",
];

/// Does `path` have the extension of a format lecoq reads (case-insensitive)?
pub fn has_read_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            READ_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(ext))
        })
}

/// Decode a file at its full resolution.
pub fn decode(path: &Path) -> Result<Decoded, DecodeError> {
    let reader = ImageReader::open(path)
        .map_err(DecodeError::new)?
        .with_guessed_format()
        .map_err(DecodeError::new)?;
    if reader.format() == Some(ImageFormat::Avif) {
        return decode_avif(&std::fs::read(path).map_err(DecodeError::new)?);
    }
    decode_reader(reader)
}

/// Decode an image held in memory (a clipboard image, §6.3) at its full resolution. The format is
/// read from the data.
pub fn decode_bytes(data: &[u8]) -> Result<Decoded, DecodeError> {
    let reader = ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .map_err(DecodeError::new)?;
    match reader.format() {
        Some(ImageFormat::Avif) => decode_avif(data),
        None => Err(DecodeError::new("not an image lecoq can read")),
        Some(_) => decode_reader(reader),
    }
}

fn decode_reader<R: std::io::BufRead + std::io::Seek>(
    reader: ImageReader<R>,
) -> Result<Decoded, DecodeError> {
    let may_animate = matches!(reader.format(), Some(ImageFormat::Gif | ImageFormat::WebP));
    let mut decoder = reader.into_decoder().map_err(DecodeError::new)?;
    // Both come from headers the decoder has already read: nothing extra on the JPEG path (§1.3).
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let icc = decoder.icc_profile().ok().flatten();
    let image = DynamicImage::from_decoder(decoder).map_err(DecodeError::new)?;
    let stored = Size::new(image.width(), image.height());
    let pixels = if image.color().has_alpha() {
        Pixels::Rgba(image.into_rgba8().into_raw())
    } else {
        Pixels::Rgb(image.into_rgb8().into_raw())
    };
    Ok(Decoded {
        pixels,
        stored,
        orientation,
        icc,
        may_animate,
    })
}

#[cfg(feature = "avif")]
fn decode_avif(data: &[u8]) -> Result<Decoded, DecodeError> {
    avif::decode(data)
}

#[cfg(not(feature = "avif"))]
fn decode_avif(_data: &[u8]) -> Result<Decoded, DecodeError> {
    Err(DecodeError::new("this build of lecoq can't read AVIF"))
}

/// Decode a file for display inside `max` physical pixels (§5.2).
pub fn decode_for_display(path: &Path, max: Size) -> Result<DisplayImage, DecodeError> {
    Ok(decode(path)?.into_display(max))
}

/// The largest size with `size`'s aspect ratio that fits in `max`, never above `size` itself.
pub fn fit_within(size: Size, max: Size) -> Size {
    let scale = (f64::from(max.width) / f64::from(size.width))
        .min(f64::from(max.height) / f64::from(size.height))
        .min(1.0);
    if scale >= 1.0 {
        return size;
    }
    let w = ((f64::from(size.width) * scale).round() as u32).max(1);
    let h = ((f64::from(size.height) * scale).round() as u32).max(1);
    Size::new(w, h)
}

fn swaps_axes(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

/// Resample `pixels` from `from` to `to` (a no-op when they're equal). Alpha is premultiplied for
/// the filter and divided out again.
fn resize(pixels: Pixels, from: Size, to: Size) -> Pixels {
    if from == to {
        return pixels;
    }
    let (pixel_type, rebuild): (_, fn(Vec<u8>) -> Pixels) = match pixels {
        Pixels::Rgb(_) => (fr::PixelType::U8x3, Pixels::Rgb),
        Pixels::Rgba(_) => (fr::PixelType::U8x4, Pixels::Rgba),
    };
    let (Pixels::Rgb(buffer) | Pixels::Rgba(buffer)) = pixels;
    let src = fr::images::Image::from_vec_u8(from.width, from.height, buffer, pixel_type)
        .expect("buffer matches its dimensions");
    let mut dst = fr::images::Image::new(to.width, to.height, pixel_type);
    fr::Resizer::new()
        .resize(
            &src,
            &mut dst,
            &fr::ResizeOptions::new()
                .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Bilinear)),
        )
        .expect("same pixel type");
    rebuild(dst.into_vec())
}

/// Apply `orientation` to `pixels` of `size` (as stored).
fn orient(pixels: Pixels, size: Size, orientation: Orientation) -> Pixels {
    let mut image = match pixels {
        Pixels::Rgb(rgb) => DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(size.width, size.height, rgb)
                .expect("buffer matches its dimensions"),
        ),
        Pixels::Rgba(rgba) => DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(size.width, size.height, rgba)
                .expect("buffer matches its dimensions"),
        ),
    };
    image.apply_orientation(orientation);
    match image {
        DynamicImage::ImageRgb8(rgb) => Pixels::Rgb(rgb.into_raw()),
        DynamicImage::ImageRgba8(rgba) => Pixels::Rgba(rgba.into_raw()),
        _ => unreachable!("orientation keeps the pixel type"),
    }
}
