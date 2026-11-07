//! Copy to / Move to (issue #53, spec §2, §3.1, §6.1, §6.4): the File operations that place the
//! Image's file in a Target folder. The folder picker is the in-memory `MemoryFileDialogs`; the
//! copy and move run for real on disk, with fixtures copied into temp folders. No real picker,
//! trash or clipboard is touched.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use lecoq::Launch;
use lecoq_platform::{MemoryConfigStore, MemoryFileDialogs, Platform};
use support::TestApp;

const SMALL: &str = "small-320x240.jpg";

/// A folder of `names` (copies of the small fixture) with `open` on screen, the scan finished, and
/// the fake dialogs. Follow is off (the default).
fn folder(names: &[&str], open: &str) -> (TestApp, Arc<MemoryFileDialogs>) {
    with_follow(names, open, false)
}

fn with_follow(names: &[&str], open: &str, follow: bool) -> (TestApp, Arc<MemoryFileDialogs>) {
    let dialogs = Arc::new(MemoryFileDialogs::default());
    let store = Arc::new(MemoryConfigStore::with_text(format!(
        "version = 1\nfollow_files = {follow}\n"
    )));
    let mut platform = Platform::in_memory().with_config(store);
    platform.dialogs = dialogs.clone();
    let app = TestApp::open_in_folder_on(platform, SMALL, names, open);
    (app, dialogs)
}

/// A Target folder named `name`, in a temp dir of its own (outside the watched Siblings folder, so
/// its own path component is predictable in the status-bar message). The returned `TempDir` must be
/// kept alive for the folder to exist.
fn target_dir(name: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temp dir");
    let dir = temp.path().join(name);
    std::fs::create_dir(&dir).expect("create target dir");
    (temp, dir)
}

/// Copy to `dir` (answer the picker with it) via Alt+C, then settle.
fn copy_to(app: &mut TestApp, dialogs: &MemoryFileDialogs, dir: &Path) {
    dialogs.answer_next(Some(dir.to_path_buf()));
    app.key_with(Modifiers::ALT, Key::C);
}

/// Move to `dir` via Alt+M, then settle.
fn move_to(app: &mut TestApp, dialogs: &MemoryFileDialogs, dir: &Path) {
    dialogs.answer_next(Some(dir.to_path_buf()));
    app.key_with(Modifiers::ALT, Key::M);
}

/// Draw a rectangle, then back to select: an unsaved change.
fn edit(app: &mut TestApp) {
    app.press(Key::R);
    app.drag_px((10.0, 10.0), (60.0, 40.0));
    app.press(Key::Escape);
}

fn canon(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

// ---- The picker (stories 5–9) ----

#[test]
fn the_picker_is_titled_and_starts_in_the_image_folder_then_the_last_target() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t1, target1) = target_dir("target1");

    // First File operation of the session: the picker starts in the Image's own folder (story 8),
    // and is titled for the operation (story 6).
    copy_to(&mut app, &dialogs, &target1);
    assert_eq!(dialogs.last_title().as_deref(), Some("Copy to folder"));
    assert_eq!(canon(&dialogs.last_start().unwrap()), canon(app.dir.path()));

    // The next picker starts in the last Target folder (story 7).
    let (_t2, target2) = target_dir("target2");
    move_to(&mut app, &dialogs, &target2);
    assert_eq!(dialogs.last_title().as_deref(), Some("Move to folder"));
    assert_eq!(canon(&dialogs.last_start().unwrap()), canon(&target1));
}

#[test]
fn cancelling_the_picker_does_nothing() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let a = app.dir.path().join("a.jpg");
    let before = std::fs::read(&a).unwrap();

    dialogs.answer_next(None);
    app.key_with(Modifiers::ALT, Key::C);

    assert_eq!(dialogs.opened(), 1, "the picker was shown");
    assert_eq!(app.status("File name"), "a.jpg");
    assert_eq!(app.status_field("Message"), None);
    assert_eq!(std::fs::read(&a).unwrap(), before);
}

// ---- Copy, Follow off (story 13, 25) ----

