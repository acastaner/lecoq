//! A profiled animated GIF converts its colour the same on the animation path as on the still
//! path, and converts every frame (#60). The animation builds the colour converter once and
//! applies it to each frame; this test checks the result through the public API, observing colour
//! rather than call counts (the ticket's testing decision).

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, Rgba, RgbaImage};
use lcms2::{CIExyY, CIExyYTRIPLE, Profile, ToneCurve};
use lecoq_codec::{PixelData, Size};

const W: u32 = 8;
const H: u32 = 8;

/// An RGB profile that is plainly not sRGB: sRGB's primaries with red and green swapped. A pixel
/// stored "full red" therefore shows green in sRGB, and "full green" shows red, so the conversion
/// is easy to see.
fn red_green_swapped_icc() -> Vec<u8> {
    let white = CIExyY {
        x: 0.3127,
        y: 0.3290,
        Y: 1.0,
    };
    let primaries = CIExyYTRIPLE {
        Red: CIExyY {
            x: 0.30,
            y: 0.60,
            Y: 1.0,
        },
        Green: CIExyY {
            x: 0.64,
            y: 0.33,
            Y: 1.0,
        },
        Blue: CIExyY {
            x: 0.15,
            y: 0.06,
            Y: 1.0,
        },
    };
    let gamma = ToneCurve::new(2.2);
    Profile::new_rgb(&white, &primaries, &[&gamma, &gamma, &gamma])
        .unwrap()
        .icc()
        .unwrap()
}

/// Encode a two-frame GIF of the given flat colours, then splice an ICC profile into the byte
/// stream (the `image` GIF encoder can't write one). Frame 0 is `first`, frame 1 is `second`.
fn profiled_gif(first: Rgba<u8>, second: Rgba<u8>, icc: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for colour in [first, second] {
            encoder
                .encode_frame(Frame::from_parts(
                    RgbaImage::from_pixel(W, H, colour),
                    0,
                    0,
                    Delay::from_numer_denom_ms(100, 1),
                ))
                .unwrap();
        }
    }
    splice_icc(&bytes, icc)
}

/// Insert a GIF ICC application-extension block (`ICCRGBG1012`) right after the logical screen
/// descriptor and its global colour table, where the `gif` reader expects it.
fn splice_icc(gif: &[u8], icc: &[u8]) -> Vec<u8> {
    assert_eq!(&gif[0..6], b"GIF89a", "image writes a GIF89a header");
    let packed = gif[10];
    let gct_len = if packed & 0x80 != 0 {
        3 * (1usize << ((packed & 0x07) + 1))
    } else {
        0
    };
    let insert_at = 13 + gct_len;

    let mut ext = vec![0x21, 0xFF, 0x0B];
    ext.extend_from_slice(b"ICCRGBG1012");
    for chunk in icc.chunks(0xFF) {
        ext.push(chunk.len() as u8);
        ext.extend_from_slice(chunk);
    }
    ext.push(0x00); // block terminator

    let mut out = Vec::with_capacity(gif.len() + ext.len());
    out.extend_from_slice(&gif[..insert_at]);
    out.extend_from_slice(&ext);
    out.extend_from_slice(&gif[insert_at..]);
    out
}

fn write(dir: &std::path::Path, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join("profiled.gif");
    std::fs::write(&path, bytes).unwrap();
    path
}

fn rgba(pixels: &lecoq_codec::Pixels) -> &[u8] {
    match &pixels.data {
        // Display decodes are upload-ready (premultiplied, #56); these fixtures are fully opaque,
        // so the colour channels this test reads are identical either way.
        PixelData::Rgba(buffer) | PixelData::RgbaPremultiplied(buffer) => buffer,
        PixelData::Rgb(_) => panic!("expected an RGBA frame"),
    }
}

/// The first pixel's R, G, B channels.
fn first_pixel(pixels: &lecoq_codec::Pixels) -> [u8; 3] {
    let buffer = rgba(pixels);
    [buffer[0], buffer[1], buffer[2]]
}

#[test]
fn an_animation_frame_converts_colour_the_same_as_the_still_decode() {
    let dir = tempfile::tempdir().unwrap();
    let icc = red_green_swapped_icc();
    let path = write(
        dir.path(),
        &profiled_gif(Rgba([255, 0, 0, 255]), Rgba([0, 255, 0, 255]), &icc),
    );
    let max = Size::new(1000, 1000); // larger than the image: no resize

    // The still decode confirms the GIF carries the profile and decodes frame 0 converted.
    let still = lecoq_codec::decode_for_display(&path, max).unwrap();

    let frames: Vec<_> = lecoq_codec::frames(&path, max)
        .unwrap()
        .expect("an animated GIF")
        .map(|frame| frame.unwrap())
        .collect();
    assert_eq!(frames.len(), 2, "two frames");

    // The animation path's frame 0 matches the still path's decode of the same frame, byte for
    // byte: the shared prepared-once conversion changed nothing about the result.
    assert_eq!(
        rgba(&frames[0].image.pixels),
        rgba(&still.pixels),
        "animation frame 0 must equal the still decode of frame 0"
    );
}

#[test]
fn every_frame_of_a_profiled_animation_is_converted() {
    let dir = tempfile::tempdir().unwrap();
    let icc = red_green_swapped_icc();
    let path = write(
        dir.path(),
        &profiled_gif(Rgba([255, 0, 0, 255]), Rgba([0, 255, 0, 255]), &icc),
    );
    let max = Size::new(1000, 1000);

    let frames: Vec<_> = lecoq_codec::frames(&path, max)
        .unwrap()
        .expect("an animated GIF")
        .map(|frame| frame.unwrap())
        .collect();

    // Stored "full red" is green in sRGB under the swapped profile: green beats red.
    let [r0, g0, _] = first_pixel(&frames[0].image.pixels);
    assert!(
        g0 > r0,
        "frame 0 (stored red) must convert to green-dominant, got r={r0} g={g0}"
    );
    // Stored "full green" is red in sRGB: red beats green. This is the second frame, so the one
    // prepared converter was applied to it too.
    let [r1, g1, _] = first_pixel(&frames[1].image.pixels);
    assert!(
        r1 > g1,
        "frame 1 (stored green) must convert to red-dominant, got r={r1} g={g1}"
    );
}
