//! AVIF read through libavif + dav1d (§5.1).
//!
//! The `irot`/`imir` boxes are AVIF's orientation (§5.2): they're applied here, at full
//! resolution, since they're rare. The ICC profile goes through the same sRGB conversion as every
//! other format. CICP colour primaries other than sRGB's aren't converted (only ICC is, §5.2).

use std::ffi::CStr;

use image::DynamicImage;
use image::metadata::Orientation;
use libavif_sys as sys;

use crate::{DecodeError, Decoded, Pixels, Size};

/// Owns a libavif decoder and image; frees both on drop.
struct Handles {
    decoder: *mut sys::avifDecoder,
    image: *mut sys::avifImage,
}

impl Drop for Handles {
    fn drop(&mut self) {
        // SAFETY: both were created by libavif (or are null, which libavif's destroy functions
        // don't accept, hence the checks) and are destroyed once.
        unsafe {
            if !self.image.is_null() {
                sys::avifImageDestroy(self.image);
            }
            if !self.decoder.is_null() {
                sys::avifDecoderDestroy(self.decoder);
            }
        }
    }
}

/// Owns an `avifRGBImage`'s pixel buffer; frees it on drop.
struct RgbBuffer(sys::avifRGBImage);

impl Drop for RgbBuffer {
    fn drop(&mut self) {
        // SAFETY: the pixels were allocated by `avifRGBImageAllocatePixels` (or are null, which it
        // accepts).
        unsafe { sys::avifRGBImageFreePixels(&mut self.0) };
    }
}

fn check(result: sys::avifResult) -> Result<(), DecodeError> {
    if result == sys::AVIF_RESULT_OK {
        return Ok(());
    }
    // SAFETY: libavif returns a static, NUL-terminated string for every result code.
    let reason = unsafe { CStr::from_ptr(sys::avifResultToString(result)) };
    Err(DecodeError::new(reason.to_string_lossy()))
}

/// Decode the primary image of an AVIF file to 8-bit RGB(A).
pub(crate) fn decode(data: &[u8]) -> Result<Decoded, DecodeError> {
    // SAFETY: every pointer comes from libavif and is checked for null before use; `Handles` and
    // `RgbBuffer` free them. `data` outlives the call that reads it.
    let (pixels, size, icc, rotation, mirror) = unsafe {
        let handles = Handles {
            decoder: sys::avifDecoderCreate(),
            image: sys::avifImageCreateEmpty(),
        };
        if handles.decoder.is_null() || handles.image.is_null() {
            return Err(DecodeError::new("out of memory"));
        }
        (*handles.decoder).maxThreads =
            std::thread::available_parallelism().map_or(1, |n| n.get().min(4) as i32);
        check(sys::avifDecoderReadMemory(
            handles.decoder,
            handles.image,
            data.as_ptr(),
            data.len(),
        ))?;
        let image = &*handles.image;
        let alpha = !image.alphaPlane.is_null();

        let mut rgb = RgbBuffer(sys::avifRGBImage::default());
        sys::avifRGBImageSetDefaults(&mut rgb.0, handles.image);
        rgb.0.depth = 8;
        rgb.0.format = if alpha {
            sys::AVIF_RGB_FORMAT_RGBA
        } else {
            sys::AVIF_RGB_FORMAT_RGB
        };
        check(sys::avifRGBImageAllocatePixels(&mut rgb.0))?;
        check(sys::avifImageYUVToRGB(handles.image, &mut rgb.0))?;

        let size = Size::new(rgb.0.width, rgb.0.height);
        let channels = if alpha { 4 } else { 3 };
        let row = size.width as usize * channels;
        let mut buffer = Vec::with_capacity(row * size.height as usize);
        for y in 0..size.height as usize {
            let start = rgb.0.pixels.add(y * rgb.0.rowBytes as usize);
            buffer.extend_from_slice(std::slice::from_raw_parts(start, row));
        }
        let pixels = if alpha {
            Pixels::Rgba(buffer)
        } else {
            Pixels::Rgb(buffer)
        };
        let icc = (!image.icc.data.is_null() && image.icc.size > 0)
            .then(|| std::slice::from_raw_parts(image.icc.data, image.icc.size).to_vec());
        let rotation =
            (image.transformFlags & sys::AVIF_TRANSFORM_IROT != 0).then_some(image.irot.angle % 4);
        let mirror =
            (image.transformFlags & sys::AVIF_TRANSFORM_IMIR != 0).then_some(image.imir.axis);
        (pixels, size, icc, rotation, mirror)
    };

    let (pixels, size) = transform(pixels, size, rotation, mirror);
    Ok(Decoded {
        pixels,
        stored: size,
        orientation: Orientation::NoTransforms,
        icc,
        may_animate: false,
    })
}

/// Apply `irot` (anticlockwise quarter turns), then `imir` (axis 0 exchanges top and bottom, 1
/// left and right), in that order (ISO/IEC 23008-12).
fn transform(
    pixels: Pixels,
    size: Size,
    rotation: Option<u8>,
    mirror: Option<u8>,
) -> (Pixels, Size) {
    if rotation.unwrap_or(0) == 0 && mirror.is_none() {
        return (pixels, size);
    }
    let mut image = match pixels {
        Pixels::Rgb(rgb) => DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(size.width, size.height, rgb).expect("matching buffer"),
        ),
        Pixels::Rgba(rgba) => DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(size.width, size.height, rgba).expect("matching buffer"),
        ),
    };
    image = match rotation.unwrap_or(0) {
        1 => image.rotate270(),
        2 => image.rotate180(),
        3 => image.rotate90(),
        _ => image,
    };
    image = match mirror {
        Some(0) => image.flipv(),
        Some(1) => image.fliph(),
        _ => image,
    };
    let size = Size::new(image.width(), image.height());
    let pixels = match image {
        DynamicImage::ImageRgb8(rgb) => Pixels::Rgb(rgb.into_raw()),
        DynamicImage::ImageRgba8(rgba) => Pixels::Rgba(rgba.into_raw()),
        _ => unreachable!("rotation and mirroring keep the pixel type"),
    };
    (pixels, size)
}
