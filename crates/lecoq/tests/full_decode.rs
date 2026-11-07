//! One full-resolution decode shared by the zoom upgrade and the editor (#57, perf-review
//! finding 3). Zooming past the display-size decode and picking an Annotation tool must not each
//! run their own full-resolution decode of the same file: whichever asks first runs the one
//! decode, the other adopts its pixels.

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use egui::Key;
use support::TestApp;

/// Zoom `large-2400x1600.jpg` past its display-size decode. Reports the full-resolution pixels the
/// Document now holds (the shared decode's result) and the size drawn on the canvas.
fn open_large_zoomed() -> TestApp {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    // The headless default is 2048; 8192 lets the full 2400×1600 upload uncapped.
    app.harness.input_mut().max_texture_side = Some(8192);
    let display = app.harness.state().canvas().unwrap().decoded_size;
    assert!(display.width < 2400, "the startup decode is display-size");
    // Fit is about 42%; four steps reach about 102%, past the display-size pixels.
    for _ in 0..4 {
        app.press(Key::Plus);
    }
    app.settle();
    app
}

#[test]
fn zooming_in_then_drawing_decodes_the_full_resolution_once() {
    let mut app = open_large_zoomed();

    // The zoom upgrade ran the one full-resolution decode, so the Document already has its pixels:
    // the decode is shared, not thrown away after the texture upload.
    let zoomed = app
        .doc()
        .full_resolution()
        .cloned()
        .expect("zooming past the display size runs the shared full-resolution decode");
    assert_eq!(
        app.harness
            .state()
            .canvas()
            .unwrap()
            .decoded_size
            .to_string(),
        "2400×1600"
    );
    assert!(!app.harness.state().is_busy());

    // Picking a tool adopts those pixels instead of decoding the file a second time.
    app.harness.key_press(Key::R);
    app.harness.step();
    assert!(
        !app.harness.state().is_busy(),
        "the editor adopts the zoom decode; it starts no second full-resolution decode"
    );
    let drawn = app
        .doc()
        .full_resolution()
        .cloned()
        .expect("the full-resolution pixels are still there");
    assert!(
        Arc::ptr_eq(&zoomed, &drawn),
        "the same pixels: the file was decoded once"
    );
}

#[test]
fn drawing_then_zooming_in_reuses_the_editors_pixels() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    app.harness.input_mut().max_texture_side = Some(8192);
    let display = app.harness.state().canvas().unwrap().decoded_size;
    assert!(display.width < 2400);

    // Picking the Rectangle tool runs the one full-resolution decode.
    app.press(Key::R);
    app.settle();
    let edited = app
        .doc()
        .full_resolution()
        .cloned()
        .expect("the first tool use runs the full-resolution decode");

    // Zooming past the display size must reuse those pixels, not start a second decode. A second
    // decode shows up as the app going busy on the frame the zoom crosses the display size (the
    // upgrade is created synchronously, before its thread is polled).
    let mut went_busy = false;
    for _ in 0..4 {
        app.harness.key_press(Key::Plus);
        app.harness.step();
        went_busy |= app.harness.state().is_busy();
    }
    app.settle();
    assert!(
        !went_busy,
        "zooming after a tool decode reuses the editor's pixels; it starts no second decode"
    );
    assert_eq!(
        app.harness
            .state()
            .canvas()
            .unwrap()
            .decoded_size
            .to_string(),
        "2400×1600",
        "the zoom texture is built from the editor's full-resolution pixels"
    );
    let zoomed = app
        .doc()
        .full_resolution()
        .cloned()
        .expect("the full-resolution pixels are still there");
    assert!(
        Arc::ptr_eq(&edited, &zoomed),
        "the same pixels: the file was decoded once"
    );
}

/// A uniform JPEG just over 100 MP (10 000 × 10 001): too large to edit (§4.7), so neither path
/// decodes it at full resolution. Generated once per target directory (as in `view_only.rs`).
fn huge_jpeg() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("huge-10000x10001.jpg");
    if !path.exists() {
        let (w, h) = (10_000u32, 10_001u32);
        let pixels = image::GrayImage::from_pixel(w, h, image::Luma([128]));
        let tmp = path.with_extension("tmp");
        pixels
            .save_with_format(&tmp, image::ImageFormat::Jpeg)
            .expect("write the huge fixture");
        std::fs::rename(&tmp, &path).expect("move the huge fixture in place");
    }
    path
}

#[test]
fn a_view_only_image_over_the_limit_starts_no_full_resolution_decode() {
    let mut app = TestApp::new();
    let path = app.dir.path().join("huge.jpg");
    std::fs::copy(huge_jpeg(), &path).unwrap();
    app.open(path);
    assert!(app.harness.state().is_view_only());
    app.harness.input_mut().max_texture_side = Some(8192);

    // Zoom in: a view-only Image draws its display-size pixels at any zoom, and no full decode is
    // started (neither by the zoom upgrade nor by the editor).
    for _ in 0..4 {
        app.press(Key::Plus);
    }
    app.settle();

    assert!(!app.harness.state().is_busy());
    assert!(
        app.doc().full_resolution().is_none(),
        "an Image over the full-resolution limit is never decoded at full resolution"
    );
}
