//! Cursor feedback across the tools and hover states (#48, §4): the glyph reflects what a press at
//! the pointer would do, recomputed every frame from the same hit-testing the tools use. Asserted
//! on the user-visible `platform_output.cursor_icon` through the headless App seam (§9), the way
//! `editor_text` asserts on `platform_output.ime`.

mod support;

use egui::{CursorIcon, Key, PointerButton, pos2, vec2};
use support::TestApp;

/// Rectangle A from (40, 30) to (140, 90) on a 320×240 Image shown at 100% (Image pixels are
/// screen points), with the select tool active and A selected.
fn app_with_selected_rectangle() -> TestApp {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    app.press(Key::Escape);
    // The rectangle is unfilled, so a press hits its outline, not its interior: click the edge.
    app.click_px((40.0, 60.0));
    assert_eq!(app.selection_bounds(), Some([40, 30, 140, 90]));
    app
}

/// Like [`app_with_selected_rectangle`], then the selection cleared: A is on the canvas but
/// nothing is selected, the select tool active.
fn app_with_unselected_rectangle() -> TestApp {
    let mut app = app_with_selected_rectangle();
    app.click_px((300.0, 10.0));
    assert!(app.doc().selection().is_empty());
    app
}

/// The large fixture zoomed in three steps: larger than the canvas on both axes, so it is pannable
/// (mirrors `pan::zoomed`).
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

// ---- Resize handles (story 1, 2) ----

#[test]
fn corner_handles_show_the_diagonal_resize_glyphs() {
    let mut app = app_with_selected_rectangle();

    assert_eq!(
        app.cursor_at(app.screen(40.0, 30.0)),
        CursorIcon::ResizeNwSe
    );
    assert_eq!(
        app.cursor_at(app.screen(140.0, 90.0)),
        CursorIcon::ResizeNwSe
    );
    assert_eq!(
        app.cursor_at(app.screen(140.0, 30.0)),
        CursorIcon::ResizeNeSw
    );
    assert_eq!(
        app.cursor_at(app.screen(40.0, 90.0)),
        CursorIcon::ResizeNeSw
    );
}

#[test]
fn edge_handles_show_the_axis_resize_glyphs() {
    let mut app = app_with_selected_rectangle();

    assert_eq!(
        app.cursor_at(app.screen(40.0, 60.0)),
        CursorIcon::ResizeHorizontal
    );
    assert_eq!(
        app.cursor_at(app.screen(140.0, 60.0)),
        CursorIcon::ResizeHorizontal
    );
    assert_eq!(
        app.cursor_at(app.screen(90.0, 30.0)),
        CursorIcon::ResizeVertical
    );
    assert_eq!(
        app.cursor_at(app.screen(90.0, 90.0)),
        CursorIcon::ResizeVertical
    );
}

// ---- Selected body and selectable Annotation (story 3, 4, 11) ----

#[test]
fn the_body_of_a_selected_annotation_shows_the_move_cursor() {
    let mut app = app_with_selected_rectangle();

    // On the top edge, clear of every handle: a press hits the selected rectangle, so a drag moves
    // it (the outline is the body of this unfilled shape, matching the Select tool's press).
    assert_eq!(app.cursor_at(app.screen(65.0, 30.0)), CursorIcon::Move);
}

#[test]
fn an_unselected_annotation_shows_the_pointing_hand() {
    let mut app = app_with_unselected_rectangle();

    assert_eq!(
        app.cursor_at(app.screen(65.0, 30.0)),
        CursorIcon::PointingHand
    );
}

#[test]
fn empty_canvas_with_select_falls_back_to_the_arrow() {
    let mut app = app_with_unselected_rectangle();

    assert_eq!(app.cursor_at(app.screen(300.0, 10.0)), CursorIcon::Default);
}

// ---- A drawing tool draws over everything (story 5) ----

#[test]
fn a_drawing_tool_shows_the_crosshair_even_over_an_annotation() {
    let mut app = app_with_selected_rectangle();
    app.press(Key::R);

    // Over the body of the existing rectangle: a drag draws a new shape, not a grab of the old one.
    assert_eq!(app.cursor_at(app.screen(90.0, 60.0)), CursorIcon::Crosshair);
}

// ---- Editable text (story 6) ----

#[test]
fn the_text_tool_shows_the_i_beam_over_the_canvas() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::T);

    assert_eq!(app.cursor_at(app.screen(120.0, 100.0)), CursorIcon::Text);
}

#[test]
fn an_active_text_caret_shows_the_i_beam_over_its_box() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::T);
    app.click_px((50.0, 60.0));
    assert!(app.harness.state().editing_text());

    assert_eq!(app.cursor_at(app.screen(52.0, 66.0)), CursorIcon::Text);
}

// ---- Panning a zoomed Image (story 7, 8, 9, 12) ----

#[test]
fn space_over_a_pannable_image_shows_the_open_grab_hand() {
    let mut app = zoomed();
    let p = pos2(500.0, 300.0);
    app.harness.hover_at(p);
    app.harness.key_down(Key::Space);
    app.harness.run();

    assert_eq!(
        app.harness.output().platform_output.cursor_icon,
        CursorIcon::Grab
    );
    app.harness.key_up(Key::Space);
}

#[test]
fn space_over_a_fit_image_leaves_the_cursor_unchanged() {
    // A small Image fits (not pannable): Space promises no pan, so the arrow stands (story 9).
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    let p = pos2(500.0, 300.0);
    app.harness.hover_at(p);
    app.harness.key_down(Key::Space);
    app.harness.run();

    assert_eq!(
        app.harness.output().platform_output.cursor_icon,
        CursorIcon::Default
    );
    app.harness.key_up(Key::Space);
}

#[test]
fn a_pan_drag_shows_the_closed_grabbing_hand() {
    let mut app = zoomed();
    let p = pos2(500.0, 300.0);
    app.harness.hover_at(p);
    app.harness.event(egui::Event::PointerButton {
        pos: p,
        button: PointerButton::Middle,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    app.harness.hover_at(p + vec2(-30.0, -20.0));
    app.harness.run();

    assert_eq!(
        app.harness.output().platform_output.cursor_icon,
        CursorIcon::Grabbing
    );
    app.harness.event(egui::Event::PointerButton {
        pos: p + vec2(-30.0, -20.0),
        button: PointerButton::Middle,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    app.settle();
}

// ---- Crop region handles match the selection handles (story 10) ----

#[test]
fn crop_region_handles_show_the_resize_glyphs() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::C);
    app.drag_px((50.0, 40.0), (200.0, 160.0));
    assert!(app.harness.state().crop_region().is_some());

    // The top-left corner and the left edge, the same glyphs as a selection's handles.
    assert_eq!(
        app.cursor_at(app.screen(50.0, 40.0)),
        CursorIcon::ResizeNwSe
    );
    assert_eq!(
        app.cursor_at(app.screen(50.0, 100.0)),
        CursorIcon::ResizeHorizontal
    );
}

#[test]
fn inside_the_crop_region_is_the_move_cursor_and_outside_the_crosshair() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::C);
    app.drag_px((50.0, 40.0), (200.0, 160.0));

    assert_eq!(app.cursor_at(app.screen(120.0, 100.0)), CursorIcon::Move);
    assert_eq!(
        app.cursor_at(app.screen(280.0, 220.0)),
        CursorIcon::Crosshair
    );
}
