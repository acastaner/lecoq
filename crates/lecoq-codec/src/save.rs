//! Saving (§5.1, §5.3): encode PNG, JPEG or WebP, carry the EXIF and ICC data of the source file
//! through, and write atomically (a temporary file in the same directory, then a rename).
//!
//! The pixels handed in are the Flattened Image (§7.2): already oriented (so the EXIF orientation
//! tag is written as 1) and in sRGB when the source profile was converted on decode (so sRGB is
//! written in its place).

use std::fmt;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::metadata::Orientation;
use image::{ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader};

use lecoq_core::{PixelData, Pixels};

use crate::{Size, colour};

/// Lossy WebP quality (libwebp's 0–100 scale). There is no setting for it in v1 (§8.3).
pub const WEBP_QUALITY: f32 = 90.0;

/// The formats lecoq writes (§5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFormat {
    Png,
    Jpeg,
    WebP,
}

impl SaveFormat {
    /// The format a file name asks for, by its extension (case-insensitive). `None` for the
    /// read-only formats (GIF, BMP, ICO, TIFF, AVIF) and anything else.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" | "jpe" | "jfif" => Some(Self::Jpeg),
            "webp" => Some(Self::WebP),
            _ => None,
        }
    }
}

/// What a save carries over from the file the Image came from.
#[derive(Clone, Debug)]
pub struct SourceInfo {
    /// The raw EXIF chunk (TIFF structure, without the JPEG `Exif\0\0` header).
    pub exif: Option<Vec<u8>>,
    /// The embedded ICC profile, as stored in the file.
    pub icc: Option<Vec<u8>>,
    /// Was the source lossless? WebP is then written lossless too, lossy otherwise.
    pub lossless: bool,
}

impl Default for SourceInfo {
    /// No source file (an Image from the clipboard): no metadata, lossless.
    fn default() -> Self {
        Self {
            exif: None,
            icc: None,
            lossless: true,
        }
    }
}

/// Read what a save keeps from `path`'s headers. Anything unreadable is simply left out.
pub fn source_info(path: &Path) -> SourceInfo {
    let Ok(reader) = ImageReader::open(path).and_then(|r| r.with_guessed_format()) else {
        return SourceInfo::default();
    };
    let format = reader.format();
    let lossless = match format {
        Some(ImageFormat::Jpeg | ImageFormat::Avif) => false,
        Some(ImageFormat::WebP) => webp_is_lossless(path),
        _ => true,
    };
    if format == Some(ImageFormat::Avif) {
        return SourceInfo {
            exif: None,
            icc: avif_icc(path),
            lossless,
        };
    }
    let Ok(mut decoder) = reader.into_decoder() else {
        return SourceInfo {
            lossless,
            ..SourceInfo::default()
        };
    };
    SourceInfo {
        icc: decoder.icc_profile().ok().flatten(),
        exif: decoder.exif_metadata().ok().flatten(),
        lossless,
    }
}

#[cfg(feature = "avif")]
fn avif_icc(path: &Path) -> Option<Vec<u8>> {
    crate::decode(path).ok().and_then(|d| d.icc)
}

#[cfg(not(feature = "avif"))]
fn avif_icc(_path: &Path) -> Option<Vec<u8>> {
    None
}

/// Why a save failed: the `<reason>` of "Can't save <name>: <reason>". Its `Display` is the
/// library's (or the OS's) own reason; the app words the cases it names itself (§8.6).
#[derive(Debug)]
pub enum SaveError {
    /// An encoder or the file system failed: its own reason, passed through.
    Library(String),
    /// libwebp refused the pixels: its error code.
    WebPEncoding(String),
}

