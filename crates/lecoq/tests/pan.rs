//! Panning a zoomed Image (#43, follow-up to spec §3.1).

mod support;

use egui::{Key, PointerButton, pos2, vec2};
use support::TestApp;

/// The large fixture, zoomed in three steps (about 82%): larger than the canvas on both axes.
fn zoomed() -> TestApp {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    for _ in 0..3 {
        app.press(Key::Plus);
    }
    let canvas = app.harness.state().canvas().unwrap();
    assert!(canvas.image.width() > canvas.area.width() + 200.0);
    assert!(canvas.image.height() > canvas.area.height() + 200.0);
    app
}

fn image(app: &TestApp) -> egui::Rect {
    app.harness.state().canvas().unwrap().image
}

fn close(a: egui::Vec2, b: egui::Vec2) -> bool {
    (a - b).length() < 1.01
}

#[test]
fn space_and_left_drag_pans_a_zoomed_image() {
    let mut app = zoomed();
    let before = image(&app);

    app.drag(
        PointerButton::Primary,
        pos2(500.0, 300.0),
        pos2(400.0, 250.0),
        true,
    );

    let moved = image(&app).min - before.min;
    assert!(close(moved, vec2(-100.0, -50.0)), "moved by {moved:?}");
}

#[test]
fn middle_drag_pans_a_zoomed_image() {
    let mut app = zoomed();
    let before = image(&app);

    app.drag(
        PointerButton::Middle,
        pos2(500.0, 300.0),
        pos2(560.0, 340.0),
        false,
    );

    let moved = image(&app).min - before.min;
    assert!(close(moved, vec2(60.0, 40.0)), "moved by {moved:?}");
}

#[test]
fn a_left_drag_without_space_does_not_pan() {
    let mut app = zoomed();
    let before = image(&app);

    app.drag(
        PointerButton::Primary,
        pos2(500.0, 300.0),
        pos2(400.0, 250.0),
        false,
    );

    assert_eq!(image(&app), before);
}

#[test]
fn the_pan_stops_where_the_image_edge_meets_the_canvas_edge() {
    let mut app = zoomed();

    // Further than the Image overhangs: its top-left corner stops at the canvas's.
    for _ in 0..3 {
        app.drag(
            PointerButton::Middle,
            pos2(100.0, 100.0),
            pos2(900.0, 650.0),
            false,
        );
    }
    let canvas = app.harness.state().canvas().unwrap();
    assert!(close(canvas.image.min.to_vec2(), canvas.area.min.to_vec2()));

    // And the other way, the bottom-right corner.
    for _ in 0..3 {
        app.drag(
            PointerButton::Middle,
            pos2(900.0, 650.0),
            pos2(100.0, 100.0),
            false,
        );
    }
    let canvas = app.harness.state().canvas().unwrap();
    assert!(close(canvas.image.max.to_vec2(), canvas.area.max.to_vec2()));
}

#[test]
fn an_image_narrower_than_the_canvas_stays_centred_on_that_axis() {
    // 280% makes 320×240 about 898×674: narrower than the canvas (the window less the tool strip),
    // but taller.
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    for _ in 0..4 {
        app.press(Key::Plus);
    }
    let centre = app.harness.state().canvas().unwrap().area.center();
    app.harness.state_mut().execute(lecoq::Command::ZoomAround {
        factor: 1.15,
        around: centre,
    });
    app.settle();
    let before = app.harness.state().canvas().unwrap();
    assert!(before.image.width() < before.area.width());
    assert!(before.image.height() > before.area.height());

    app.drag(
        PointerButton::Middle,
        pos2(500.0, 300.0),
        pos2(300.0, 290.0),
        false,
    );

    let after = image(&app);
    assert!((after.center().x - before.area.center().x).abs() < 1.0);
    assert!(close(after.min - before.image.min, vec2(0.0, -10.0)));
}

#[test]
fn panning_at_fit_does_nothing() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    let before = image(&app);

    app.drag(
        PointerButton::Middle,
        pos2(500.0, 300.0),
        pos2(300.0, 200.0),
        false,
    );

    assert_eq!(image(&app), before);
    assert_eq!(app.status("Zoom"), "Fit");
}

#[test]
fn shift_wheel_pans_horizontally() {
    let mut app = zoomed();
    let before = image(&app);

    // One notch down scrolls the view right: the Image moves left by one line step (40 points).
    app.wheel_at(pos2(500.0, 300.0), -1.0, egui::Modifiers::SHIFT);

    let moved = image(&app).min - before.min;
    assert!(close(moved, vec2(-40.0, 0.0)), "moved by {moved:?}");
}

