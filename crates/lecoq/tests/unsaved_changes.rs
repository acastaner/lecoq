//! The unsaved-changes flow (spec §3.1, §6.4): moving to a Sibling, opening or dropping a file, or
//! closing the window with unsaved changes asks "Save changes?" with Cancel · Discard · Save.
//! Save saves then proceeds; Discard proceeds; Cancel and Esc stay put.

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use egui::{Key, ViewportCommand, ViewportEvent, ViewportId};
use egui_kittest::kittest::Queryable;
use lecoq_platform::{MemoryFileDialogs, MemoryTrash, Platform};
use support::TestApp;

const SMALL: &str = "small-320x240.jpg";
const TITLE: &str = "Save changes?";

/// A folder of `names` (copies of the small fixture) with `open` on screen, and the fake dialogs.
fn folder(names: &[&str], open: &str) -> (TestApp, Arc<MemoryFileDialogs>) {
    let dialogs = Arc::new(MemoryFileDialogs::default());
    let mut platform = Platform::in_memory();
    platform.dialogs = dialogs.clone();
    let app = TestApp::open_in_folder_on(platform, SMALL, names, open);
    (app, dialogs)
}

/// Draw a rectangle with the rectangle tool, then back to the select tool: an unsaved change.
fn edit(app: &mut TestApp) {
    app.press(Key::R);
    app.drag_px((10.0, 10.0), (60.0, 40.0));
    app.press(Key::Escape);
}

fn prompt_shown(app: &TestApp) -> bool {
    app.harness.query_by_label(TITLE).is_some()
}

/// Click one of the prompt's buttons and settle.
fn answer(app: &mut TestApp, button: &str) {
    app.dialog_control(button).click();
    app.settle();
}

fn bytes(path: &PathBuf) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

// ---- Moving to a Sibling ----

#[test]
fn moving_to_a_sibling_with_unsaved_changes_asks_first() {
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    edit(&mut app);

    app.press(Key::ArrowRight);

    assert!(prompt_shown(&app));
    app.harness.get_by_label("a.jpg has unsaved changes.");
    app.control("Cancel");
    app.control("Discard");
    app.dialog_control("Save");
    assert_eq!(app.status("File name"), "a.jpg •");
}

#[test]
fn jumping_to_a_sibling_with_unsaved_changes_asks_first() {
    // Home / End go through the same unsaved-changes flow as the arrow keys.
    let (mut app, _) = folder(&["a.jpg", "b.jpg", "c.jpg"], "a.jpg");
    edit(&mut app);

    app.press(Key::End);

    assert!(prompt_shown(&app));
    app.harness.get_by_label("a.jpg has unsaved changes.");
    answer(&mut app, "Discard");
    assert_eq!(app.status("File name"), "c.jpg");
}

#[test]
fn a_no_op_jump_with_unsaved_changes_does_not_ask() {
    // Already on the first image: Home jumps nowhere, so there is nothing to ask about.
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    edit(&mut app);

    app.press(Key::Home);

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "a.jpg •");
    assert_eq!(app.doc().annotations().len(), 1);
}

#[test]
fn without_unsaved_changes_there_is_no_prompt() {
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "a.jpg");

    app.press(Key::ArrowRight);

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "b.jpg");
}

#[test]
fn at_the_end_of_the_list_there_is_nothing_to_ask() {
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "b.jpg");
    edit(&mut app);

    app.press(Key::ArrowRight);

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("Message"), "Last image in folder");
    assert_eq!(app.status("File name"), "b.jpg •");
}

#[test]
fn discard_moves_on_and_leaves_the_file_alone() {
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let a = app.dir.path().join("a.jpg");
    let before = bytes(&a);
    edit(&mut app);
    app.press(Key::ArrowRight);

    answer(&mut app, "Discard");

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "b.jpg");
    assert_eq!(bytes(&a), before);
    // The edits are gone: back on a.jpg, nothing is unsaved.
    app.press(Key::ArrowLeft);
    assert_eq!(app.status("File name"), "a.jpg");
    assert!(app.doc().annotations().is_empty());
}

#[test]
fn cancel_stays_put_with_the_edits() {
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    edit(&mut app);
    app.press(Key::ArrowRight);

    answer(&mut app, "Cancel");

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "a.jpg •");
    assert_eq!(app.doc().annotations().len(), 1);
}

#[test]
fn esc_is_cancel_and_keeps_the_tool() {
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    edit(&mut app);
    app.press(Key::R);
    app.press(Key::ArrowRight);
    assert!(prompt_shown(&app));

    app.press(Key::Escape);

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "a.jpg •");
    // Esc answered the dialog only: it didn't also pick the select tool.
    assert_eq!(app.harness.state().tool(), lecoq::tools::ToolId::Rectangle);
}

