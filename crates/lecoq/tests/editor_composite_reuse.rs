//! Composite buffer reuse (#55, §7.2): the full-render path renders straight into a buffer that is
//! kept between frames, so a view of the same size reuses the allocation instead of allocating and
//! copying three times. These tests assert external behaviour through the composite seam (§9): the
//! reuse is an internal optimisation, so no production seam is added.
//!
//! The hazard the reuse introduces is a stale pixel — the previous frame surviving where the new
//! frame does not draw. The crate-level test in `lecoq-render` locks that at the renderer; here it
//! is checked end to end: panning and zooming an annotated Image and returning to a view must give
//! a composite identical to that view rendered fresh.

mod support;

use egui::{Key, vec2};
use lecoq::Command;
use lecoq_core::{Property, Rgba};
use support::TestApp;

/// The large fixture zoomed in so it overhangs the canvas on both axes (panning is then possible
/// and the visible region is clamped to the viewport, so the composite buffer size stays constant),
/// with a shadowed white Rectangle drawn in the middle of the view.
fn zoomed_with_rectangle() -> TestApp {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    for _ in 0..3 {
        app.press(Key::Plus);
    }
    let canvas = app.harness.state().canvas().unwrap();
    assert!(canvas.image.width() > canvas.area.width() + 200.0);
    assert!(canvas.image.height() > canvas.area.height() + 200.0);

    app.press(Key::R);
    app.drag_px((900.0, 600.0), (1400.0, 1000.0));
    app.harness.state_mut().execute(Command::SetProperty {
        property: Property::FillColour(Some(Rgba::WHITE)),
        merge: false,
    });
    app.settle();
    app.press(Key::Escape);
    app
}

#[test]
fn panning_and_zooming_back_to_a_view_reuses_the_buffer_without_stale_pixels() {
    let mut app = zoomed_with_rectangle();
    let before = app.harness.state().canvas().unwrap().image;
    let fresh = app.composite_image();

    // A zoom round trip (Plus then Minus returns to the same ladder rung, centred on the canvas)
    // and a pan round trip (±delta). Every frame renders into the same kept buffer, so the final
    // composite is produced by the reuse path, over a buffer that held other views in between.
    app.press(Key::Plus);
    app.press(Key::Minus);
    app.harness
        .state_mut()
        .execute(Command::Pan(vec2(-120.0, -80.0)));
    app.settle();
    app.harness
        .state_mut()
        .execute(Command::Pan(vec2(120.0, 80.0)));
    app.settle();

    let after = app.harness.state().canvas().unwrap().image;
    assert!(
        (after.min - before.min).length() < 1.0 && (after.max - before.max).length() < 1.0,
        "the view returned to where it started: {before:?} -> {after:?}"
    );

    // The reuse render of the returned view must match the view rendered fresh, within the golden
    // tolerance: no pixel of the panned/zoomed frames in between may survive.
    let reused = app.composite_image();
    support::assert_golden_image_against(&reused, &fresh);
}

/// Measurement, not a gate (#55): frame time while panning a 2560×1440 canvas with one shadowed
/// Rectangle. Ignored by default; run it on the reference machine and record the number in the
/// release notes:
///
///   cargo test -p lecoq --all-features --test editor_composite_reuse -- --ignored --nocapture
#[test]
#[ignore = "measurement, not a gate: run by hand and record in the release notes"]
fn measure_pan_frame_at_2560x1440() {
    use std::time::Instant;

    let mut app = TestApp::open_fixture_sized("large-2400x1600.jpg", vec2(2560.0, 1440.0));
    // Zoom so the Image overhangs the canvas and panning moves (the visible region then fills the
    // 2560×1440 viewport).
    for _ in 0..4 {
        app.press(Key::Plus);
    }
    // One shadowed Rectangle, the ticket's measurement shape.
    app.press(Key::R);
    app.drag_px((700.0, 500.0), (1700.0, 1100.0));
    app.press(Key::Escape);

    // Time each pan frame: a one-point pan (a view change that takes the full render path) followed
    // by one frame. Alternating direction keeps the Image off its pan clamp.
    let mut frames = Vec::new();
    for i in 0..120 {
        let dx = if i % 2 == 0 { -1.0 } else { 1.0 };
        app.harness.state_mut().execute(Command::Pan(vec2(dx, 0.0)));
        let t = Instant::now();
        app.harness.step();
        frames.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    frames.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = frames[frames.len() / 2];
    let min = frames[0];
    let composite = app.harness.state().composite().expect("a composite");
    eprintln!(
        "pan frame at {}×{}: min {:.2} ms, median {:.2} ms ({} frames)",
        composite.width,
        composite.height,
        min,
        median,
        frames.len(),
    );
}
