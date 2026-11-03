//! Endpoint handles for a lone line/arrow (#50): a selection of exactly one line or arrow is
//! reshaped by two endpoint handles (one at each end) rather than the dashed bounding box, so either
//! end can be dragged on its own. Any other selection keeps the box + 8 handles. Geometry is checked
//! on the Document; what is painted is checked on the frame's paint output (the white handle squares
//! `paint_endpoints`/`paint_selection` draw), the way `editor_handle_highlight` does, since handles
//! are an egui overlay the composite never sees.

mod support;

use egui::{Color32, CursorIcon, Key, Modifiers, Shape};
use lecoq_core::{AnnotationShape, Point};
use support::TestApp;

/// The side of a handle at rest and while hovered, in points (mirrors `ui::canvas`).
const HANDLE_SIZE: f32 = 9.0;
const HANDLE_HOVER_SIZE: f32 = 12.0;

/// A 320×240 Image with one line from (40, 200) to (260, 60), selected, the select tool active.
fn app_with_selected_line() -> TestApp {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::L);
    app.drag_px((40.0, 200.0), (260.0, 60.0));
    app.press(Key::Escape); // back to the select tool; the line stays selected
    assert_eq!(app.doc().undo_len(), 1);
    app
}

/// The lone annotation's segment ends.
fn segment(app: &TestApp) -> (Point, Point) {
    match &app.doc().annotations()[0].shape {
        AnnotationShape::Line(l) => (l.from, l.to),
        AnnotationShape::Arrow(a) => (a.from, a.to),
        other => panic!("not a line or arrow: {other:?}"),
    }
}

fn assert_near(actual: Point, expected: (f32, f32)) {
    assert!(
        (actual.x - expected.0).abs() < 0.5 && (actual.y - expected.1).abs() < 0.5,
        "{actual:?} is not near {expected:?}"
    );
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

/// The side lengths of the handle squares painted this frame: the white squares with the dark
/// (rgb 20) border, the border colour unique to handles (mirrors `editor_handle_highlight`).
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

/// Hover an Image pixel and run a frame, so the paint output reflects the pointer resting there.
fn hover(app: &mut TestApp, x: f32, y: f32) {
    let at = app.screen(x, y);
    app.harness.hover_at(at);
    app.harness.run();
}

// ---- What is drawn ----

#[test]
fn a_lone_line_is_drawn_with_two_endpoint_handles_and_no_box() {
    let app = app_with_selected_line();

    // No dashed bounding box (its screen rect is what `selection_box` exposes).
    assert_eq!(app.harness.state().selection_box(), None);
    // Exactly two handles, both at rest (no pointer over them).
    let sizes = handle_sizes(&app);
    assert_eq!(sizes.len(), 2, "two endpoint handles: {sizes:?}");
    assert!(
        sizes.iter().all(|s| close(*s, HANDLE_SIZE)),
        "both at rest: {sizes:?}"
    );
}

#[test]
fn an_arrow_is_drawn_with_two_endpoint_handles_too() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::A);
    app.drag_px((40.0, 200.0), (260.0, 60.0));
    app.press(Key::Escape);

    assert_eq!(app.harness.state().selection_box(), None);
    assert_eq!(handle_sizes(&app).len(), 2);
}

#[test]
fn a_single_non_line_shape_keeps_the_box_and_eight_handles() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    app.press(Key::Escape);
    app.click_px((40.0, 60.0)); // on the outline of the unfilled rectangle

    assert!(app.harness.state().selection_box().is_some(), "a box");
    assert_eq!(handle_sizes(&app).len(), 8, "eight box handles");
}

#[test]
fn a_multi_selection_containing_a_line_keeps_the_box_and_eight_handles() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::L);
    app.drag_px((40.0, 40.0), (120.0, 120.0));
    app.press(Key::R);
    app.drag_px((160.0, 40.0), (240.0, 120.0));
    app.press(Key::Escape);
    // Rubber-band both into the selection.
    app.drag_px((20.0, 20.0), (260.0, 160.0));
    assert_eq!(app.doc().selection().len(), 2);

    // Two shapes selected, even though one is a line: the bounding box is back.
    assert!(app.harness.state().selection_box().is_some(), "a box");
    assert_eq!(handle_sizes(&app).len(), 8, "eight box handles");
}

// ---- Dragging an endpoint ----

