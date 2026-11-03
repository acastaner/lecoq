//! Animated GIF and WebP play in the viewer (spec §3.4).
//!
//! The fixtures (`tools/fixtures/make_fixtures.py`) show COLOUR for 100 ms, then for 300 ms a frame
//! whose left half is OTHER. The GIF stores that second frame as the left half only, drawn over the
//! first: its right half on screen shows the decoder composited it.

mod support;

use egui::Color32;
use support::TestApp;

const COLOUR: Color32 = Color32::from_rgb(40, 120, 200);
const OTHER: Color32 = Color32::from_rgb(200, 40, 40);

const ANIMATED: &str = "▶ animated";

/// WebP's encoder shifts a flat colour by a step or two, even lossless.
fn close(a: Color32, b: Color32) -> bool {
    a.to_array()
        .iter()
        .zip(b.to_array())
        .all(|(x, y)| x.abs_diff(y) <= 4)
}

#[test]
fn an_animated_gif_says_so_in_the_status_bar() {
    let app = TestApp::open_fixture("anim-partial-64x48.gif");
    assert_eq!(app.status_field("Animation").as_deref(), Some(ANIMATED));
}

/// Step the app frame by frame for `seconds` of egui time, 10 ms apart, and return the times at
/// which the on-screen pixel at `watched` changed, with its new colour. Also checks that the pixel
/// at `still` keeps `still_colour` throughout.
fn watch(
    app: &mut TestApp,
    seconds: f64,
    watched: (u32, u32),
    still: (u32, u32),
    still_colour: Color32,
) -> Vec<(f64, Color32)> {
    let start = app.harness.ctx.input(|i| i.time);
    let mut changes = Vec::new();
    let mut last = app.pixel(watched.0, watched.1);
    let mut t = start;
    while t < start + seconds {
        t += 0.01;
        app.harness.input_mut().time = Some(t);
        app.harness.step();
        let now = app.pixel(watched.0, watched.1);
        if now != last {
            changes.push((t, now.expect("a pixel")));
            last = now;
        }
        let kept = app.pixel(still.0, still.1).expect("a pixel");
        assert!(close(kept, still_colour), "{kept:?} at {t}");
    }
    changes
}

/// The animation shows COLOUR for 100 ms and OTHER for 300 ms, over and over.
fn assert_plays_its_own_timing(changes: &[(f64, Color32)]) {
    assert!(
        changes.len() >= 4,
        "expected a few frame changes, got {changes:?}"
    );
    for pair in changes.windows(2) {
        let ((t0, shown), (t1, _)) = (pair[0], pair[1]);
        assert!(
            close(shown, OTHER) || close(shown, COLOUR),
            "unexpected colour {shown:?}"
        );
        let expected = if close(shown, OTHER) { 0.3 } else { 0.1 };
        assert!(
            (t1 - t0 - expected).abs() < 0.015,
            "{shown:?} shown for {:.3} s, expected {expected} s: {changes:?}",
            t1 - t0
        );
    }
}

#[test]
fn an_animated_gif_plays_composited_frames_at_their_own_timing() {
    let mut app = TestApp::open_fixture("anim-partial-64x48.gif");
    // The right half is only in frame 1: the decoder draws frame 2 over it.
    let changes = watch(&mut app, 2.0, (8, 24), (56, 24), COLOUR);
    assert_plays_its_own_timing(&changes);
}

#[test]
fn an_animated_webp_plays_at_its_own_timing() {
    let mut app = TestApp::open_fixture("anim-64x48.webp");
    assert_eq!(app.status_field("Animation").as_deref(), Some(ANIMATED));
    let changes = watch(&mut app, 2.0, (8, 24), (56, 24), COLOUR);
    assert_plays_its_own_timing(&changes);
}

#[test]
fn moving_to_a_sibling_stops_the_animation_and_frees_its_frames() {
    let mut app = TestApp::new();
    app.add_fixture_as("anim-partial-64x48.gif", "a.gif");
    app.add_fixture_as("png-64x48.png", "b.png");
    app.open(app.dir.path().join("a.gif"));
    assert_eq!(app.harness.state().animation_frames(), 2);

    app.press(egui::Key::ArrowRight);
    assert_eq!(app.status("File name"), "b.png");
    assert_eq!(app.status_field("Animation"), None);
    assert_eq!(app.harness.state().animation_frames(), 0);
    let changes = watch(&mut app, 0.5, (8, 24), (56, 24), COLOUR);
    assert!(changes.is_empty(), "{changes:?}");

    // Back again: it plays from the start.
    app.press(egui::Key::ArrowLeft);
    assert_eq!(app.status_field("Animation").as_deref(), Some(ANIMATED));
    let changes = watch(&mut app, 2.0, (8, 24), (56, 24), COLOUR);
    assert_plays_its_own_timing(&changes);
}