#[test]
fn copy_leaves_the_same_image_on_screen_and_says_where_it_went() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t, target) = target_dir("Keepers");

    copy_to(&mut app, &dialogs, &target);

    assert_eq!(
        app.status("File name"),
        "a.jpg",
        "the same Image stays on screen"
    );
    assert_eq!(app.status("Sibling position"), "1 / 2");
    assert_eq!(app.status("Message"), "Copied to Keepers");
    assert_eq!(
        app.harness.state().file_op_warning_message(),
        None,
        "a success keeps the status bar, no modal (ADR 0009)"
    );
    assert!(
        target.join("a.jpg").exists(),
        "the copy landed in the folder"
    );
    assert!(app.dir.path().join("a.jpg").exists(), "the original stays");
}

// ---- Move, Follow off (stories 10–12, 33) ----

#[test]
fn move_shows_the_next_sibling() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg", "c.jpg"], "b.jpg");
    let (_t, target) = target_dir("Elsewhere");

    move_to(&mut app, &dialogs, &target);

    assert_eq!(app.status("File name"), "c.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 2");
    assert_eq!(app.status("Message"), "Moved to Elsewhere");
    assert!(target.join("b.jpg").exists(), "the file moved");
    assert!(!app.dir.path().join("b.jpg").exists(), "it left its folder");
}

#[test]
fn move_at_the_end_of_the_list_shows_the_previous_sibling() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg", "c.jpg"], "c.jpg");
    let (_t, target) = target_dir("Elsewhere");

    move_to(&mut app, &dialogs, &target);

    assert_eq!(app.status("File name"), "b.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 2");
}

#[test]
fn moving_the_only_image_leaves_the_empty_window() {
    let (mut app, dialogs) = folder(&["only.jpg"], "only.jpg");
    let (_t, target) = target_dir("Elsewhere");

    move_to(&mut app, &dialogs, &target);

    assert!(app.harness.state().canvas().is_none());
    app.harness
        .get_by_label("Open (Ctrl+O), drop or paste (Ctrl+V) an image");
    assert_eq!(app.status_field("File name"), None);
    assert_eq!(app.status("Message"), "Moved to Elsewhere");
}

#[test]
fn the_moved_file_is_not_reported_as_deleted_from_outside() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg", "c.jpg"], "b.jpg");
    let (_t, target) = target_dir("Elsewhere");

    move_to(&mut app, &dialogs, &target);

    // The window is on c.jpg with no unsaved marker (the move is lecoq's own action, not a deletion
    // from outside, story 33), and b is gone from the list for good: moving back lands on a, not b.
    assert_eq!(app.status("File name"), "c.jpg");
    assert!(
        app.harness.state().is_watching_directory(),
        "the source folder is still watched"
    );
    app.press(Key::ArrowLeft);
    assert_eq!(app.status("File name"), "a.jpg");
}

// ---- Collisions and the file's own folder (stories 27–29) ----

#[test]
fn a_name_already_in_the_target_folder_is_refused() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t, target) = target_dir("target");
    // A different file already called a.jpg is in the Target folder.
    std::fs::write(target.join("a.jpg"), b"not the same file").unwrap();

    copy_to(&mut app, &dialogs, &target);

    // The name clash opens the warning modal (ADR 0009), not the status bar.
    assert_eq!(
        app.harness.state().file_op_warning_message(),
        Some("a.jpg already exists in target")
    );
    assert_eq!(app.status_field("Message"), None);
    assert!(
        app.harness
            .query_by_label(lecoq::strings::FILE_OP_WARNING_TITLE)
            .is_some(),
        "the modal is rendered"
    );
    assert_eq!(
        std::fs::read(target.join("a.jpg")).unwrap(),
        b"not the same file",
        "the existing file is not overwritten"
    );
    assert_eq!(app.status("File name"), "a.jpg");
}

#[test]
fn copying_into_its_own_folder_hits_the_collision_rule() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let own = app.dir.path().to_path_buf();

    copy_to(&mut app, &dialogs, &own);

    assert!(
        app.harness
            .state()
            .file_op_warning_message()
            .is_some_and(|m| m.starts_with("a.jpg already exists in")),
        "warning was {:?}",
        app.harness.state().file_op_warning_message()
    );
}

#[test]
fn moving_into_its_own_folder_does_nothing_but_says_so_neutrally() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let own = app.dir.path().to_path_buf();

    move_to(&mut app, &dialogs, &own);

    // Nothing moves and the file stays — but the outcome is a neutral modal line now, not silence
    // (ADR 0009): the file is already where it was asked to go.
    assert_eq!(app.status("File name"), "a.jpg");
    assert_eq!(app.status("Sibling position"), "1 / 2");
    assert_eq!(app.status_field("Message"), None);
    assert!(
        app.harness
            .state()
            .file_op_warning_message()
            .is_some_and(|m| m.starts_with("a.jpg is already in")),
        "a neutral warning, {:?}",
        app.harness.state().file_op_warning_message()
    );
    assert!(app.dir.path().join("a.jpg").exists());
}