#[test]
fn the_prompt_is_modal() {
    let (mut app, _) = folder(&["a.jpg", "b.jpg", "c.jpg"], "a.jpg");
    edit(&mut app);
    app.press(Key::ArrowRight);

    // Keys meant for the window behind it do nothing while it is up.
    app.press(Key::ArrowRight);
    app.press(Key::Delete);
    app.press(Key::E);

    assert!(prompt_shown(&app));
    assert_eq!(app.status("File name"), "a.jpg •");
    assert_eq!(app.doc().annotations().len(), 1);
    assert_eq!(app.harness.state().tool(), lecoq::tools::ToolId::Select);
}

#[test]
fn save_saves_then_moves_on() {
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let a = app.dir.path().join("a.jpg");
    let before = bytes(&a);
    edit(&mut app);
    app.press(Key::ArrowRight);

    answer(&mut app, "Save");

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "b.jpg");
    assert_ne!(bytes(&a), before, "a.jpg was written");
    // The rectangle (red, §4.3 default) is in the saved file.
    let saved = image::open(&a).unwrap().to_rgb8();
    let edge = saved.get_pixel(10, 25);
    assert!(edge[0] > 180 && edge[1] < 90 && edge[2] < 90, "{edge:?}");
}

#[test]
fn save_on_a_read_only_format_asks_where_then_moves_on() {
    let dialogs = Arc::new(MemoryFileDialogs::default());
    let mut platform = Platform::in_memory();
    platform.dialogs = dialogs.clone();
    let mut app = TestApp::with_platform(platform);
    app.add_fixture_as("gif-64x48.gif", "a.gif");
    app.add_fixture_as("gif-64x48.gif", "b.gif");
    app.open(app.dir.path().join("a.gif"));
    edit(&mut app);
    app.press(Key::ArrowRight);

    let target = app.dir.path().join("a.png");
    dialogs.answer_next(Some(target.clone()));
    answer(&mut app, "Save");

    assert!(target.exists(), "Save As wrote a.png");
    // The move happens from the saved file, in the re-scanned folder: a.png → b.gif.
    assert_eq!(app.status("File name"), "b.gif");
}

#[test]
fn cancelling_save_as_stays_put() {
    let dialogs = Arc::new(MemoryFileDialogs::default());
    let mut platform = Platform::in_memory();
    platform.dialogs = dialogs.clone();
    let mut app = TestApp::with_platform(platform);
    app.add_fixture_as("gif-64x48.gif", "a.gif");
    app.add_fixture_as("gif-64x48.gif", "b.gif");
    app.open(app.dir.path().join("a.gif"));
    edit(&mut app);
    app.press(Key::ArrowRight);

    dialogs.answer_next(None);
    answer(&mut app, "Save");

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "a.gif •");
}

// ---- While editing text (#28) ----

/// The text tool, a click at (50, 60): a text box being edited.
fn start_text(app: &mut TestApp) {
    app.press(Key::T);
    app.click_px((50.0, 60.0));
    assert!(app.harness.state().editing_text());
}

fn type_text(app: &mut TestApp, text: &str) {
    app.harness.event(egui::Event::Text(text.to_owned()));
    app.settle();
}

#[test]
fn moving_to_a_sibling_while_editing_text_finishes_it_first_then_asks() {
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    start_text(&mut app);
    type_text(&mut app, "Hi");

    // The wheel moves through the Siblings even while the text box has the keyboard.
    app.wheel(-1.0);

    assert!(!app.harness.state().editing_text(), "the text was finished");
    assert!(prompt_shown(&app));
    answer(&mut app, "Cancel");
    assert_eq!(app.status("File name"), "a.jpg •");
    assert_eq!(app.doc().annotations().len(), 1);
    assert_eq!(app.doc().undo_len(), 1);
}

#[test]
fn moving_to_a_sibling_from_an_empty_text_box_does_not_ask() {
    // Finishing discards the empty box, which leaves nothing unsaved: nothing to ask.
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    start_text(&mut app);

    app.wheel(-1.0);

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "b.jpg");
}

#[test]
fn esc_in_the_prompt_reached_while_editing_text_only_cancels() {
    let (mut app, _) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    start_text(&mut app);
    type_text(&mut app, "Hi");
    app.wheel(-1.0);
    assert!(prompt_shown(&app));

    app.press(Key::Escape);

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "a.jpg •");
    // Esc answered the dialog only: the text tool stays (a second Esc would pick Select).
    assert_eq!(app.harness.state().tool(), lecoq::tools::ToolId::Text);
    assert_eq!(app.doc().annotations().len(), 1);
}

// ---- Opening and dropping ----

