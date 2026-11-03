//! Handle hover highlight (#49): the selection or crop handle under the pointer grows, so the user
//! can see which handle a press will grab. Asserted on the frame's paint output — the white handle
//! squares `paint_selection` draws — which is what the user sees, not an internal hover flag.

mod support;

use egui::{Color32, Key, Shape};
use support::TestApp;

/// The side of a handle at rest and while hovered, in points (mirrors `ui::canvas`).
const HANDLE_SIZE: f32 = 9.0;
const HANDLE_HOVER_SIZE: f32 = 12.0;

/// The side lengths of the handles painted this frame. A handle is one of the white squares with the
/// dark (rgb 20) border that `paint_selection` draws for the selection and crop boxes; that border
/// colour is unique to handles, so it picks them out of the rest of the frame's shapes.
fn handle_sizes(app: &TestApp) -> Vec<f32> {
    app.harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Rect(rect) => Some(rect),
            _ => None,
        })
        .filter(|rect| {
            rect.fill == Color32::WHITE && rect.stroke.color == Color32::from_rgb(20, 20, 20)
        })
        .map(|rect| rect.rect.width())
        .collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

/// The number of handles painted at rest and the number grown to the hover size.
fn at_rest_and_grown(sizes: &[f32]) -> (usize, usize) {
    (
        sizes.iter().filter(|s| close(**s, HANDLE_SIZE)).count(),
        sizes
            .iter()
            .filter(|s| close(**s, HANDLE_HOVER_SIZE))
            .count(),
    )
}

/// Hover an Image pixel and run a frame, so the paint output reflects the pointer resting there.
fn hover(app: &mut TestApp, x: f32, y: f32) {
    let at = app.screen(x, y);
    app.harness.hover_at(at);
    app.harness.run();
}

/// A 320×240 Image with one rectangle (40, 30)–(140, 90), selected, with the select tool active.
fn app_with_selected_rectangle() -> TestApp {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    app.press(Key::Escape); // back to the select tool; the rectangle stays selected
    app
}

#[test]
fn hovering_a_selection_handle_grows_only_that_handle() {
    let mut app = app_with_selected_rectangle();

    // Over the bottom-right corner handle: exactly one handle grows, the other seven stay put.
    hover(&mut app, 140.0, 90.0);
    let sizes = handle_sizes(&app);
    assert_eq!(sizes.len(), 8, "eight handles painted: {sizes:?}");
    assert_eq!(
        at_rest_and_grown(&sizes),
        (7, 1),
        "seven at rest, one grown: {sizes:?}"
    );
}

#[test]
fn the_highlight_clears_when_the_pointer_leaves_the_handles() {
    let mut app = app_with_selected_rectangle();

    hover(&mut app, 140.0, 90.0); // grab a handle first
    hover(&mut app, 90.0, 60.0); // then move into the body, off every handle

    let sizes = handle_sizes(&app);
    assert_eq!(sizes.len(), 8, "eight handles painted: {sizes:?}");
    assert!(
        sizes.iter().all(|s| close(*s, HANDLE_SIZE)),
        "every handle back at rest: {sizes:?}"
    );
}

#[test]
fn hovering_a_crop_handle_grows_only_that_handle() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::C);
    app.drag_px((40.0, 30.0), (240.0, 180.0)); // a crop region, painted with its own 8 handles

    // Off every handle: all at rest.
    hover(&mut app, 140.0, 105.0);
    let sizes = handle_sizes(&app);
    assert_eq!(sizes.len(), 8, "eight crop handles painted: {sizes:?}");
    assert!(
        sizes.iter().all(|s| close(*s, HANDLE_SIZE)),
        "every crop handle at rest: {sizes:?}"
    );

    // Over the bottom-right crop handle: exactly one grows, the same hover variant as the selection.
    hover(&mut app, 240.0, 180.0);
    let sizes = handle_sizes(&app);
    assert_eq!(
        at_rest_and_grown(&sizes),
        (7, 1),
        "seven at rest, one grown: {sizes:?}"
    );
}