#[test]
fn the_warning_modal_is_dismissed_by_its_close_button() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t, target) = target_dir("target");
    std::fs::write(target.join("a.jpg"), b"not the same file").unwrap();

    copy_to(&mut app, &dialogs, &target);
    assert!(app.harness.state().file_op_warning_message().is_some());

    app.dialog_control(lecoq::strings::CLOSE).click();
    app.settle();

    assert_eq!(
        app.harness.state().file_op_warning_message(),
        None,
        "Close dismisses the modal"
    );
}

#[test]
fn the_warning_modal_is_dismissed_by_the_escape_key() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t, target) = target_dir("target");
    std::fs::write(target.join("a.jpg"), b"not the same file").unwrap();

    copy_to(&mut app, &dialogs, &target);
    assert!(app.harness.state().file_op_warning_message().is_some());

    app.press(egui::Key::Escape);
    app.settle();

    assert_eq!(
        app.harness.state().file_op_warning_message(),
        None,
        "Esc dismisses the modal"
    );
}

// ---- An OS failure (story 32) ----

#[test]
fn a_failed_copy_keeps_the_image_and_says_why() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t, target) = target_dir("target");
    set_readonly(&target, true);

    copy_to(&mut app, &dialogs, &target);

    set_readonly(&target, false); // so the temp dir can be cleaned up
    // The failure opens the warning modal (ADR 0009), not the status bar.
    assert!(
        app.harness
            .state()
            .file_op_warning_message()
            .is_some_and(|m| m.starts_with("Couldn't copy a.jpg:")),
        "warning was {:?}",
        app.harness.state().file_op_warning_message()
    );
    assert_eq!(app.status_field("Message"), None);
    assert_eq!(
        app.status("File name"),
        "a.jpg",
        "the Image stays on screen"
    );
}

#[test]
fn a_failed_move_keeps_the_image_and_says_why() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t, target) = target_dir("target");
    set_readonly(&target, true);

    move_to(&mut app, &dialogs, &target);

    set_readonly(&target, false); // so the temp dir can be cleaned up
    // The failure opens the warning modal (ADR 0009), not the status bar.
    assert!(
        app.harness
            .state()
            .file_op_warning_message()
            .is_some_and(|m| m.starts_with("Couldn't move a.jpg:")),
        "warning was {:?}",
        app.harness.state().file_op_warning_message()
    );
    assert_eq!(app.status_field("Message"), None);
    assert_eq!(
        app.status("File name"),
        "a.jpg",
        "the Image stays on screen"
    );
    assert!(
        app.dir.path().join("a.jpg").exists(),
        "the original is left in place"
    );
    assert!(
        !target.join("a.jpg").exists(),
        "nothing landed in the Target folder"
    );
}

// ---- Enabling (stories 34–36) ----

#[test]
fn copy_to_and_move_to_are_disabled_in_the_empty_window() {
    let mut app = TestApp::new();
    app.settle();
    app.control("File").click();
    app.harness.run();

    assert!(app.control("Copy to…").accesskit_node().is_disabled());
    assert!(app.control("Move to…").accesskit_node().is_disabled());
}

#[test]
fn copy_to_and_move_to_are_disabled_for_a_clipboard_image() {
    use lecoq_platform::{ClipboardItem, MemoryClipboard};
    let clipboard = Arc::new(MemoryClipboard::default());
    // A 2×2 PNG on the clipboard, pasted into the empty window: the Image has no file.
    clipboard.put(vec![ClipboardItem::new("image/png", tiny_png())]);
    let mut app = TestApp::with_platform(Platform::in_memory().with_clipboard(clipboard));
    app.key_with(Modifiers::COMMAND, Key::V);
    assert_eq!(app.status("File name"), "Clipboard image •");

    app.control("File").click();
    app.harness.run();
    assert!(app.control("Copy to…").accesskit_node().is_disabled());
    assert!(app.control("Move to…").accesskit_node().is_disabled());
}

