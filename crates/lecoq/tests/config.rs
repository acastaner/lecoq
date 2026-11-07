//! `config.toml` (§8.3): defaults, the corrupt-file rename, debounced writes, the remembered window
//! and the *Allow multiple instances* toggle.

mod support;

use std::sync::Arc;

use lecoq::Launch;
use lecoq_platform::{Config, MemoryConfigStore, OsConfigStore, Platform};
use support::TestApp;

fn platform_with(store: &Arc<MemoryConfigStore>) -> Platform {
    Platform::in_memory().with_config(store.clone())
}

#[test]
fn keys_in_the_file_override_the_defaults_and_unknown_keys_are_ignored() {
    let store = Arc::new(MemoryConfigStore::with_text(
        "version = 1\n\
         jpeg_quality = 80\n\
         from_the_future = \"ignored\"\n\
         [window]\n\
         width = 1200.0\n",
    ));
    let app = TestApp::with_platform(platform_with(&store));

    let config = app.harness.state().config().clone();
    assert_eq!(config.jpeg_quality, 80);
    assert_eq!(config.window.width, 1200.0);
    assert_eq!(config.window.height, 1000.0);
    assert!(!config.window.maximised);
    assert!(config.allow_multiple_instances);
    assert_eq!(config.magnify_factor, 2.0);
}

#[test]
fn a_corrupt_file_is_set_aside_and_the_defaults_are_used() {
    let corrupt = "version = 1\njpeg_quality = = 80\n";
    let store = Arc::new(MemoryConfigStore::with_text(corrupt));
    let app = TestApp::with_platform(platform_with(&store));

    assert_eq!(app.harness.state().config(), &Config::default());
    assert_eq!(store.bad_text().as_deref(), Some(corrupt));
    assert_eq!(store.text(), None);
}

#[test]
fn a_known_key_with_the_wrong_type_makes_the_file_corrupt() {
    let store = Arc::new(MemoryConfigStore::with_text(
        "version = 1\njpeg_quality = \"high\"\n",
    ));
    let app = TestApp::with_platform(platform_with(&store));

    assert_eq!(app.harness.state().config(), &Config::default());
    assert!(store.bad_text().is_some());
}

#[test]
fn an_out_of_range_jpeg_quality_is_clamped_not_corrupt() {
    for (value, clamped) in [
        ("300", 100),
        ("0", 1),
        ("-5", 1),
        ("9223372036854775807", 100),
    ] {
        let store = Arc::new(MemoryConfigStore::with_text(format!(
            "version = 1\njpeg_quality = {value}\nmagnify_factor = 3.0\n"
        )));
        let app = TestApp::with_platform(platform_with(&store));

        let config = app.harness.state().config().clone();
        assert_eq!(config.jpeg_quality, clamped, "{value}");
        assert_eq!(
            config.magnify_factor, 3.0,
            "{value}: the rest of the file is kept"
        );
        assert_eq!(store.bad_text(), None, "{value}");
    }
}

#[test]
fn on_disk_a_corrupt_config_toml_is_renamed_to_config_toml_bad() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "not toml at all [").unwrap();
    let platform = Platform::in_memory().with_config(Arc::new(OsConfigStore::at(&path)));
    let app = TestApp::with_platform(platform);

    assert_eq!(app.harness.state().config(), &Config::default());
    assert!(!path.exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("config.toml.bad")).unwrap(),
        "not toml at all ["
    );
}

#[test]
fn the_window_size_is_remembered_for_the_next_run() {
    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(platform_with(&store));

    app.resize_window(egui::vec2(1280.0, 800.0), false);
    app.settle();

    let text = store.text().expect("config.toml written");
    assert!(text.starts_with("version = 1\n"), "{text}");
    let next = Launch::new(platform_with(&store));
    assert_eq!(
        (next.config.window.width, next.config.window.height),
        (1280.0, 800.0)
    );
    assert!(!next.config.window.maximised);
}

