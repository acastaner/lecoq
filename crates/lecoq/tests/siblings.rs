//! Sibling navigation (spec §3.1–3.3): the wheel and the arrow keys move through the other images
//! in the folder, in the file manager's order, with prefetch.

mod support;

use egui::Key;
use support::TestApp;

const SMALL: &str = "small-320x240.jpg";

#[test]
fn arrow_keys_move_through_siblings_in_glib_order() {
    let mut app =
        TestApp::open_in_folder(SMALL, &["img10.jpg", "img2.jpg", "img1.jpg"], "img1.jpg");
    assert_eq!(app.status("File name"), "img1.jpg");
    assert_eq!(app.status("Sibling position"), "1 / 3");

    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "img2.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 3");

    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "img10.jpg");
    assert_eq!(app.status("Sibling position"), "3 / 3");

    app.press(Key::ArrowLeft);
    assert_eq!(app.status("File name"), "img2.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 3");
}

#[test]
fn wheel_down_is_next_and_wheel_up_is_previous() {
    let mut app = TestApp::open_in_folder(SMALL, &["a.jpg", "b.jpg", "c.jpg"], "b.jpg");

    app.wheel(-1.0);
    assert_eq!(app.status("File name"), "c.jpg");
    app.wheel(1.0);
    assert_eq!(app.status("File name"), "b.jpg");
    app.wheel(1.0);
    assert_eq!(app.status("File name"), "a.jpg");
}

#[test]
fn shift_wheel_and_ctrl_wheel_do_not_move_to_a_sibling() {
    // Shift+wheel pans (#43) and Ctrl+wheel zooms (#18); only the plain wheel navigates.
    let mut app = TestApp::open_in_folder(SMALL, &["a.jpg", "b.jpg", "c.jpg"], "b.jpg");
    let over_canvas = egui::pos2(500.0, 300.0);

    app.wheel_at(over_canvas, -1.0, egui::Modifiers::SHIFT);
    assert_eq!(app.status("File name"), "b.jpg");

    app.wheel_at(over_canvas, 1.0, egui::Modifiers::COMMAND);
    assert_eq!(app.status("File name"), "b.jpg");
    assert_ne!(app.status("Zoom"), "Fit");

    app.wheel_at(over_canvas, -1.0, egui::Modifiers::NONE);
    assert_eq!(app.status("File name"), "c.jpg");
}

#[test]
fn no_wrap_around_at_the_ends() {
    let mut app = TestApp::open_in_folder(SMALL, &["a.jpg", "b.jpg"], "b.jpg");
    assert_eq!(app.status_field("Message"), None);

    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "b.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 2");
    assert_eq!(app.status("Message"), "Last image in folder");

    app.press(Key::ArrowLeft);
    assert_eq!(app.status("File name"), "a.jpg");
    assert_eq!(app.status_field("Message"), None);

    app.wheel(1.0);
    assert_eq!(app.status("File name"), "a.jpg");
    assert_eq!(app.status("Message"), "First image in folder");
}

#[test]
fn siblings_are_supported_extensions_case_insensitive_and_not_hidden() {
    let mut app = TestApp::open_in_folder(
        SMALL,
        &[
            "a.jpg",
            "B.JPEG",
            "c.Jpg",
            ".hidden.jpg",
            "notes.txt",
            "d.jpg.bak",
        ],
        "a.jpg",
    );
    std::fs::create_dir(app.dir.path().join("folder.jpg")).unwrap();
    app.open(app.dir.path().join("a.jpg"));
    assert_eq!(app.status("Sibling position"), "1 / 3");

    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "B.JPEG");
    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "c.Jpg");
    app.press(Key::ArrowRight);
    assert_eq!(app.status("Message"), "Last image in folder");
}