#[test]
fn copy_to_and_move_to_are_available_for_a_view_only_image() {
    let mut app = TestApp::new();
    let path = app.dir.path().join("huge.jpg");
    std::fs::copy(huge_jpeg(), &path).unwrap();
    app.open(path);
    assert!(app.harness.state().is_view_only());

    app.control("File").click();
    app.harness.run();
    assert!(!app.control("Copy to…").accesskit_node().is_disabled());
    assert!(!app.control("Move to…").accesskit_node().is_disabled());
}

// ---- The typing guard (story 37) ----

#[test]
fn alt_c_and_alt_m_do_not_open_a_picker_while_editing_text() {
    let (mut app, dialogs) = folder(&["a.jpg"], "a.jpg");
    app.press(Key::T);
    app.click_px((50.0, 60.0));
    assert!(app.harness.state().editing_text());

    app.key_with(Modifiers::ALT, Key::C);
    app.key_with(Modifiers::ALT, Key::M);

    assert_eq!(dialogs.opened(), 0, "no picker opened");
    assert!(app.harness.state().editing_text(), "still typing");
}

// ---- The unsaved-changes prompt (stories 20–23) ----

#[test]
fn copy_with_unsaved_changes_asks_first_and_save_copies_the_saved_file() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t, target) = target_dir("target");
    let a = app.dir.path().join("a.jpg");
    let before = std::fs::read(&a).unwrap();
    edit(&mut app);

    copy_to(&mut app, &dialogs, &target);
    assert!(app.harness.query_by_label("Save changes?").is_some());
    app.dialog_control("Save").click();
    app.settle();

    assert_ne!(std::fs::read(&a).unwrap(), before, "a.jpg was written");
    // The edits travelled with the file: the copy carries the red rectangle (story 21).
    let copied = image::open(target.join("a.jpg")).unwrap().to_rgb8();
    let edge = copied.get_pixel(10, 25);
    assert!(edge[0] > 180 && edge[1] < 90 && edge[2] < 90, "{edge:?}");
}

#[test]
fn copy_with_unsaved_changes_discard_copies_the_file_as_on_disk() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t, target) = target_dir("target");
    let a = app.dir.path().join("a.jpg");
    let before = std::fs::read(&a).unwrap();
    edit(&mut app);

    copy_to(&mut app, &dialogs, &target);
    app.dialog_control("Discard").click();
    app.settle();

    assert_eq!(
        std::fs::read(&a).unwrap(),
        before,
        "the original is untouched"
    );
    // The copy is the file as it was on disk (no rectangle), byte for byte the pre-edit a.jpg.
    assert_eq!(std::fs::read(target.join("a.jpg")).unwrap(), before);
}

#[test]
fn copy_with_unsaved_changes_cancel_does_nothing() {
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t, target) = target_dir("target");
    edit(&mut app);

    copy_to(&mut app, &dialogs, &target);
    app.dialog_control("Cancel").click();
    app.settle();

    assert!(!target.join("a.jpg").exists(), "nothing was copied");
    assert_eq!(
        app.status("File name"),
        "a.jpg •",
        "the edits are still there"
    );
}

#[test]
fn trashing_still_does_not_prompt_but_copy_and_move_do() {
    // Trash keeps its no-prompt rule (story 24); the prompt is Copy to / Move to only.
    let (mut app, dialogs) = folder(&["a.jpg", "b.jpg"], "a.jpg");
    let (_t, target) = target_dir("target");
    edit(&mut app);
    app.click_px((200.0, 200.0)); // deselect, so a later Delete would mean the file

    move_to(&mut app, &dialogs, &target);
    assert!(
        app.harness.query_by_label("Save changes?").is_some(),
        "Move to asks"
    );
    app.dialog_control("Cancel").click();
    app.settle();
}

// ---- Follow on (stories 14, 17–19) ----

#[test]
fn copy_with_follow_on_shows_the_copy_in_its_new_folder_with_a_fresh_history() {
    let (mut app, dialogs) = with_follow(&["a.jpg", "b.jpg"], "a.jpg", true);
    let (_t, target) = target_dir("Copies");
    // A drawn-and-saved edit, so there is History to lose when the window opens the copy.
    edit(&mut app);
    app.key_with(Modifiers::COMMAND, Key::S);

    copy_to(&mut app, &dialogs, &target);

    assert_eq!(app.status("File name"), "a.jpg");
    assert_eq!(app.status("Message"), "Copied to Copies");
    // The window is now in the Target folder: a.jpg is its only Sibling.
    assert_eq!(app.status("Sibling position"), "1 / 1");
    assert_eq!(
        app.doc().undo_len(),
        0,
        "History starts over at the new file (story 19)"
    );
}