// ---- Memory: the frames share the display-size cache's budget (§1.2) ----

/// Six 64×48 frames of 100 ms, each one flat colour.
const FRAME_COLOURS: [Color32; 6] = [
    Color32::from_rgb(200, 40, 40),
    Color32::from_rgb(40, 200, 40),
    Color32::from_rgb(40, 40, 200),
    Color32::from_rgb(200, 200, 40),
    Color32::from_rgb(40, 200, 200),
    Color32::from_rgb(200, 40, 200),
];

/// The bytes one 64×48 frame (or the display decode of the GIF) holds: RGBA.
const FRAME_BYTES: usize = 64 * 48 * 4;

/// Write a GIF of [`FRAME_COLOURS`] into `dir` as `name`.
fn six_frame_gif(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    use image::codecs::gif::{GifEncoder, Repeat};
    let path = dir.join(name);
    let mut encoder = GifEncoder::new(std::fs::File::create(&path).unwrap());
    encoder.set_repeat(Repeat::Infinite).unwrap();
    let frames = FRAME_COLOURS.iter().map(|c| {
        let [r, g, b, a] = c.to_array();
        image::Frame::from_parts(
            image::RgbaImage::from_pixel(64, 48, image::Rgba([r, g, b, a])),
            0,
            0,
            image::Delay::from_numer_denom_ms(100, 1),
        )
    });
    encoder.encode_frames(frames).unwrap();
    path
}

/// The frame colours that appear at (8, 24) over two loops of the six frames.
fn colours_shown(app: &mut TestApp) -> Vec<usize> {
    let mut seen = Vec::new();
    let mut t = app.harness.ctx.input(|i| i.time);
    let end = t + 1.4;
    while t < end {
        let colour = app.pixel(8, 24).expect("a pixel");
        let i = FRAME_COLOURS
            .iter()
            .position(|&f| close(f, colour))
            .unwrap_or_else(|| panic!("unexpected colour {colour:?}"));
        if !seen.contains(&i) {
            seen.push(i);
        }
        t += 0.01;
        app.harness.input_mut().time = Some(t);
        app.harness.step();
    }
    seen.sort();
    seen
}

#[test]
fn an_animation_keeps_only_the_frames_that_fit_the_display_cache_budget() {
    // Room for four frames' worth: the file's display decode plus three animation frames.
    let mut app = TestApp::with_launch(|l| l.cache_cap = 4 * FRAME_BYTES);
    let path = six_frame_gif(app.dir.path(), "six.gif");
    app.open(path);
    assert_eq!(app.status_field("Animation").as_deref(), Some(ANIMATED));

    assert_eq!(
        colours_shown(&mut app),
        [0, 1, 2],
        "it loops over the first three"
    );
}

#[test]
fn with_room_for_every_frame_the_whole_animation_plays() {
    let mut app = TestApp::with_launch(|l| l.cache_cap = 7 * FRAME_BYTES);
    let path = six_frame_gif(app.dir.path(), "six.gif");
    app.open(path);

    assert_eq!(colours_shown(&mut app), [0, 1, 2, 3, 4, 5]);
}

#[test]
fn an_animations_frames_leave_less_room_for_prefetched_siblings() {
    // Room for the GIF's display decode, its six frames and one 64×48 RGB Sibling, not two.
    let cap = 7 * FRAME_BYTES + 64 * 48 * 3;
    let mut app = TestApp::with_launch(|l| l.cache_cap = cap);
    let gif = six_frame_gif(app.dir.path(), "1.gif");
    for name in ["2.png", "3.png"] {
        image::RgbImage::from_pixel(64, 48, image::Rgb([90, 90, 90]))
            .save(app.dir.path().join(name))
            .unwrap();
    }
    app.open(gif);
    app.harness.step();

    let cached = app.harness.state().cached_siblings();
    let siblings = cached
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .count();
    assert_eq!(siblings, 1, "{cached:?}");
}

#[test]
fn a_still_webp_has_no_animation_state_and_stays_still() {
    let mut app = TestApp::open_fixture("webp-64x48.webp");
    assert_eq!(app.status_field("Animation"), None);
    let right = app.pixel(56, 24).expect("a pixel");
    let changes = watch(&mut app, 0.5, (8, 24), (56, 24), right);
    assert!(changes.is_empty(), "{changes:?}");
}
