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
use std::sync::Arc;

use fast_image_resize as fr;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
pub use lecoq_core::{PixelData, Pixels};

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

/// An image decoded for display: reduced to fit a box, never above its original size, and left
/// upload-ready. `pixels` are premultiplied RGBA in egui's exact layout (#56, §7.2), produced on
/// the decode pool so a cached switch is an `Arc` clone and a GPU upload with no conversion on the
/// UI thread; they are shared (`Arc`) with the display-size cache, counted once against its cap.
/// Full-resolution pixels stay straight and never pass through here (that is [`Decoded::into_full`]
/// and #57).
#[derive(Clone)]
pub struct DisplayImage {
    /// The size of the file's pixels, before any reduction, as displayed (after orientation).
    pub original_size: Size,
    /// The size of `pixels`.
    pub size: Size,
    /// Premultiplied, upload-ready pixels, shared with the cache and the shown Image's texture.
    pub pixels: Arc<Pixels>,
    /// The source had no alpha channel, so every pixel is opaque (alpha 255) and straight alpha
    /// equals premultiplied: the common case, uploaded with no premultiply pass (#56).
    pub opaque: bool,
    /// The file is a GIF or WebP, so it may be an animation: [`frames`] tells. The pixels are its
    /// first frame.
    pub may_animate: bool,
}

impl DisplayImage {
    /// A display-size, upload-ready image reduced from straight full-resolution `pixels` (whose
    /// stored size is `full_size`) to fit inside `max`, never enlarged, then premultiplied: a
    /// pasted image's display copy, built from the same decode that feeds the editor's
    /// full-resolution pixels (§3.5), so the file is decoded once.
    pub fn reduced_upload_ready(pixels: &Pixels, full_size: Size, max: Size) -> DisplayImage {
        let target = fit_within(full_size, max);
        let reduced = resize(pixels.clone(), full_size, target);
        upload_ready(reduced, full_size, target, false)
    }
}

/// Wrap straight, oriented, colour-converted `pixels` (displayed `size`, original `original_size`)
/// as an upload-ready [`DisplayImage`]: premultiply exactly the way egui does and flag whether the
/// source was opaque. For RGB this only appends an opaque alpha (no premultiply arithmetic); for
/// straight RGBA it premultiplies once, here on the pool thread (#56).
fn upload_ready(
    pixels: Pixels,
    original_size: Size,
    size: Size,
    may_animate: bool,
) -> DisplayImage {
    let opaque = matches!(pixels.data, PixelData::Rgb(_));
    DisplayImage {
        original_size,
        size,
        pixels: Arc::new(pixels.premultiplied()),
        opaque,
        may_animate,
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
    /// The embedded ICC profile, if any. [`Decoded::into_display`] prepares a [`colour::Converter`]
    /// from it and converts the display pixels to sRGB unless the profile already is sRGB (§5.2).
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
    /// then apply the orientation, convert to sRGB, and leave the pixels upload-ready
    /// (premultiplied, #56). The colour converter is prepared from this decode's profile, once,
    /// here.
    pub fn into_display(self, max: Size) -> DisplayImage {
        let converter = colour::Converter::prepare(self.icc.as_deref());
        self.into_display_with(max, &converter)
    }

    /// As [`Decoded::into_display`], but with a colour `converter` prepared elsewhere and reused:
    /// an animation prepares one for the whole file and passes it for every frame, so the profile
    /// is parsed once, not once per frame (#60). This decode's own `icc` is not consulted.
    pub(crate) fn into_display_with(
        self,
        max: Size,
        converter: &colour::Converter,
    ) -> DisplayImage {
        let may_animate = self.may_animate;
        let (pixels, original_size, size) = self.oriented(max, converter);
        // Premultiply here, on the decode pool, so the UI thread never converts on a switch (#56).
        upload_ready(pixels, original_size, size, may_animate)
    }

    /// The full-resolution pixels, oriented and colour-converted but left straight (not reduced,
    /// not premultiplied): what the editor and the save encoders work from (§5.3, §7.2). The
    /// colour converter is prepared from this decode's profile, once, here.
    pub fn into_full(self) -> Pixels {
        let converter = colour::Converter::prepare(self.icc.as_deref());
        let max = self.size();
        let (pixels, _, _) = self.oriented(max, &converter);
        pixels
    }

    /// Reduce to fit inside `max`, then orient and convert to sRGB, leaving straight pixels. The
    /// shared core of the display path (which then premultiplies) and the full-resolution path
    /// (which does not). Returns the straight pixels with their original and reduced sizes.
    fn oriented(self, max: Size, converter: &colour::Converter) -> (Pixels, Size, Size) {
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
        converter.convert(&mut pixels);
        (pixels, original_size, target)
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
    let data = if image.color().has_alpha() {
        PixelData::Rgba(image.into_rgba8().into_raw())
    } else {
        PixelData::Rgb(image.into_rgb8().into_raw())
    };
    let pixels = Pixels {
        width: stored.width,
        height: stored.height,
        data,
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

/// Reduce `pixels` to fit within `max`, keeping the aspect ratio and never enlarging: the zoom
/// texture built from the shared full-resolution decode, capped at the GPU's largest texture
/// (#57). A no-op clone when the pixels already fit.
pub fn reduced_to_fit(pixels: &Pixels, max: Size) -> Pixels {
    let from = Size::new(pixels.width, pixels.height);
    resize(pixels.clone(), from, fit_within(from, max))
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
    let (pixel_type, rebuild): (_, fn(Vec<u8>) -> PixelData) = match pixels.data {
        PixelData::Rgb(_) => (fr::PixelType::U8x3, PixelData::Rgb),
        PixelData::Rgba(_) => (fr::PixelType::U8x4, PixelData::Rgba),
        PixelData::RgbaPremultiplied(_) => (fr::PixelType::U8x4, PixelData::RgbaPremultiplied),
    };
    let (PixelData::Rgb(buffer) | PixelData::Rgba(buffer) | PixelData::RgbaPremultiplied(buffer)) =
        pixels.data;
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
    Pixels {
        width: to.width,
        height: to.height,
        data: rebuild(dst.into_vec()),
    }
}

/// Apply `orientation` to `pixels` of `size` (as stored).
fn orient(pixels: Pixels, size: Size, orientation: Orientation) -> Pixels {
    let mut image = match pixels.data {
        PixelData::Rgb(rgb) => DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(size.width, size.height, rgb)
                .expect("buffer matches its dimensions"),
        ),
        // Decode pixels are straight; premultiplied pixels are never oriented, but keep the type
        // total by treating them as a four-channel buffer.
        PixelData::Rgba(rgba) | PixelData::RgbaPremultiplied(rgba) => DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(size.width, size.height, rgba)
                .expect("buffer matches its dimensions"),
        ),
    };
    image.apply_orientation(orientation);
    let (width, height) = (image.width(), image.height());
    let data = match image {
        DynamicImage::ImageRgb8(rgb) => PixelData::Rgb(rgb.into_raw()),
        DynamicImage::ImageRgba8(rgba) => PixelData::Rgba(rgba.into_raw()),
        _ => unreachable!("orientation keeps the pixel type"),
    };
    Pixels {
        width,
        height,
        data,
    }
}
