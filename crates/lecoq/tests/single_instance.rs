//! Single instance (spec §8.4, §1.3 step 2): with *Allow multiple instances* off, a starting lecoq
//! hands its path to the running one, which raises its window and opens it (asking about unsaved
//! changes first). The socket itself is `lecoq-platform`'s `tests/single_instance_socket.rs`.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Key, ViewportCommand, ViewportId};
use egui_kittest::kittest::Queryable;
use lecoq::Launch;
use lecoq_platform::{MemoryConfigStore, MemoryInstance, Platform};
use support::TestApp;

const SMALL: &str = "small-320x240.jpg";
const LARGE: &str = "large-2400x1600.jpg";

fn platform_with(instance: &Arc<MemoryInstance>) -> Platform {
    Platform::in_memory().with_instance(instance.clone())
}

/// What startup hands the app with this `config.toml` setting and this instance socket.
fn launch(allow_multiple_instances: bool, instance: &Arc<MemoryInstance>) -> Launch {
    let config = format!("version = 1\nallow_multiple_instances = {allow_multiple_instances}\n");
    let platform =
        platform_with(instance).with_config(Arc::new(MemoryConfigStore::with_text(config)));
    Launch::new(platform)
}

fn sent(app: &TestApp, wanted: &ViewportCommand) -> bool {
    app.harness
        .output()
        .viewport_output
        .get(&ViewportId::ROOT)
        .is_some_and(|v| v.commands.contains(wanted))
}

// ---- Startup (§1.3 step 2) ----

#[test]
fn with_multiple_instances_allowed_startup_never_touches_the_socket() {
    let instance = Arc::new(MemoryInstance::with_running_instance());

    let exits = launch(true, &instance).handed_off(Some(Path::new("/a.png")));

    assert!(!exits);
    assert!(instance.offered().is_empty());
}

#[test]
fn with_multiple_instances_off_a_running_instance_takes_the_path_and_this_process_exits() {
    let instance = Arc::new(MemoryInstance::with_running_instance());

    let exits = launch(false, &instance).handed_off(Some(Path::new("/a.png")));

    assert!(exits);
    assert_eq!(instance.offered(), vec![Some(PathBuf::from("/a.png"))]);
}

#[test]
fn with_multiple_instances_off_and_nothing_running_this_process_opens_its_window() {
    let instance = Arc::new(MemoryInstance::default());

    let exits = launch(false, &instance).handed_off(Some(Path::new("/a.png")));

    assert!(!exits);
    assert_eq!(instance.offered().len(), 1);
}

// ---- The running window ----

#[test]
fn a_path_handed_over_raises_the_window_and_opens_it() {
    let instance = Arc::new(MemoryInstance::default());
    let mut app = TestApp::with_platform(platform_with(&instance));
    let path = app.add_fixtures(&[SMALL]);

    instance.arrive(Some(path));
    app.harness.step();

    assert!(sent(&app, &ViewportCommand::Focus));
    assert!(sent(&app, &ViewportCommand::Minimized(false)));
    app.settle();
    assert_eq!(app.status("File name"), SMALL);
    assert_eq!(app.status("Pixel size"), "320×240");
}

#[test]
fn a_process_started_without_a_file_only_raises_the_window() {
    let instance = Arc::new(MemoryInstance::default());
    let mut app = TestApp::with_platform(platform_with(&instance));
    let path = app.add_fixtures(&[SMALL]);
    app.open(path);

    instance.arrive(None);
    app.harness.step();

    assert!(sent(&app, &ViewportCommand::Focus));
    app.settle();
    assert_eq!(app.status("File name"), SMALL);
}

#[test]
fn a_path_handed_over_with_unsaved_changes_asks_first() {
    let instance = Arc::new(MemoryInstance::default());
    let mut app = TestApp::with_platform(platform_with(&instance));
    app.add_fixtures(&[SMALL, LARGE]);
    app.open(app.dir.path().join(SMALL));
    app.press(Key::R);
    app.drag_px((10.0, 10.0), (60.0, 40.0));
    app.press(Key::Escape);

    instance.arrive(Some(app.dir.path().join(LARGE)));
    app.harness.step();
    assert!(sent(&app, &ViewportCommand::Focus));
    app.settle();

    app.harness.get_by_label("Save changes?");
    assert!(app.status("File name").starts_with(SMALL));

    app.control("Discard").click();
    app.settle();
    assert_eq!(app.status("File name"), LARGE);
}

#[test]
fn a_path_handed_over_while_a_dialog_is_up_opens_once_it_closes() {
    let instance = Arc::new(MemoryInstance::default());
    let mut app = TestApp::with_platform(platform_with(&instance));
    app.add_fixtures(&[SMALL, LARGE]);
    app.open(app.dir.path().join(SMALL));
    app.harness
        .state_mut()
        .execute(lecoq::Command::ShowResizeDialog);
    app.settle();
    assert!(app.harness.state().is_resize_dialog_open());

    instance.arrive(Some(app.dir.path().join(LARGE)));
    app.settle();
    assert!(app.harness.state().is_resize_dialog_open());
    assert_eq!(app.status("File name"), SMALL);

    app.control("Cancel").click();
    app.settle();
    assert_eq!(app.status("File name"), LARGE);
}
