//! Opening a JPEG (spec §2 status bar, §5.2 display-size decode).

mod support;

use egui_kittest::kittest::Queryable;
use support::TestApp;

#[test]
fn status_bar_shows_file_name_pixel_size_and_fit() {
    let app = TestApp::open_fixture("small-320x240.jpg");

    assert_eq!(app.status("File name"), "small-320x240.jpg");
    assert_eq!(app.status("Pixel size"), "320×240");
    assert_eq!(app.status("Zoom"), "Fit");
}

#[test]
fn large_image_is_decoded_at_display_size_and_fit_to_the_canvas() {
    let app = TestApp::open_fixture("large-2400x1600.jpg");

    assert_eq!(app.status("Pixel size"), "2400×1600");
    let canvas = app
        .harness
        .state()
        .canvas()
        .expect("an image on the canvas");
    // Decoded no larger than the canvas, aspect ratio kept (3:2).
    assert!(canvas.decoded_size.width <= canvas.area.width() as u32);
    assert!(canvas.decoded_size.height <= canvas.area.height() as u32);
    let (w, h) = (canvas.decoded_size.width, canvas.decoded_size.height);
    assert!((w * 2).abs_diff(h * 3) <= 3, "{w}×{h} isn't 3:2");
    // Fit: inside the canvas, touching it on one axis, centred.
    let shown = canvas.image;
    assert!(canvas.area.contains_rect(shown));
    assert!(
        (shown.width() - canvas.area.width()).abs() < 1.0
            || (shown.height() - canvas.area.height()).abs() < 1.0
    );
    assert!((shown.center() - canvas.area.center()).length() < 1.0);
}

#[test]
fn undecodable_file_shows_why_on_the_canvas() {
    let mut app = TestApp::new();
    let path = app.dir.path().join("broken.jpg");
    std::fs::write(&path, b"\xFF\xD8 this is not really a JPEG").unwrap();
    app.open(path);

    app.harness.get_by_label_contains("Can't open broken.jpg: ");
    assert!(app.harness.state().canvas().is_none());
}

#[test]
fn small_image_is_never_enlarged() {
    let app = TestApp::open_fixture("small-320x240.jpg");

    let canvas = app
        .harness
        .state()
        .canvas()
        .expect("an image on the canvas");
    assert_eq!(canvas.decoded_size.to_string(), "320×240");
    assert_eq!(canvas.image.size(), egui::vec2(320.0, 240.0));
}