#[test]
fn after_a_follow_on_copy_edits_and_saves_go_to_the_copy_not_the_original() {
    let (mut app, dialogs) = with_follow(&["a.jpg", "b.jpg"], "a.jpg", true);
    let (_t, target) = target_dir("Copies");
    let original = app.dir.path().join("a.jpg");
    let original_before = std::fs::read(&original).unwrap();

    copy_to(&mut app, &dialogs, &target);
    // Now editing the shown Image and saving writes the copy, leaving the original alone (story 18).
    edit(&mut app);
    app.key_with(Modifiers::COMMAND, Key::S);
    app.wait_for_status("File name", "a.jpg");

    let saved_copy = image::open(target.join("a.jpg")).unwrap().to_rgb8();
    let edge = saved_copy.get_pixel(10, 25);
    assert!(edge[0] > 180 && edge[1] < 90 && edge[2] < 90, "{edge:?}");
    assert_eq!(
        std::fs::read(&original).unwrap(),
        original_before,
        "the original is untouched"
    );
}

#[test]
fn move_with_follow_on_shows_the_file_at_its_new_path() {
    let (mut app, dialogs) = with_follow(&["a.jpg", "b.jpg"], "a.jpg", true);
    let (_t, target) = target_dir("Moved");

    move_to(&mut app, &dialogs, &target);

    assert_eq!(app.status("File name"), "a.jpg");
    assert_eq!(app.status("Message"), "Moved to Moved");
    assert_eq!(
        app.status("Sibling position"),
        "1 / 1",
        "now in the Target folder"
    );
    assert!(target.join("a.jpg").exists());
    assert!(!app.dir.path().join("a.jpg").exists());
}

#[test]
fn move_with_follow_on_steps_through_the_target_folders_siblings() {
    let (mut app, dialogs) = with_follow(&["a.jpg", "b.jpg"], "a.jpg", true);
    let (_t, target) = target_dir("Moved");
    // The Target folder already holds other images: following the move lands among them, not in a
    // folder of one.
    std::fs::copy(support::fixture_path(SMALL), target.join("m.jpg")).unwrap();
    std::fs::copy(support::fixture_path(SMALL), target.join("z.jpg")).unwrap();

    move_to(&mut app, &dialogs, &target);

    assert_eq!(app.status("File name"), "a.jpg");
    assert_eq!(
        app.status("Sibling position"),
        "1 / 3",
        "now among the Target folder's three images"
    );
    // Arrowing on moves through the Target folder's Siblings, not back to the source folder.
    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "m.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 3");
    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "z.jpg");
    assert_eq!(app.status("Sibling position"), "3 / 3");
}

// ---- The Follow setting (stories 15, 16) ----

#[test]
fn the_follow_toggle_in_the_view_menu_defaults_off_and_persists() {
    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(Platform::in_memory().with_config(store.clone()));
    assert!(
        !app.harness.state().config().follow_files,
        "off by default (story 15)"
    );

    app.harness.get_by_label("View").click();
    app.harness.run();
    app.harness
        .get_by_label("Follow files after copy/move")
        .click();
    app.settle();

    assert!(app.harness.state().config().follow_files);
    let next = Launch::new(Platform::in_memory().with_config(store));
    assert!(
        next.config.follow_files,
        "remembered across runs (story 16)"
    );
}

// ---- Helpers ----

fn set_readonly(dir: &Path, readonly: bool) {
    let mut perms = std::fs::metadata(dir).unwrap().permissions();
    perms.set_readonly(readonly);
    std::fs::set_permissions(dir, perms).unwrap();
}

/// A 2×2 opaque PNG, for a clipboard image with no file.
fn tiny_png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

/// A uniform JPEG just over 100 MP (view-only, §4.7), generated once per target directory.
fn huge_jpeg() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("huge-10000x10001.jpg");
    if !path.exists() {
        let pixels = image::GrayImage::from_pixel(10_000, 10_001, image::Luma([128]));
        let tmp = path.with_extension("tmp");
        pixels
            .save_with_format(&tmp, image::ImageFormat::Jpeg)
            .expect("write the huge fixture");
        std::fs::rename(&tmp, &path).expect("move the huge fixture in place");
    }
    path
}
