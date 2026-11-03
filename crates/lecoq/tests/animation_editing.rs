//! Animated images while editing (spec §3.4, §5.3, §6.4): the first Annotation or Image operation
//! freezes the animation on frame 1, and saving an animation over its own file asks first.
//!
//! The fixtures (`tools/fixtures/make_fixtures.py`) show COLOUR for 100 ms, then for 300 ms a frame
//! whose left half is OTHER.

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use egui::{Color32, Key, Modifiers};
use egui_kittest::kittest::Queryable;
use lecoq_platform::{MemoryFileDialogs, Platform};
use support::TestApp;

const COLOUR: Color32 = Color32::from_rgb(40, 120, 200);
const OTHER: Color32 = Color32::from_rgb(200, 40, 40);

const PLAYING: &str = "▶ animated";
const FROZEN: &str = "⏸ frozen at frame 1";
const FROZEN_MESSAGE: &str = "Animation frozen at frame 1 for editing";
const TITLE: &str = "Overwrite animated image?";

/// WebP's encoder shifts a flat colour by a step or two, even lossless.
fn close(a: Color32, b: Color32) -> bool {
    a.to_array()
        .iter()
        .zip(b.to_array())
        .all(|(x, y)| x.abs_diff(y) <= 4)
}

/// A headless App with scripted file dialogs, showing `fixture` (copied into its temp dir).
fn open(fixture: &str) -> (TestApp, Arc<MemoryFileDialogs>, PathBuf) {
    let dialogs = Arc::new(MemoryFileDialogs::default());
    let mut platform = Platform::in_memory();
    platform.dialogs = dialogs.clone();
    let mut app = TestApp::with_platform(platform);
    let path = app.add_fixtures(&[fixture]);
    app.open(path.clone());
    (app, dialogs, path)
}

/// Step egui time forward (10 ms a frame) until the left half shows frame 2.
fn play_until_frame_2(app: &mut TestApp) {
    let mut t = app.harness.ctx.input(|i| i.time);
    for _ in 0..200 {
        if app.pixel(8, 24).is_some_and(|p| close(p, OTHER)) {
            return;
        }
        t += 0.01;
        app.harness.input_mut().time = Some(t);
        app.harness.step();
    }
    panic!("frame 2 never showed");
}

/// Let `seconds` of egui time pass; returns whether the pixel at (8, 24) changed meanwhile.
fn changes_over(app: &mut TestApp, seconds: f64) -> bool {
    let before = app.pixel(8, 24);
    let mut t = app.harness.ctx.input(|i| i.time);
    let end = t + seconds;
    let mut changed = false;
    while t < end {
        t += 0.01;
        app.harness.input_mut().time = Some(t);
        app.harness.step();
        changed |= app.pixel(8, 24) != before;
    }
    changed
}

/// A rectangle in the bottom right, away from the watched pixels.
fn draw_rectangle(app: &mut TestApp) {
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (60.0, 44.0));
    app.press(Key::Escape);
}

fn save(app: &mut TestApp) {
    app.key_with(Modifiers::COMMAND, Key::S);
}

fn prompt_shown(app: &TestApp) -> bool {
    app.harness.query_by_label(TITLE).is_some()
}

fn composite_at(app: &TestApp, x: u32, y: u32) -> Color32 {
    let image = app.composite_image();
    let [r, g, b, _] = image.get_pixel(x, y).0;
    Color32::from_rgb(r, g, b)
}

// ---- Freezing ----

#[test]
fn the_first_annotation_freezes_the_animation_on_frame_1() {
    let (mut app, _, _) = open("anim-partial-64x48.gif");
    play_until_frame_2(&mut app);

    draw_rectangle(&mut app);

    assert_eq!(app.status_field("Animation").as_deref(), Some(FROZEN));
    assert_eq!(app.status_field("Message").as_deref(), Some(FROZEN_MESSAGE));
    // Frame 1 is back on screen, under the Annotation, and stays there.
    assert!(close(app.pixel(8, 24).unwrap(), COLOUR));
    assert!(close(composite_at(&app, 8, 24), COLOUR));
    assert!(!changes_over(&mut app, 1.0));
    assert_eq!(app.harness.state().animation_frames(), 0);
}

#[test]
fn an_image_operation_freezes_the_animation_too() {
    let (mut app, _, _) = open("anim-64x48.webp");
    play_until_frame_2(&mut app);

    app.harness
        .state_mut()
        .execute(lecoq::Command::Flip { horizontal: true });
    app.settle();

    assert_eq!(app.status_field("Animation").as_deref(), Some(FROZEN));
    assert_eq!(app.status_field("Message").as_deref(), Some(FROZEN_MESSAGE));
    assert!(!changes_over(&mut app, 1.0));
    // Frame 1 is all COLOUR: flipped, it still is.
    assert!(close(composite_at(&app, 8, 24), COLOUR));
}