#[test]
fn position_is_unknown_until_the_scan_after_the_first_paint() {
    let (mut app, release) = TestApp::launch_held(&[SMALL, "large-2400x1600.jpg"]);
    release.send(()).unwrap();
    // Run frames until the first image is drawn. The scan only starts on the frame after it.
    while app.status_field("File name").is_none() {
        app.harness.step();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(app.status("Sibling position"), "? / ?");

    app.settle();
    // GLib order: "large…" before "small…".
    assert_eq!(app.status("Sibling position"), "2 / 2");
}

#[test]
fn previous_image_stays_on_screen_while_the_next_one_decodes() {
    let mut app = TestApp::open_in_folder(
        "large-2400x1600.jpg",
        &["1.jpg", "2.jpg", "3.jpg", "4.jpg", "5.jpg", "6.jpg"],
        "1.jpg",
    );
    // Jump past the prefetch window, so the targets aren't cached.
    for _ in 0..5 {
        app.harness.key_press(Key::ArrowRight);
        app.harness.step();
        assert!(
            app.harness.state().canvas().is_some(),
            "the canvas went blank"
        );
    }
    app.settle();
    assert_eq!(app.status("File name"), "6.jpg");
}

/// A folder of seven copies of the small fixture, `1.jpg` open, with the given cache cap.
fn seven_with_cache_cap(cap: usize) -> TestApp {
    let mut app = TestApp::with_launch(|launch| launch.cache_cap = cap);
    for n in 1..=7 {
        app.add_fixture_as(SMALL, &format!("{n}.jpg"));
    }
    app.open(app.dir.path().join("1.jpg"));
    app
}

/// Display-size bytes of the small fixture: 320×240 RGB.
const SMALL_BYTES: usize = 320 * 240 * 3;

#[test]
fn cache_stays_under_its_cap_evicting_least_recently_shown_first() {
    // Room for two and a half of the Siblings: two are kept.
    let mut app = seven_with_cache_cap(SMALL_BYTES * 5 / 2);
    for _ in 0..4 {
        app.press(Key::ArrowRight);
        let cached = cached_names(&app);
        assert!(cached.len() <= 2, "{cached:?}");
        let name = app.status("File name");
        assert!(cached_names(&app).contains(&name), "{name} was evicted");
    }
    // Least recently shown goes first: 1.jpg and 2.jpg are long gone, 5.jpg (current) stays.
    let cached = cached_names(&app);
    assert!(!cached.contains(&"1.jpg".to_owned()), "{cached:?}");
    assert!(!cached.contains(&"2.jpg".to_owned()), "{cached:?}");
}

#[test]
fn current_image_is_never_evicted_even_over_the_cap() {
    let mut app = seven_with_cache_cap(1);
    app.press(Key::ArrowRight);
    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "3.jpg");
    assert_eq!(cached_names(&app), ["3.jpg"]);
}

fn cached_names(app: &TestApp) -> Vec<String> {
    let paths = app.harness.state().cached_siblings();
    let mut names: Vec<String> = paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn prefetch_keeps_two_ahead_in_the_direction_of_travel_and_one_behind() {
    let names = [
        "1.jpg", "2.jpg", "3.jpg", "4.jpg", "5.jpg", "6.jpg", "7.jpg",
    ];
    let mut app = TestApp::open_in_folder(SMALL, &names, "1.jpg");
    // Forward at startup: the next two.
    assert_eq!(cached_names(&app), ["2.jpg", "3.jpg"]);

    app.press(Key::ArrowRight);
    app.press(Key::ArrowRight);
    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "4.jpg");
    let cached = cached_names(&app);
    for wanted in ["3.jpg", "4.jpg", "5.jpg", "6.jpg"] {
        assert!(
            cached.contains(&wanted.to_owned()),
            "{wanted} not in {cached:?}"
        );
    }

    // Turning round: the next two the other way, and one behind.
    app.press(Key::ArrowLeft);
    assert_eq!(app.status("File name"), "3.jpg");
    let cached = cached_names(&app);
    for wanted in ["1.jpg", "2.jpg", "3.jpg", "4.jpg"] {
        assert!(
            cached.contains(&wanted.to_owned()),
            "{wanted} not in {cached:?}"
        );
    }
}
