//! The empty window, Ctrl+O and dropping a file (§2, §3.5).

mod support;

use std::sync::Arc;

use egui_kittest::kittest::Queryable;
use lecoq_platform::{MemoryFileDialogs, Platform};
use support::TestApp;

const EMPTY_HINT: &str = "Open (Ctrl+O), drop or paste (Ctrl+V) an image";

/// A headless App whose open dialog is the returned fake.
fn app_with_dialogs() -> (TestApp, Arc<MemoryFileDialogs>) {
    let dialogs = Arc::new(MemoryFileDialogs::default());
    let mut platform = Platform::in_memory();
    platform.dialogs = dialogs.clone();
    (TestApp::with_platform(platform), dialogs)
}

#[test]
fn started_without_a_file_the_canvas_invites_to_open_one() {
    let mut app = TestApp::new();
    app.settle();

    app.harness.get_by_label(EMPTY_HINT);
}

#[test]
fn the_hint_goes_once_an_image_is_open() {
    let app = TestApp::open_fixture("small-320x240.jpg");

    assert!(app.harness.query_by_label(EMPTY_HINT).is_none());
}

#[test]
fn ctrl_o_opens_the_file_chosen_in_the_open_dialog() {
    let (mut app, dialogs) = app_with_dialogs();
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    dialogs.answer_next(Some(path));
    app.settle();

    app.press_ctrl(egui::Key::O);
    app.settle();

    assert_eq!(dialogs.opened(), 1);
    assert_eq!(app.status("File name"), "small-320x240.jpg");
    assert_eq!(app.status("Pixel size"), "320×240");
}

#[test]
fn cancelling_the_open_dialog_keeps_the_empty_window() {
    let (mut app, dialogs) = app_with_dialogs();
    dialogs.answer_next(None);
    app.settle();

    app.press_ctrl(egui::Key::O);
    app.settle();

    assert_eq!(dialogs.opened(), 1);
    assert_eq!(app.status_field("File name"), None);
    app.harness.get_by_label(EMPTY_HINT);
}

#[test]
fn the_open_dialog_shows_the_apps_title_and_filter_names() {
    let (mut app, dialogs) = app_with_dialogs();
    dialogs.answer_next(None);
    app.settle();

    app.press_ctrl(egui::Key::O);
    app.settle();

    let text = dialogs.last_open_text().expect("the open dialog was shown");
    assert_eq!(text.title, lecoq::strings::OPEN_DIALOG_TITLE);
    assert_eq!(text.images, lecoq::strings::IMAGES_FILTER);
    assert_eq!(text.all_files, lecoq::strings::ALL_FILES_FILTER);
}

#[test]
fn cancelling_the_open_dialog_keeps_the_current_image() {
    let (mut app, dialogs) = app_with_dialogs();
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    dialogs.answer_next(None);

    app.press_ctrl(egui::Key::O);
    app.settle();

    assert_eq!(app.status("File name"), "small-320x240.jpg");
}

#[test]
fn ctrl_o_while_the_dialog_is_up_opens_no_second_one() {
    let (mut app, dialogs) = app_with_dialogs();
    dialogs.hold_answers();
    app.settle();

    app.press_ctrl(egui::Key::O);
    app.harness.run();
    app.press_ctrl(egui::Key::O);
    app.harness.run();

    assert_eq!(dialogs.opened(), 1);
}

#[test]
fn dropping_a_file_onto_the_empty_window_opens_it() {
    let mut app = TestApp::new();
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.settle();

    app.drop_file(path);
    app.settle();

    assert_eq!(app.status("File name"), "small-320x240.jpg");
    assert!(app.harness.query_by_label(EMPTY_HINT).is_none());
}

#[test]
fn dropping_a_file_replaces_the_image() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    let other = app.add_fixtures(&["large-2400x1600.jpg"]);

    app.drop_file(other);
    app.settle();

    assert_eq!(app.status("File name"), "large-2400x1600.jpg");
    assert_eq!(app.status("Pixel size"), "2400×1600");
}

#[test]
fn dropping_something_that_is_not_an_image_says_so() {
    let mut app = TestApp::new();
    let path = app.dir.path().join("notes.txt");
    std::fs::write(&path, "not an image").unwrap();
    app.settle();

    app.drop_file(path);
    app.settle();

    app.harness.get_by_label_contains("Can't open notes.txt");
}
