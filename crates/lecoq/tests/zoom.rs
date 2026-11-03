//! Zoom and fullscreen (spec §2, §3.1, §7.2).

mod support;

use egui::Key;
use support::TestApp;

#[test]
fn plus_and_minus_step_the_zoom_shown_in_the_status_bar() {
    // A small Image fits at 100%, so the steps land on round numbers.
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    assert_eq!(app.status("Zoom"), "Fit");

    app.press(Key::Plus);
    assert_eq!(app.status("Zoom"), "125%");

    app.press(Key::Minus);
    app.press(Key::Minus);
    assert_eq!(app.status("Zoom"), "80%");
}

#[test]
fn plus_zooms_around_the_centre() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    let fit = app.harness.state().canvas().unwrap().image;

    app.press(Key::Plus);

    let canvas = app.harness.state().canvas().unwrap();
    assert!((canvas.image.width() / fit.width() - 1.25).abs() < 0.01);
    assert!((canvas.image.center() - canvas.area.center()).length() < 1.0);
}

#[test]
fn zero_returns_to_fit() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    let fit = app.harness.state().canvas().unwrap().image;

    app.press(Key::Plus);
    app.press(Key::Plus);
    app.press(Key::Num0);

    assert_eq!(app.status("Zoom"), "Fit");
    assert_eq!(app.harness.state().canvas().unwrap().image, fit);
}

#[test]
fn every_new_image_opens_fit() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::Plus);
    assert_eq!(app.status("Zoom"), "125%");

    let other = app.add_fixtures(&["large-2400x1600.jpg"]);
    app.open(other);

    assert_eq!(app.status("Zoom"), "Fit");
}

#[test]
fn ctrl_wheel_zooms_around_the_cursor() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    let before = app.harness.state().canvas().unwrap().image;
    let cursor = egui::pos2(300.0, 250.0);
    // Where the cursor is on the Image, as a fraction of its size.
    let at = (cursor - before.min) / before.size();

    app.ctrl_wheel_at(cursor, 2.0);

    let after = app.harness.state().canvas().unwrap().image;
    assert!(
        after.width() > before.width() * 1.05,
        "{before:?} → {after:?}"
    );
    assert!(app.status("Zoom").ends_with('%'));
    let at_after = (cursor - after.min) / after.size();
    assert!((at_after - at).length() < 0.005, "{at:?} → {at_after:?}");
}

#[test]
fn f11_toggles_fullscreen_and_hides_the_status_bar() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    let windowed = app.harness.state().canvas().unwrap().area;

    app.press(Key::F11);
    assert!(app.harness.state().is_fullscreen());
    assert_eq!(app.status_field("Zoom"), None);
    assert_eq!(app.status_field("File name"), None);
    let full = app.harness.state().canvas().unwrap().area;
    assert!(
        full.height() > windowed.height(),
        "the canvas takes the status bar's room"
    );

    app.press(Key::F11);
    assert!(!app.harness.state().is_fullscreen());
    assert_eq!(app.status("Zoom"), "Fit");
}

#[test]
fn f11_hides_the_menu_bar_too() {
    use egui_kittest::kittest::Queryable;

    let mut app = TestApp::open_fixture("small-320x240.jpg");
    assert!(app.harness.query_by_label("View").is_some());

    app.press(Key::F11);
    assert!(
        app.harness.query_by_label("View").is_none(),
        "fullscreen hides all chrome (§2)"
    );

    app.press(Key::F11);
    assert!(app.harness.query_by_label("View").is_some());
}

#[test]
fn zooming_past_the_display_size_decode_brings_full_resolution() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    // The headless default is 2048; a real GL context reports 8192 or more.
    app.harness.input_mut().max_texture_side = Some(8192);
    let display = app.harness.state().canvas().unwrap().decoded_size;
    assert!(display.width < 2400);

    // Fit is about 42%; four steps reach about 102%, past the display-size pixels.
    for _ in 0..4 {
        app.press(Key::Plus);
    }

    let canvas = app.harness.state().canvas().unwrap();
    assert_eq!(canvas.decoded_size.to_string(), "2400×1600");

    // Back at Fit the display-size pixels are drawn again.
    app.press(Key::Num0);
    assert_eq!(app.harness.state().canvas().unwrap().decoded_size, display);
}

#[test]
fn hidpi_screen_redecodes_for_its_physical_pixels() {
    // The startup decode targets the configured window size at scale 1 (1600×1000 → 1500×1000);
    // at 2 pixels per point the canvas is twice as many physical pixels, and the redecode fits
    // the 3:2 Image into that.
    let mut app = TestApp::launch_scaled(&["large-2400x1600.jpg"], 2.0);
    app.settle();

    let canvas = app.harness.state().canvas().unwrap();
    let physical = canvas.area.size() * 2.0;
    let expected = physical.x.min(physical.y * 1.5).round() as u32;
    assert!(expected > 1500, "{physical:?}");
    assert_eq!(canvas.decoded_size.width, expected);
    assert_eq!(app.status("Zoom"), "Fit");
}