/// The large fixture copied under three names and opened in the middle, then zoomed in (larger than
/// the canvas on both axes), so a plain scroll pans and the Sibling position can be read.
fn zoomed_in_folder() -> TestApp {
    let mut app =
        TestApp::open_in_folder("large-2400x1600.jpg", &["a.jpg", "b.jpg", "c.jpg"], "b.jpg");
    for _ in 0..3 {
        app.press(Key::Plus);
    }
    let canvas = app.harness.state().canvas().unwrap();
    assert!(canvas.image.width() > canvas.area.width() + 200.0);
    assert!(canvas.image.height() > canvas.area.height() + 200.0);
    app
}

#[test]
fn a_plain_scroll_pans_a_zoomed_image_and_keeps_the_sibling() {
    let mut app = zoomed_in_folder();
    let before = image(&app);
    assert_eq!(app.status("File name"), "b.jpg");

    // One notch down: the view moves down the Image, so the Image shifts up by one line step
    // (40 points). The shown Sibling does not change (§3.1: scroll pans when zoomed).
    app.wheel_at(pos2(500.0, 300.0), -1.0, egui::Modifiers::NONE);

    let moved = image(&app).min - before.min;
    assert!(close(moved, vec2(0.0, -40.0)), "moved by {moved:?}");
    assert_eq!(app.status("File name"), "b.jpg", "scroll must not navigate");
}

#[test]
fn a_two_axis_trackpad_scroll_pans_both_axes_when_zoomed() {
    let mut app = zoomed_in_folder();
    let before = image(&app);

    // A trackpad two-finger scroll arrives as a point-unit wheel with both deltas; it pans on both
    // axes at a 1:1 point ratio, tracking the gesture.
    app.scroll_at(pos2(500.0, 300.0), vec2(-60.0, -40.0));

    let moved = image(&app).min - before.min;
    assert!(close(moved, vec2(-60.0, -40.0)), "moved by {moved:?}");
    assert_eq!(app.status("File name"), "b.jpg", "scroll must not navigate");
}

#[test]
fn at_fit_a_plain_scroll_still_navigates_siblings() {
    let mut app =
        TestApp::open_in_folder("large-2400x1600.jpg", &["a.jpg", "b.jpg", "c.jpg"], "b.jpg");
    assert_eq!(app.status("Zoom"), "Fit");

    // At Fit the Image is not larger than the canvas, so scroll navigates exactly as today.
    app.wheel_at(pos2(500.0, 300.0), -1.0, egui::Modifiers::NONE);
    assert_eq!(app.status("File name"), "c.jpg");

    app.wheel_at(pos2(500.0, 300.0), 1.0, egui::Modifiers::NONE);
    assert_eq!(app.status("File name"), "b.jpg");
}

#[test]
fn a_scroll_shifts_the_composited_view() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    // An Annotation so the canvas composites through Skia (§7.2); the composite is what the user
    // sees, and panning moves which Image pixels it shows.
    app.press(Key::R);
    app.drag_px((900.0, 600.0), (1500.0, 1000.0));
    app.press(Key::Escape);
    for _ in 0..3 {
        app.press(Key::Plus);
    }

    // Scroll up and left: the view moves toward the top-left of the Image.
    app.scroll_at(pos2(500.0, 300.0), vec2(200.0, 150.0));

    app.assert_golden("scroll-panned");
}

#[test]
fn arrow_keys_navigate_siblings_while_zoomed() {
    let mut app = zoomed_in_folder();
    assert_eq!(app.status("File name"), "b.jpg");

    // Arrow keys always move through the Siblings, zoomed or not, so navigation is never lost.
    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "c.jpg");

    app.press(Key::ArrowLeft);
    app.press(Key::ArrowLeft);
    assert_eq!(app.status("File name"), "a.jpg");
}

#[test]
fn zero_resets_the_pan() {
    let mut app = zoomed();
    app.drag(
        PointerButton::Middle,
        pos2(500.0, 300.0),
        pos2(300.0, 200.0),
        false,
    );

    app.press(Key::Num0);
    app.press(Key::Plus);

    let canvas = app.harness.state().canvas().unwrap();
    assert!((canvas.image.center() - canvas.area.center()).length() < 1.0);
}

#[test]
fn opening_an_image_resets_the_pan() {
    let mut app = zoomed();
    app.drag(
        PointerButton::Middle,
        pos2(500.0, 300.0),
        pos2(300.0, 200.0),
        false,
    );

    let path = app.add_fixtures(&["large-2400x1600.jpg"]);
    app.open(path);
    app.press(Key::Plus);

    let canvas = app.harness.state().canvas().unwrap();
    assert!((canvas.image.center() - canvas.area.center()).length() < 1.0);
}
