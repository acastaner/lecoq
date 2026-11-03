//! Right-click returns to the Select tool (#47, ADR 0006): a secondary press on the canvas leaves
//! any drawing tool for Select, cancelling a mid-draw gesture; in Select it does nothing.

mod support;

use egui::{Key, Modifiers};
use lecoq::tools::ToolId;
use support::TestApp;

#[test]
fn right_click_with_a_drawing_tool_active_returns_to_select() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::R);
    assert_eq!(app.harness.state().tool(), ToolId::Rectangle);

    app.secondary_press_px((120.0, 90.0));
    assert_eq!(app.harness.state().tool(), ToolId::Select);
}

#[test]
fn right_click_cancels_an_in_progress_draw() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::R);
    // Begin a rectangle and leave the gesture open: it is a provisional Annotation in the Document.
    app.drag_hold_px((40.0, 30.0), (100.0, 80.0));
    assert_eq!(app.doc().annotations().len(), 1);

    // A secondary press mid-gesture switches to Select and abandons the half-drawn rectangle.
    let p = app.screen(100.0, 80.0);
    app.harness.event(egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    // Release the still-held primary button; acted on at press, it no longer carries meaning.
    app.harness.event(egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    app.settle();

    assert_eq!(app.harness.state().tool(), ToolId::Select);
    assert!(
        app.doc().annotations().is_empty(),
        "the mid-draw rectangle is rolled back"
    );
    assert_eq!(app.doc().undo_len(), 0, "a cancelled draw leaves no step");
}

#[test]
fn right_click_in_select_does_nothing_and_keeps_the_selection() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    app.press(Key::Escape);
    assert_eq!(app.harness.state().tool(), ToolId::Select);

    // Select the rectangle, then right-click it.
    app.click_px((40.0, 60.0));
    let selected = app.doc().selection().to_vec();
    assert_eq!(selected.len(), 1);

    app.secondary_press_px((40.0, 60.0));

    assert_eq!(app.harness.state().tool(), ToolId::Select);
    assert_eq!(
        app.doc().selection().to_vec(),
        selected,
        "the selection is left untouched"
    );
}
