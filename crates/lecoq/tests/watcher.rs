//! The live directory watcher (spec §3.2): files added, deleted and renamed in the folder while
//! lecoq is open update the Sibling list, and the current Image keeps its place in it.

mod support;

use egui::Key;
use support::{TestApp, fixture_path};

const SMALL: &str = "small-320x240.jpg";

#[test]
fn a_new_file_joins_the_siblings_and_the_current_image_keeps_its_place() {
    let mut app = TestApp::open_in_folder(SMALL, &["a.jpg", "b.jpg", "c.jpg"], "b.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 3");

    std::fs::copy(fixture_path(SMALL), app.dir.path().join("0.jpg")).unwrap();
    app.wait_for_status("Sibling position", "3 / 4");
    assert_eq!(app.status("File name"), "b.jpg");

    app.press(Key::ArrowLeft);
    app.press(Key::ArrowLeft);
    assert_eq!(app.status("File name"), "0.jpg");
    assert_eq!(app.status("Sibling position"), "1 / 4");
}

#[test]
fn a_deleted_file_leaves_the_siblings() {
    let mut app = TestApp::open_in_folder(SMALL, &["a.jpg", "b.jpg", "c.jpg"], "b.jpg");

    std::fs::remove_file(app.dir.path().join("a.jpg")).unwrap();
    app.wait_for_status("Sibling position", "1 / 2");
    assert_eq!(app.status("File name"), "b.jpg");

    app.press(Key::ArrowLeft);
    assert_eq!(app.status("File name"), "b.jpg");
    assert_eq!(app.status("Message"), "First image in folder");
}

#[test]
fn a_renamed_file_moves_to_its_new_place() {
    let mut app = TestApp::open_in_folder(SMALL, &["a.jpg", "b.jpg", "c.jpg"], "b.jpg");

    let dir = app.dir.path().to_owned();
    std::fs::rename(dir.join("a.jpg"), dir.join("d.jpg")).unwrap();
    app.wait_for_status("Sibling position", "1 / 3");

    app.press(Key::ArrowRight);
    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "d.jpg");
    assert_eq!(app.status("Sibling position"), "3 / 3");
}

#[test]
fn files_that_are_not_siblings_are_ignored() {
    let mut app = TestApp::open_in_folder(SMALL, &["a.jpg", "b.jpg"], "a.jpg");

    let dir = app.dir.path().to_owned();
    std::fs::write(dir.join("notes.txt"), "not an image").unwrap();
    std::fs::copy(fixture_path(SMALL), dir.join(".hidden.jpg")).unwrap();
    std::fs::create_dir(dir.join("folder.jpg")).unwrap();
    // A Sibling added after them: once it's in, the others have been seen too.
    std::fs::copy(fixture_path(SMALL), dir.join("c.jpg")).unwrap();
    app.wait_for_status("Sibling position", "1 / 3");
}

#[test]
fn the_current_file_deleted_from_outside_stays_open_and_unsaved() {
    let mut app = TestApp::open_in_folder(SMALL, &["a.jpg", "b.jpg", "c.jpg"], "b.jpg");
    assert_eq!(app.status("File name"), "b.jpg");

    std::fs::remove_file(app.dir.path().join("b.jpg")).unwrap();
    app.wait_for_status("File name", "b.jpg •");
    assert!(app.harness.state().is_modified());
    assert!(
        app.harness.state().canvas().is_some(),
        "the Image stays on screen"
    );
    assert_eq!(app.status("Pixel size"), "320×240");
    // No longer a Sibling, but it keeps its place between a.jpg and c.jpg.
    assert_eq!(app.status("Sibling position"), "– / 2");

    // Treated as unsaved: moving on asks first (#35, §6.4); Discard lets it go.
    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "b.jpg •");
    app.control("Discard").click();
    app.settle();
    assert_eq!(app.status("File name"), "c.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 2");
}

#[test]
fn the_directory_is_watched_only_after_the_first_paint() {
    let (mut app, release) = TestApp::launch_held(&[SMALL]);
    assert!(!app.harness.state().is_watching_directory());

    release.send(()).unwrap();
    while app.status_field("File name").is_none() {
        app.harness.step();
        assert!(!app.harness.state().is_watching_directory());
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(!app.harness.state().is_watching_directory());

    app.settle();
    assert!(app.harness.state().is_watching_directory());
}
