//! The display decode lands in upload-ready pixels, produced here (on the decode pool, #56) and
//! not on the UI thread: premultiplied RGBA in egui's layout. Full-resolution pixels stay straight
//! (that is #57's path and the save encoders' contract); this test only covers the display decode.

use std::io::Cursor;

use image::{ImageFormat, RgbImage, RgbaImage};
use lecoq_codec::{PixelData, Size};

/// A box far larger than the fixtures, so `into_display` never resizes them: the only transform
/// under test is the premultiply.
const BOX: Size = Size::new(1000, 1000);

fn png_bytes(encode: impl FnOnce(&mut Cursor<Vec<u8>>)) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    encode(&mut cursor);
    cursor.into_inner()
}

#[test]
fn an_opaque_image_is_upload_ready_rgba_with_alpha_255() {
    // A 2×2 RGB image (no alpha channel): the opaque, common case.
    let mut img = RgbImage::new(2, 2);
    img.put_pixel(0, 0, image::Rgb([10, 120, 200]));
    img.put_pixel(1, 0, image::Rgb([40, 50, 60]));
    img.put_pixel(0, 1, image::Rgb([1, 2, 3]));
    img.put_pixel(1, 1, image::Rgb([250, 240, 230]));
    let bytes = png_bytes(|c| img.write_to(c, ImageFormat::Png).unwrap());

    let display = lecoq_codec::decode_bytes(&bytes).unwrap().into_display(BOX);

    let PixelData::RgbaPremultiplied(data) = &display.pixels.data else {
        panic!("display pixels must be upload-ready (premultiplied RGBA), not decode-format");
    };
    // Grown by a third: RGB (12 bytes) became RGBA (16 bytes).
    assert_eq!(data.len(), 2 * 2 * 4);
    assert_eq!(display.pixels.byte_len(), 2 * 2 * 4);
    // Opaque: the colour bytes are the source's, alpha is 255 (straight == premultiplied).
    assert_eq!(&data[0..4], &[10, 120, 200, 255]);
    assert_eq!(&data[4..8], &[40, 50, 60, 255]);
}

#[test]
fn a_translucent_image_is_premultiplied_exactly() {
    // A 2×1 RGBA image: one opaque pixel, one half-transparent.
    let mut img = RgbaImage::new(2, 1);
    img.put_pixel(0, 0, image::Rgba([10, 120, 200, 255]));
    img.put_pixel(1, 0, image::Rgba([200, 100, 50, 128]));
    let bytes = png_bytes(|c| img.write_to(c, ImageFormat::Png).unwrap());

    let display = lecoq_codec::decode_bytes(&bytes).unwrap().into_display(BOX);

    let PixelData::RgbaPremultiplied(data) = &display.pixels.data else {
        panic!("display pixels must be upload-ready (premultiplied RGBA)");
    };
    // The opaque pixel is unchanged. The half-transparent pixel is premultiplied by egui's exact
    // `mul_frac_round` (worked by hand, not recomputed the code's way): 200·128/255 rounds to 100,
    // 100·128/255 to 50, 50·128/255 to 25, alpha kept.
    assert_eq!(&data[0..4], &[10, 120, 200, 255]);
    assert_eq!(&data[4..8], &[100, 50, 25, 128]);
}
