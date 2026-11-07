//! The whole-Image composite at display density (#58, §7.2, ADR 0011): up to the display density
//! the composite is rendered once over the whole Image at the display texture's resolution, and the
//! canvas draws the visible sub-rectangle of it. Pan and zoom below the display density then reuse
//! that texture instead of re-rendering the visible region every frame; crossing the display
//! density, or changing the Document, re-renders.
//!
//! These tests assert behaviour through the headless App's composite seam (§9): the composite buffer
//! (`composite()`) and the render counter (`composite_renders()`), a seam added for ADR 0011 that
//! counts actual (re)renders and is not bumped by the reuse path.

mod support;

use egui::{Key, vec2};
use lecoq::Command;
use support::TestApp;

/// Image pixels per physical pixel on screen now (the canvas's on-screen density), read from the
/// canvas seam. Tests run at 1 point = 1 pixel, so the on-screen rect is already in pixels.
fn on_screen_density(app: &TestApp) -> f32 {
    let canvas = app.harness.state().canvas().unwrap();
    canvas.image.width() / app.doc().width() as f32
}

/// The display texture's density (the cutover the whole-Image composite targets): display decode
/// pixels per Image pixel.
fn display_density(app: &TestApp) -> f32 {
    let canvas = app.harness.state().canvas().unwrap();
    canvas.decoded_size.width as f32 / app.doc().width() as f32
}

/// Zoom (around the canvas centre) to land the on-screen density on `target`.
fn zoom_to_density(app: &mut TestApp, target: f32) {
    let around = app.harness.state().canvas().unwrap().area.center();
    let factor = target / on_screen_density(app);
    app.harness
        .state_mut()
        .execute(Command::ZoomAround { factor, around });
    app.settle();
}

/// The large fixture with one Rectangle drawn, then the window shrunk so the display decode (sized
/// for the original, larger canvas) is bigger than the current canvas. This opens a band of zoom
/// where the Image overhangs the canvas (so it pans) yet sits below the display density (so it is
/// the resampled whole-Image composite, not the exact full-resolution one). Returns the app with a
/// verified gap between the Fit density and the display density.
fn annotated_with_a_sub_display_band() -> TestApp {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    app.press(Key::R);
    app.drag_px((900.0, 600.0), (1500.0, 1000.0));
    app.press(Key::Escape);
    app.settle();

    // Shrink the window; the display decode is not re-decoded smaller, so it now overspills the
    // canvas and the Fit density drops below the display density.
    app.resize_window(vec2(560.0, 440.0), false);
    app.settle();

    let fit = on_screen_density(&app);
    let dd = display_density(&app);
    assert!(
        dd > fit * 1.1,
        "the shrunk window must leave a sub-display band: fit {fit}, display {dd}"
    );
    app
}

/// The composite buffer covers the whole Image (it is the display decode's size), not just the
/// visible region, whenever it is drawn below the display density.
#[test]
fn below_the_display_density_the_composite_covers_the_whole_image() {
    let app = annotated_with_a_sub_display_band();
    let composite = app.harness.state().composite().expect("a composite");
    let canvas = app.harness.state().canvas().unwrap();
    assert_eq!(
        (composite.width, composite.height),
        (canvas.decoded_size.width, canvas.decoded_size.height),
        "the composite is the whole Image at the display texture's resolution",
    );
}

/// Panning below the display density moves the Image but does not re-render the composite.
#[test]
fn panning_below_the_display_density_does_not_re_render() {
    let mut app = annotated_with_a_sub_display_band();
    // Sit in the middle of the band: overhanging (so panning moves) and below the display density.
    let fit = on_screen_density(&app);
    let dd = display_density(&app);
    zoom_to_density(&mut app, fit + 0.5 * (dd - fit));

    let before_image = app.harness.state().canvas().unwrap().image;
    let renders = app.harness.state().composite_renders();

    app.harness
        .state_mut()
        .execute(Command::Pan(vec2(-40.0, 0.0)));
    app.settle();

    let after_image = app.harness.state().canvas().unwrap().image;
    assert!(
        (after_image.min.x - before_image.min.x).abs() > 1.0,
        "the pan moved the Image: {before_image:?} -> {after_image:?}"
    );
    assert_eq!(
        app.harness.state().composite_renders(),
        renders,
        "panning below the display density must not re-render the composite",
    );
}

/// Zooming below the display density tracks the wheel without re-rendering the composite.
#[test]
fn zooming_below_the_display_density_does_not_re_render() {
    let mut app = annotated_with_a_sub_display_band();
    let fit = on_screen_density(&app);
    let dd = display_density(&app);
    zoom_to_density(&mut app, fit + 0.3 * (dd - fit));

    let renders = app.harness.state().composite_renders();
    let before = app.harness.state().canvas().unwrap().image.width();

    // Zoom in further, still below the display density.
    zoom_to_density(&mut app, fit + 0.7 * (dd - fit));

    let after = app.harness.state().canvas().unwrap().image.width();
    assert!(after > before + 1.0, "the zoom grew the Image on screen");
    assert_eq!(
        app.harness.state().composite_renders(),
        renders,
        "zooming below the display density must not re-render the composite",
    );
}

/// Crossing the display density switches to the exact full-resolution composite, which re-renders.
#[test]
fn crossing_the_display_density_re_renders() {
    let mut app = annotated_with_a_sub_display_band();
    let dd = display_density(&app);
    zoom_to_density(&mut app, dd * 0.7);
    let renders = app.harness.state().composite_renders();

    // Zoom past the display density (beyond the margin): the full-resolution path takes over.
    zoom_to_density(&mut app, dd * 1.4);

    assert!(
        app.harness.state().composite_renders() > renders,
        "crossing the display density must re-render at full resolution",
    );
}

/// A Document change (a new Annotation) re-renders even below the display density.
#[test]
fn a_document_change_re_renders_below_the_display_density() {
    let mut app = annotated_with_a_sub_display_band();
    let fit = on_screen_density(&app);
    let dd = display_density(&app);
    zoom_to_density(&mut app, fit + 0.5 * (dd - fit));
    let renders = app.harness.state().composite_renders();

    // Draw a second Rectangle: the revision changes, so the composite re-renders.
    app.press(Key::R);
    app.drag_px((400.0, 300.0), (700.0, 500.0));
    app.press(Key::Escape);
    app.settle();

    assert!(
        app.harness.state().composite_renders() > renders,
        "a new Annotation must re-render the composite",
    );
}
