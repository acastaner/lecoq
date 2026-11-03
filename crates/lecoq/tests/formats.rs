//! Opening every v1 read format with the right orientation and colours (spec §5.1, §5.2).
//!
//! The fixtures come from `tools/fixtures/make_fixtures.py`: flat colours, so a few pixels tell
//! whether the right thing is on screen.

mod support;

use egui::Color32;
use egui_kittest::kittest::Queryable;
use support::TestApp;

/// The flat colour of the format fixtures.
const COLOUR: Color32 = Color32::from_rgb(40, 120, 200);

/// Lossy formats (JPEG, WebP, AVIF) shift a flat colour a little.
const TOLERANCE: u8 = 12;

fn assert_close(actual: Color32, expected: Color32, what: &str) {
    let close = actual
        .to_array()
        .iter()
        .zip(expected.to_array())
        .all(|(a, e)| a.abs_diff(e) <= TOLERANCE);
    assert!(close, "{what}: got {actual:?}, expected about {expected:?}");
}

fn assert_opens_flat(name: &str, size: &str) {
    let app = TestApp::open_fixture(name);
    assert_eq!(app.status("File name"), name);
    assert_eq!(app.status("Pixel size"), size, "{name}");
    let canvas = app
        .harness
        .state()
        .canvas()
        .expect("an image on the canvas");
    let centre = app
        .pixel(
            canvas.decoded_size.width / 2,
            canvas.decoded_size.height / 2,
        )
        .expect("a pixel");
    assert_close(centre, COLOUR, name);
}

#[test]
fn png_opens() {
    assert_opens_flat("png-64x48.png", "64×48");
}

#[test]
fn webp_opens() {
    assert_opens_flat("webp-64x48.webp", "64×48");
}

#[test]
fn gif_opens() {
    // Animated GIFs play (§3.4, tests/animation.rs), so the pixels on screen change.
    assert_opens_flat("gif-64x48.gif", "64×48");
}

#[test]
fn bmp_opens() {
    assert_opens_flat("bmp-64x48.bmp", "64×48");
}

#[test]
fn ico_opens_its_largest_entry() {
    assert_opens_flat("ico-48x48.ico", "48×48");
}

#[test]
fn tiff_opens() {
    assert_opens_flat("tiff-64x48.tif", "64×48");
}

#[test]
#[cfg_attr(
    not(feature = "avif"),
    ignore = "AVIF needs the `avif` feature (libavif + dav1d)"
)]
fn avif_opens() {
    assert_opens_flat("avif-64x48.avif", "64×48");
}

#[test]
fn transparency_is_kept() {
    let app = TestApp::open_fixture("rgba-64x48.png");
    // Left half opaque COLOUR, right half fully transparent.
    assert_close(app.pixel(10, 24).unwrap(), COLOUR, "opaque half");
    assert_eq!(app.pixel(54, 24).unwrap().a(), 0, "transparent half");
}

#[test]
fn exif_orientation_is_applied_at_decode() {
    // Stored 80×40 (red left, blue right) with orientation 6: shown 40×80, red on top, blue below.
    let app = TestApp::open_fixture("exif-orientation-6.jpg");
    assert_eq!(app.status("Pixel size"), "40×80");
    let canvas = app.harness.state().canvas().unwrap();
    assert_eq!(canvas.decoded_size.to_string(), "40×80");
    assert!(canvas.image.height() > canvas.image.width());
    assert_close(app.pixel(20, 10).unwrap(), Color32::RED, "top");
    assert_close(app.pixel(20, 70).unwrap(), Color32::BLUE, "bottom");
}

#[test]
fn non_srgb_icc_profile_is_converted_to_srgb() {
    // Pure red in a profile with red and green swapped is pure green in sRGB.
    let app = TestApp::open_fixture("red-swapped-icc.jpg");
    assert_close(app.pixel(32, 24).unwrap(), Color32::GREEN, "converted");
}

#[test]
fn srgb_and_untagged_images_keep_their_colours() {
    let app = TestApp::open_fixture("red-srgb-icc.jpg");
    assert_close(app.pixel(32, 24).unwrap(), Color32::RED, "sRGB-tagged");
    let app = TestApp::open_fixture("png-64x48.png");
    assert_close(app.pixel(32, 24).unwrap(), COLOUR, "untagged");
}

#[test]
fn file_in_no_known_format_shows_why() {
    let mut app = TestApp::new();
    let path = app.dir.path().join("notes.png");
    std::fs::write(&path, b"just some text, not an image").unwrap();
    app.open(path);

    app.harness.get_by_label_contains("Can't open notes.png: ");
    assert!(app.harness.state().canvas().is_none());
}

#[test]
fn opening_another_image_after_a_failure_works() {
    let mut app = TestApp::new();
    let broken = app.dir.path().join("broken.webp");
    std::fs::write(&broken, b"RIFF\x10\0\0\0WEBPVP8 truncated").unwrap();
    app.open(broken);
    app.harness
        .get_by_label_contains("Can't open broken.webp: ");

    let good = app.add_fixtures(&["png-64x48.png"]);
    app.open(good);
    assert_eq!(app.status("File name"), "png-64x48.png");
    assert!(app.harness.query_by_label_contains("Can't open").is_none());
    assert!(app.harness.state().canvas().is_some());
}