impl SaveError {
    fn new(reason: impl fmt::Display) -> Self {
        Self::Library(reason.to_string())
    }
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library(reason) | Self::WebPEncoding(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for SaveError {}

/// Encode `pixels` (sRGB, straight alpha, `size`) as `format` and write them to `path` atomically,
/// with the EXIF (orientation set to 1) and ICC data of `source` (§5.3).
pub fn save(
    path: &Path,
    pixels: &Pixels,
    size: Size,
    format: SaveFormat,
    source: &SourceInfo,
    jpeg_quality: u8,
) -> Result<(), SaveError> {
    let exif = source.exif.clone().map(|mut exif| {
        let _ = Orientation::remove_from_exif_chunk(&mut exif);
        exif
    });
    let icc = colour::output_profile(source.icc.as_deref());
    let encoded = match format {
        SaveFormat::Png => encode_png(pixels, size, icc, exif)?,
        SaveFormat::Jpeg => encode_jpeg(pixels, size, icc, exif, jpeg_quality)?,
        SaveFormat::WebP => encode_webp(pixels, size, icc, exif, source.lossless)?,
    };
    write_atomically(path, &encoded).map_err(SaveError::new)
}

fn color_type(pixels: &Pixels) -> (&[u8], ExtendedColorType) {
    match &pixels.data {
        PixelData::Rgb(rgb) => (rgb, ExtendedColorType::Rgb8),
        PixelData::Rgba(rgba) => (rgba, ExtendedColorType::Rgba8),
        PixelData::RgbaPremultiplied(rgba) => (rgba, ExtendedColorType::Rgba8),
    }
}

/// Encode `pixels` (sRGB, straight alpha, `size`) as a PNG with no metadata: what copying the Image
/// puts on the clipboard (§6.2).
pub fn encode_png_plain(pixels: &Pixels, size: Size) -> Result<Vec<u8>, SaveError> {
    encode_png(pixels, size, None, None)
}

fn encode_png(
    pixels: &Pixels,
    size: Size,
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
) -> Result<Vec<u8>, SaveError> {
    let mut out = Vec::new();
    let mut encoder = PngEncoder::new(&mut out);
    if let Some(icc) = icc {
        encoder.set_icc_profile(icc).map_err(SaveError::new)?;
    }
    if let Some(exif) = exif {
        encoder.set_exif_metadata(exif).map_err(SaveError::new)?;
    }
    let (data, color) = color_type(pixels);
    encoder
        .write_image(data, size.width, size.height, color)
        .map_err(SaveError::new)?;
    Ok(out)
}

fn encode_jpeg(
    pixels: &Pixels,
    size: Size,
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    quality: u8,
) -> Result<Vec<u8>, SaveError> {
    // JPEG has no alpha: transparent areas become white.
    let rgb;
    let data = match &pixels.data {
        PixelData::Rgb(rgb) => rgb.as_slice(),
        PixelData::Rgba(rgba) | PixelData::RgbaPremultiplied(rgba) => {
            rgb = over_white(rgba);
            rgb.as_slice()
        }
    };
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
    if let Some(icc) = icc {
        encoder.set_icc_profile(icc).map_err(SaveError::new)?;
    }
    if let Some(exif) = exif {
        encoder.set_exif_metadata(exif).map_err(SaveError::new)?;
    }
    encoder
        .write_image(data, size.width, size.height, ExtendedColorType::Rgb8)
        .map_err(SaveError::new)?;
    Ok(out)
}

fn over_white(rgba: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for px in rgba.chunks_exact(4) {
        let a = u32::from(px[3]);
        for &c in &px[..3] {
            rgb.push(((u32::from(c) * a + 255 * (255 - a) + 127) / 255) as u8);
        }
    }
    rgb
}

fn encode_webp(
    pixels: &Pixels,
    size: Size,
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    lossless: bool,
) -> Result<Vec<u8>, SaveError> {
    let encoder = match &pixels.data {
        PixelData::Rgb(rgb) => webp::Encoder::from_rgb(rgb, size.width, size.height),
        PixelData::Rgba(rgba) | PixelData::RgbaPremultiplied(rgba) => {
            webp::Encoder::from_rgba(rgba, size.width, size.height)
        }
    };
    let encoded = encoder
        .encode_simple(lossless, WEBP_QUALITY)
        .map_err(|e| SaveError::WebPEncoding(format!("{e:?}")))?;
    Ok(webp_container(
        &encoded,
        size,
        !matches!(pixels.data, PixelData::Rgb(_)),
        icc.as_deref(),
        exif.as_deref(),
    ))
}

/// The chunks of a RIFF/WebP file, after its 12-byte header: (fourcc, payload).
fn riff_chunks(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut chunks = Vec::new();
    let mut at = 12;
    while at + 8 <= data.len() {
        let fourcc: [u8; 4] = data[at..at + 4].try_into().expect("4 bytes");
        let len = u32::from_le_bytes(data[at + 4..at + 8].try_into().expect("4 bytes")) as usize;
        let end = (at + 8 + len).min(data.len());
        chunks.push((fourcc, &data[at + 8..end]));
        at = at + 8 + len + (len & 1);
    }
    chunks
}

/// Does this WebP file hold a lossless (VP8L) bitstream?
fn webp_is_lossless(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|data| {
        data.len() > 12
            && &data[8..12] == b"WEBP"
            && riff_chunks(&data)
                .iter()
                .any(|(fourcc, _)| fourcc == b"VP8L")
    })
}

/// Rebuild libwebp's output as an extended WebP file (VP8X) carrying `icc` and `exif`. libwebp's
/// simple encoder writes no metadata; this is what `WebPMux` would assemble.
fn webp_container(
    encoded: &[u8],
    size: Size,
    alpha: bool,
    icc: Option<&[u8]>,
    exif: Option<&[u8]>,
) -> Vec<u8> {
    if icc.is_none() && exif.is_none() {
        return encoded.to_vec();
    }
    let mut flags = 0u8;
    if icc.is_some() {
        flags |= 0x20;
    }
    if alpha {
        flags |= 0x10;
    }
    if exif.is_some() {
        flags |= 0x08;
    }
    let mut vp8x = vec![flags, 0, 0, 0];
    vp8x.extend_from_slice(&(size.width - 1).to_le_bytes()[..3]);
    vp8x.extend_from_slice(&(size.height - 1).to_le_bytes()[..3]);

    let mut body = b"WEBP".to_vec();
    let mut chunk = |fourcc: &[u8; 4], payload: &[u8]| {
        body.extend_from_slice(fourcc);
        body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        body.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            body.push(0);
        }
    };
    chunk(b"VP8X", &vp8x);
    if let Some(icc) = icc {
        chunk(b"ICCP", icc);
    }
    for (fourcc, payload) in riff_chunks(encoded) {
        if &fourcc != b"VP8X" {
            chunk(&fourcc, payload);
        }
    }
    if let Some(exif) = exif {
        chunk(b"EXIF", exif);
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Write `data` to a temporary file next to `path`, flush it to disk, then rename it over `path`
/// (§5.3). An existing file keeps its permissions; a new one gets the usual ones (umask applies).
fn write_atomically(path: &Path, data: &[u8]) -> io::Result<()> {
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let existing = std::fs::metadata(path).ok().map(|m| m.permissions());
    let mut builder = tempfile::Builder::new();
    builder.prefix(".lecoq-save-").suffix(".tmp");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut tmp = builder.tempfile_in(dir)?;
    {
        let mut writer = BufWriter::new(tmp.as_file_mut());
        writer.write_all(data)?;
        writer.flush()?;
    }
    if let Some(permissions) = existing {
        tmp.as_file().set_permissions(permissions)?;
    }
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