#[test]
fn undoing_the_edit_leaves_it_frozen() {
    let (mut app, _, _) = open("anim-partial-64x48.gif");
    draw_rectangle(&mut app);

    app.key_with(Modifiers::COMMAND, Key::Z);

    assert_eq!(app.status_field("Animation").as_deref(), Some(FROZEN));
    assert!(!changes_over(&mut app, 1.0));
}

#[test]
fn an_animation_not_edited_keeps_playing() {
    let (mut app, _, _) = open("anim-partial-64x48.gif");
    assert_eq!(app.status_field("Animation").as_deref(), Some(PLAYING));
    assert!(changes_over(&mut app, 1.0));
}

// ---- Saving over the animated file ----

#[test]
fn saving_an_edited_animation_over_its_own_file_asks_first() {
    let (mut app, _, path) = open("anim-64x48.webp");
    let before = std::fs::read(&path).unwrap();
    draw_rectangle(&mut app);

    save(&mut app);

    assert!(prompt_shown(&app));
    app.harness.get_by_label(
        "Saving will replace the animated file anim-64x48.webp with a still image. \
         All frames except the first will be lost.",
    );
    app.control("Cancel");
    app.control("Save As…");
    app.control("Overwrite");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn cancel_and_esc_write_nothing() {
    let (mut app, _, path) = open("anim-64x48.webp");
    let before = std::fs::read(&path).unwrap();
    draw_rectangle(&mut app);

    save(&mut app);
    app.control("Cancel").click();
    app.settle();
    assert!(!prompt_shown(&app));

    save(&mut app);
    assert!(prompt_shown(&app));
    app.press(Key::Escape);
    assert!(!prompt_shown(&app));
    // Esc closed the dialog only: the tool is still the select tool, the edit still unsaved.
    assert_eq!(app.status("File name"), "anim-64x48.webp •");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn overwrite_writes_a_still_image() {
    let (mut app, _, path) = open("anim-64x48.webp");
    draw_rectangle(&mut app);

    save(&mut app);
    app.control("Overwrite").click();
    app.settle();

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "anim-64x48.webp");

    // Opened again, the file is a still WebP showing frame 1 and the rectangle.
    let (mut again, _, _) = open("png-64x48.png");
    again.open(path);
    assert_eq!(again.status_field("Animation"), None);
    assert!(close(again.pixel(8, 24).unwrap(), COLOUR));
    assert!(!close(again.pixel(50, 37).unwrap(), COLOUR));
    assert!(!changes_over(&mut again, 1.0));
}

#[test]
fn save_as_from_the_prompt_writes_elsewhere() {
    let (mut app, dialogs, path) = open("anim-64x48.webp");
    let before = std::fs::read(&path).unwrap();
    draw_rectangle(&mut app);
    let target = path.with_file_name("still.png");
    dialogs.answer_next(Some(target.clone()));

    save(&mut app);
    app.control("Save As…").click();
    app.settle();

    assert_eq!(dialogs.opened(), 1);
    assert!(!prompt_shown(&app));
    assert!(target.exists());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(app.status("File name"), "still.png");
}

#[test]
fn save_as_onto_the_animated_file_asks_too() {
    let (mut app, dialogs, path) = open("anim-64x48.webp");
    draw_rectangle(&mut app);
    dialogs.answer_next(Some(path.clone()));

    app.key_with(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);

    assert!(prompt_shown(&app));
}

#[test]
fn saving_a_still_image_never_asks() {
    let (mut app, _, _) = open("webp-64x48.webp");
    draw_rectangle(&mut app);

    save(&mut app);

    assert!(!prompt_shown(&app));
    assert_eq!(app.status("File name"), "webp-64x48.webp");
}

#[test]
fn the_prompts_save_from_save_changes_waits_for_the_overwrite_answer() {
    let mut platform = Platform::in_memory();
    platform.dialogs = Arc::new(MemoryFileDialogs::default());
    let mut app = TestApp::with_platform(platform);
    app.add_fixture_as("anim-64x48.webp", "a.webp");
    app.add_fixture_as("png-64x48.png", "b.png");
    app.open(app.dir.path().join("a.webp"));
    draw_rectangle(&mut app);

    app.harness.key_press(Key::ArrowRight);
    app.harness.run();
    app.dialog_control("Save").click();
    app.harness.run();
    assert!(prompt_shown(&app));
    app.control("Overwrite").click();
    app.settle();

    assert_eq!(app.status("File name"), "b.png");
}