#[test]
fn dragging_an_endpoint_moves_only_that_end_as_one_undo_step() {
    let mut app = app_with_selected_line();
    // Grab the `from` end at (40, 200) and drag it; the `to` end stays put.
    app.drag_px((40.0, 200.0), (40.0, 100.0));

    let (from, to) = segment(&app);
    assert_near(from, (40.0, 100.0));
    assert_near(to, (260.0, 60.0));
    assert_eq!(app.doc().undo_len(), 2, "one more step than the draw");

    // One undo restores the end; the other is untouched throughout.
    app.key_with(Modifiers::COMMAND, Key::Z);
    let (from, to) = segment(&app);
    assert_near(from, (40.0, 200.0));
    assert_near(to, (260.0, 60.0));
}

#[test]
fn dragging_the_other_endpoint_moves_only_that_end() {
    let mut app = app_with_selected_line();
    // Grab the `to` end at (260, 60) and drag it; the `from` end stays put.
    app.drag_px((260.0, 60.0), (200.0, 200.0));

    let (from, to) = segment(&app);
    assert_near(from, (40.0, 200.0));
    assert_near(to, (200.0, 200.0));
}

#[test]
fn dragging_the_body_moves_the_whole_line() {
    let mut app = app_with_selected_line();
    // The segment's midpoint (150, 130): clear of both endpoint handles, so a drag moves the shape.
    app.drag_px((150.0, 130.0), (160.0, 140.0));

    let (from, to) = segment(&app);
    assert_near(from, (50.0, 210.0));
    assert_near(to, (270.0, 70.0));
}

#[test]
fn a_press_on_an_endpoint_without_moving_records_no_step() {
    let mut app = app_with_selected_line();
    app.click_px((40.0, 200.0)); // press and release on the `from` endpoint, no drag

    let (from, to) = segment(&app);
    assert_near(from, (40.0, 200.0));
    assert_near(to, (260.0, 60.0));
    assert_eq!(app.doc().undo_len(), 1, "still just the draw");
    // The line stays selected (still drawn with its endpoint handles).
    assert_eq!(app.doc().selection().len(), 1);
}

#[test]
fn shift_snaps_the_dragged_endpoint_to_15_degrees_around_the_fixed_end() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::L);
    app.drag_px((50.0, 100.0), (150.0, 100.0)); // a horizontal line
    app.press(Key::Escape);

    // Grab the `to` end and drag it up-and-right with Shift; the `from` end (50, 100) is the pivot.
    app.drag_px_with(Modifiers::SHIFT, (150.0, 100.0), (151.0, 40.0));

    let (from, to) = segment(&app);
    assert_near(from, (50.0, 100.0)); // the fixed end did not move
    // The segment now lies on a 15° multiple around the fixed end.
    let degrees = (to.y - from.y)
        .atan2(to.x - from.x)
        .to_degrees()
        .rem_euclid(15.0);
    assert!(
        degrees < 0.05 || (15.0 - degrees) < 0.05,
        "angle not a multiple of 15°: remainder {degrees}"
    );
    // It actually moved off the original horizontal.
    assert!((to.y - from.y).abs() > 1.0, "the end did move: {to:?}");
}

// ---- Hover highlight and cursor (#48, #49) ----

#[test]
fn hovering_an_endpoint_grows_only_that_handle() {
    let mut app = app_with_selected_line();
    hover(&mut app, 40.0, 200.0); // over the `from` endpoint

    let sizes = handle_sizes(&app);
    assert_eq!(sizes.len(), 2, "two endpoint handles: {sizes:?}");
    assert_eq!(
        (
            sizes.iter().filter(|s| close(**s, HANDLE_SIZE)).count(),
            sizes
                .iter()
                .filter(|s| close(**s, HANDLE_HOVER_SIZE))
                .count(),
        ),
        (1, 1),
        "one at rest, one grown: {sizes:?}"
    );
}

#[test]
fn an_endpoint_shows_a_resize_cursor_and_the_body_shows_move() {
    let mut app = app_with_selected_line();

    let resize = [
        CursorIcon::ResizeNwSe,
        CursorIcon::ResizeNeSw,
        CursorIcon::ResizeHorizontal,
        CursorIcon::ResizeVertical,
    ];
    let at_from = app.cursor_at(app.screen(40.0, 200.0));
    assert!(resize.contains(&at_from), "endpoint cursor: {at_from:?}");
    // The body of the selected line moves it, not a resize.
    assert_eq!(app.cursor_at(app.screen(150.0, 130.0)), CursorIcon::Move);
}