#[test]
fn opening_a_file_with_unsaved_changes_asks_first() {
    let (mut app, dialogs) = folder(&["a.jpg"], "a.jpg");
    let other = app.add_fixture_as("png-64x48.png", "other.png");
    edit(&mut app);

    dialogs.answer_next(Some(other));
    app.press_ctrl(Key::O);
    app.settle();

    assert!(prompt_shown(&app));
    assert_eq!(app.status("File name"), "a.jpg •");
    answer(&mut app, "Discard");
    assert_eq!(app.status("File name"), "other.png");
}

#[test]
fn cancelling_an_open_keeps_the_image() {
    let (mut app, _) = folder(&["a.jpg"], "a.jpg");
    let other = app.add_fixture_as("png-64x48.png", "other.png");
    edit(&mut app);

    app.harness.state_mut().execute(lecoq::Command::Open(other));
    app.settle();
    answer(&mut app, "Cancel");

    assert_eq!(app.status("File name"), "a.jpg •");
    assert_eq!(app.doc().annotations().len(), 1);
}

#[test]
fn dropping_a_file_with_unsaved_changes_asks_first_and_save_saves_then_opens() {
    let (mut app, _) = folder(&["a.jpg"], "a.jpg");
    let a = app.dir.path().join("a.jpg");
    let before = bytes(&a);
    let other = app.add_fixture_as("png-64x48.png", "other.png");
    edit(&mut app);

    app.drop_file(other);
    app.settle();
    assert!(prompt_shown(&app));
    assert_eq!(app.status("File name"), "a.jpg •");

    answer(&mut app, "Save");

    assert_ne!(bytes(&a), before, "a.jpg was written");
    assert_eq!(app.status("File name"), "other.png");
}

// ---- Closing the window ----

/// The window manager asks to close the window (the title bar's ×, Alt+F4).
fn request_close(app: &mut TestApp) {
    app.harness
        .input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    app.harness.step();
}

fn sent(app: &TestApp, wanted: &ViewportCommand) -> bool {
    app.harness
        .output()
        .viewport_output
        .get(&ViewportId::ROOT)
        .is_some_and(|v| v.commands.contains(wanted))
}

#[test]
fn closing_without_unsaved_changes_just_closes() {
    let (mut app, _) = folder(&["a.jpg"], "a.jpg");

    request_close(&mut app);

    assert!(!sent(&app, &ViewportCommand::CancelClose));
    assert!(app.harness.state().is_closing());
    app.settle();
    assert!(!prompt_shown(&app));
}

#[test]
fn closing_with_unsaved_changes_asks_and_cancel_keeps_the_window() {
    let (mut app, _) = folder(&["a.jpg"], "a.jpg");
    edit(&mut app);

    request_close(&mut app);
    assert!(sent(&app, &ViewportCommand::CancelClose));
    app.settle();
    assert!(prompt_shown(&app));

    answer(&mut app, "Cancel");
    assert!(!app.harness.state().is_closing());
    assert_eq!(app.status("File name"), "a.jpg •");
}

#[test]
fn discard_closes_the_window() {
    let (mut app, _) = folder(&["a.jpg"], "a.jpg");
    edit(&mut app);
    request_close(&mut app);
    app.settle();

    app.control("Discard").click();
    app.harness.step();

    assert!(sent(&app, &ViewportCommand::Close));
    assert!(app.harness.state().is_closing());
    // The close that comes back is let through.
    request_close(&mut app);
    assert!(!sent(&app, &ViewportCommand::CancelClose));
}

#[test]
fn save_saves_then_closes_the_window() {
    let (mut app, _) = folder(&["a.jpg"], "a.jpg");
    let a = app.dir.path().join("a.jpg");
    let before = bytes(&a);
    edit(&mut app);
    request_close(&mut app);
    app.settle();

    answer(&mut app, "Save");

    assert_ne!(bytes(&a), before, "a.jpg was written");
    assert!(app.harness.state().is_closing());
}

// ---- Not a trigger ----

#[test]
fn trashing_the_file_does_not_ask() {
    // §3.1/§6.4 list moving to a Sibling, opening, dropping and closing; Delete (nothing selected)
    // is an explicit "throw this file away", and saving it first would be pointless.
    let trash = Arc::new(MemoryTrash::default());
    let mut app = TestApp::open_in_folder_on(
        Platform::in_memory().with_trash(trash.clone()),
        SMALL,
        &["a.jpg", "b.jpg"],
        "a.jpg",
    );
    edit(&mut app);
    // Deselect, so Delete means the file.
    app.click_px((200.0, 200.0));

    app.press(Key::Delete);

    assert!(!prompt_shown(&app));
    assert_eq!(trash.trashed(), vec![app.dir.path().join("a.jpg")]);
    assert_eq!(app.status("File name"), "b.jpg");
}
