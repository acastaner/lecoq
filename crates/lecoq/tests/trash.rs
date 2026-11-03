//! Trash from the viewer (spec §3.1, §8.2): Delete with no Annotation selected moves the file on
//! screen to the system trash and shows the next Sibling. The trash is the in-memory fake: these
//! tests never touch a real trash.

mod support;

use std::sync::Arc;
use std::time::Duration;

use egui::Key;
use egui_kittest::kittest::Queryable;
use lecoq_platform::{MemoryTrash, Platform};
use support::TestApp;

const SMALL: &str = "small-320x240.jpg";

/// A folder of `names` (copies of the small fixture) with `open` on screen, and the fake trash.
fn folder(names: &[&str], open: &str) -> (TestApp, Arc<MemoryTrash>) {
    let trash = Arc::new(MemoryTrash::default());
    let app = TestApp::open_in_folder_on(
        Platform::in_memory().with_trash(trash.clone()),
        SMALL,
        names,
        open,
    );
    (app, trash)
}

#[test]
fn delete_trashes_the_file_and_shows_the_next_sibling() {
    let (mut app, trash) = folder(&["a.jpg", "b.jpg", "c.jpg"], "b.jpg");

    app.press(Key::Delete);

    assert_eq!(trash.trashed(), vec![app.dir.path().join("b.jpg")]);
    assert_eq!(app.status("File name"), "c.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 2");
    assert_eq!(app.status("Message"), "Moved to trash");
}

#[test]
fn delete_with_an_annotation_selected_removes_it_and_keeps_the_file() {
    let (mut app, trash) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    assert!(!app.doc().selection().is_empty());

    app.press(Key::Delete);

    assert!(app.doc().annotations().is_empty());
    assert!(trash.trashed().is_empty());
    // Still a.jpg, now with unsaved edits.
    assert_eq!(app.status("File name"), "a.jpg •");
}

#[test]
fn delete_at_the_end_of_the_list_shows_the_previous_sibling() {
    let (mut app, trash) = folder(&["a.jpg", "b.jpg", "c.jpg"], "c.jpg");

    app.press(Key::Delete);

    assert_eq!(trash.trashed(), vec![app.dir.path().join("c.jpg")]);
    assert_eq!(app.status("File name"), "b.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 2");
    assert_eq!(app.status("Message"), "Moved to trash");
}

#[test]
fn deleting_one_after_another_walks_through_the_folder() {
    let (mut app, trash) = folder(&["a.jpg", "b.jpg", "c.jpg"], "a.jpg");

    app.press(Key::Delete);
    assert_eq!(app.status("File name"), "b.jpg");
    app.press(Key::Delete);
    assert_eq!(app.status("File name"), "c.jpg");
    assert_eq!(app.status("Sibling position"), "1 / 1");

    let dir = app.dir.path();
    assert_eq!(trash.trashed(), vec![dir.join("a.jpg"), dir.join("b.jpg")]);
}

#[test]
fn trashing_the_only_image_leaves_the_empty_window() {
    let (mut app, trash) = folder(&["only.jpg"], "only.jpg");

    app.press(Key::Delete);

    assert_eq!(trash.trashed(), vec![app.dir.path().join("only.jpg")]);
    assert!(app.harness.state().canvas().is_none());
    app.harness
        .get_by_label("Open (Ctrl+O), drop or paste (Ctrl+V) an image");
    assert_eq!(app.status_field("File name"), None);
    assert_eq!(app.status("Message"), "Moved to trash");

    // Nothing left to trash.
    app.press(Key::Delete);
    assert_eq!(trash.trashed().len(), 1);
}

#[test]
fn a_failed_trash_keeps_the_image_and_says_why() {
    let (mut app, trash) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    trash.fail_next("the folder is read-only");

    app.press(Key::Delete);

    assert!(trash.trashed().is_empty());
    assert_eq!(app.status("File name"), "a.jpg");
    assert_eq!(app.status("Sibling position"), "1 / 2");
    assert_eq!(
        app.status("Message"),
        "Can't move a.jpg to trash: the folder is read-only"
    );
}

#[test]
fn a_file_that_failed_to_open_can_be_trashed() {
    // Nothing was painted, so no Sibling scan has started: Delete starts one and waits for it.
    let trash = Arc::new(MemoryTrash::default());
    let mut app = TestApp::with_platform(Platform::in_memory().with_trash(trash.clone()));
    app.add_fixture_as(SMALL, "b.jpg");
    let broken = app.dir.path().join("a.jpg");
    std::fs::write(&broken, b"not a JPEG").unwrap();
    app.open(&broken);
    assert!(app.harness.state().canvas().is_none());

    app.press(Key::Delete);

    assert_eq!(trash.trashed(), vec![broken]);
    assert_eq!(app.status("File name"), "b.jpg");
    assert_eq!(app.status("Sibling position"), "1 / 1");
}

#[test]
fn delete_on_the_first_painted_frame_waits_for_the_sibling_scan() {
    let trash = Arc::new(MemoryTrash::default());
    let (mut app, release) = TestApp::launch_held_on(
        Platform::in_memory().with_trash(trash.clone()),
        &[SMALL, "png-64x48.png"],
    );
    release.send(()).unwrap();
    // Step to the first painted Image and press Delete straight away: the scan starts on the
    // frame after the first paint, the same frame the key is handled in.
    for _ in 0..2000 {
        app.harness.step();
        if app.harness.state().canvas().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.status("File name"), SMALL);
    app.harness.key_press(Key::Delete);
    app.settle();

    assert_eq!(trash.trashed(), vec![app.dir.path().join(SMALL)]);
    assert_eq!(app.status("File name"), "png-64x48.png");
    assert_eq!(app.status("Sibling position"), "1 / 1");
}