#[test]
fn the_maximised_state_is_remembered_with_the_restored_size() {
    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(platform_with(&store));

    app.resize_window(egui::vec2(1280.0, 800.0), false);
    app.resize_window(egui::vec2(1920.0, 1050.0), true);
    app.settle();

    let next = Launch::new(platform_with(&store));
    assert!(next.config.window.maximised);
    assert_eq!(
        (next.config.window.width, next.config.window.height),
        (1280.0, 800.0)
    );
}

#[test]
fn a_fullscreen_window_is_not_remembered() {
    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(platform_with(&store));

    app.resize_window(egui::vec2(1280.0, 800.0), false);
    app.resize_window(egui::vec2(1920.0, 1050.0), true);
    app.settle();
    app.press(egui::Key::F11);
    // The fullscreen window fills the screen and is no longer maximised.
    app.resize_window(egui::vec2(1920.0, 1080.0), false);
    app.settle();

    assert!(app.harness.state().is_fullscreen());
    let next = Launch::new(platform_with(&store));
    assert!(next.config.window.maximised);
    assert_eq!(
        (next.config.window.width, next.config.window.height),
        (1280.0, 800.0)
    );
}

#[test]
fn on_disk_the_config_is_written_whole_with_no_temporary_file_left() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lecoq").join("config.toml");
    let platform = Platform::in_memory().with_config(Arc::new(OsConfigStore::at(&path)));
    let mut app = TestApp::with_platform(platform);

    app.resize_window(egui::vec2(1280.0, 800.0), false);
    app.settle();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("version = 1\n"), "{text}");
    let written = Config::parse(&text).unwrap();
    assert_eq!(written.window.width, 1280.0);
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["config.toml"]);
}

#[test]
fn the_allow_multiple_instances_toggle_in_the_view_menu_persists() {
    use egui_kittest::kittest::Queryable;

    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(platform_with(&store));

    app.harness.get_by_label("View").click();
    app.harness.run();
    app.harness.get_by_label("Allow multiple instances").click();
    app.settle();

    assert!(!app.harness.state().config().allow_multiple_instances);
    let next = Launch::new(platform_with(&store));
    assert!(!next.config.allow_multiple_instances);
}

#[test]
fn writes_are_debounced() {
    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(platform_with(&store));

    for width in [1100.0, 1150.0, 1200.0, 1250.0] {
        app.resize_window(egui::vec2(width, 800.0), false);
    }
    assert_eq!(store.write_count(), 0, "written before the debounce delay");
    app.settle();

    assert_eq!(store.write_count(), 1);
    assert_eq!(app.harness.state().config().window.width, 1250.0);
}

#[test]
fn the_last_target_folder_survives_a_save_and_load_round_trip() {
    use std::path::PathBuf;

    for last_target in [Some(PathBuf::from("/home/me/Pictures/edited")), None] {
        let config = Config {
            last_target: last_target.clone(),
            ..Config::default()
        };
        let reloaded = Config::parse(&config.to_toml()).unwrap();
        assert_eq!(reloaded.last_target, last_target);
    }
}

#[test]
fn a_file_with_no_last_target_key_loads_with_none() {
    let store = Arc::new(MemoryConfigStore::with_text(
        "version = 1\njpeg_quality = 80\n",
    ));
    let app = TestApp::with_platform(platform_with(&store));

    assert_eq!(app.harness.state().config().last_target, None);
}

#[test]
fn no_file_gives_the_defaults() {
    let app = TestApp::new();

    let config = app.harness.state().config().clone();
    assert_eq!(config, Config::default());
    assert!(config.allow_multiple_instances);
    assert_eq!(config.jpeg_quality, 92);
    assert_eq!(
        (config.window.width, config.window.height),
        (1600.0, 1000.0)
    );
    assert!(!config.window.maximised);
    assert_eq!(config.magnify_factor, 2.0);
}
