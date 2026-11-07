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
fn home_and_end_jump_to_the_first_and_last_sibling() {
    let mut app = TestApp::open_in_folder(SMALL, &["a.jpg", "b.jpg", "c.jpg", "d.jpg"], "b.jpg");
    assert_eq!(app.status("Sibling position"), "2 / 4");

    app.press(Key::End);
    assert_eq!(app.status("File name"), "d.jpg");
    assert_eq!(app.status("Sibling position"), "4 / 4");

    app.press(Key::Home);
    assert_eq!(app.status("File name"), "a.jpg");
    assert_eq!(app.status("Sibling position"), "1 / 4");
}

#[test]
fn home_on_the_first_image_is_a_no_op() {
    let mut app = TestApp::open_in_folder(SMALL, &["a.jpg", "b.jpg", "c.jpg"], "a.jpg");
    assert_eq!(app.status_field("Message"), None);

    app.press(Key::Home);

    // No move, no wrap-around, and no "First image in folder" message (unlike ← at the start).
    assert_eq!(app.status("File name"), "a.jpg");
    assert_eq!(app.status("Sibling position"), "1 / 3");
    assert_eq!(app.status_field("Message"), None);
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

/// Upload-ready display-size bytes of the small opaque fixture: 320×240, premultiplied RGBA. The
/// cache holds upload-ready pixels (#56), so an opaque source (320×240 RGB when decoded) is a third
/// larger here than its decode format — this is the entry size the cap now counts.
const SMALL_BYTES: usize = 320 * 240 * 4;

/// What the same entry occupied before #56: the decoder's own RGB format. Kept to spell out the
/// trade-off the cap now makes (fewer, larger entries), not used as a cache size.
const SMALL_DECODE_FORMAT_BYTES: usize = 320 * 240 * 3;

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

#[test]
fn fewer_larger_upload_ready_entries_fit_under_the_cap() {
    // The cache now holds upload-ready pixels (#56): an opaque entry is premultiplied RGBA, a
    // third larger than the decoder's RGB. A cap that once held exactly three decode-format copies
    // of the fixture holds only two upload-ready ones — the trade-off is fewer Siblings, not a cap
    // that is silently overrun.
    let cap = 3 * SMALL_DECODE_FORMAT_BYTES;
    assert!(
        cap < 3 * SMALL_BYTES,
        "three upload-ready entries must not fit where three decode-format ones did"
    );
    let mut app = seven_with_cache_cap(cap);
    for _ in 0..4 {
        app.press(Key::ArrowRight);
        let cached = cached_names(&app);
        assert!(
            cached.len() <= 2,
            "the cap holds at most two upload-ready entries, got {cached:?}"
        );
        assert!(
            cached.contains(&app.status("File name")),
            "the current Image is never evicted"
        );
    }
}

#[test]
fn a_cached_switch_shows_the_siblings_pixels_opaque_and_translucent() {
    // An opaque PNG and a translucent PNG as Siblings in one folder. Opening the first prefetches
    // the second, so each switch is a cached one: it shares the cache's upload-ready pixels and
    // uploads them, converting nothing on the UI thread (#56). The pixels on screen must be the
    // same as on open (tests/formats.rs), opaque and translucent alike.
    // The fixtures' flat colour (tests/formats.rs); PNG is lossless, so an exact match but for a
    // rounding step.
    let close = |got: egui::Color32| {
        got.r().abs_diff(40) <= 1 && got.g().abs_diff(120) <= 1 && got.b().abs_diff(200) <= 1
    };

    let mut app = TestApp::new();
    let opaque = app.add_fixtures(&["png-64x48.png"]);
    app.add_fixtures(&["rgba-64x48.png"]);
    app.open(opaque);
    app.settle();
    assert!(
        cached_names(&app).contains(&"rgba-64x48.png".to_owned()),
        "the sibling is prefetched before the switch: {:?}",
        cached_names(&app)
    );

    // "png-…" sorts before "rgba-…", so Right moves to the cached translucent sibling.
    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "rgba-64x48.png");
    assert!(
        close(app.pixel(10, 24).unwrap()),
        "opaque half of the translucent PNG"
    );
    assert_eq!(
        app.pixel(54, 24).unwrap().a(),
        0,
        "transparent half stays transparent"
    );

    // Back to the opaque sibling, now cached: a flat colour.
    app.press(Key::ArrowLeft);
    assert_eq!(app.status("File name"), "png-64x48.png");
    assert!(
        close(app.pixel(32, 24).unwrap()),
        "the opaque PNG's flat colour"
    );
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
