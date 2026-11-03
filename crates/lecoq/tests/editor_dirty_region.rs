//! Dirty-region rendering during a gesture (#46, §7.2): while an Annotation is dragged, only the
//! region that changed is re-rendered and re-uploaded, yet the composite the user sees stays
//! pixel-identical to a full re-render — except for an opaque neighbourhood effect (Obfuscate,
//! Highlight Magnify) that only partly overlaps the change, which is frozen for the duration of the
//! gesture and corrected by one full re-render when the gesture ends.
//!
//! These tests assert external behaviour through the composite seam (§9): the fast path is an
//! internal optimisation, so they add no production seam. The mid-gesture snapshots use the
//! test-only `drag_hold_px` primitive (press, move, run a frame, do not release).

mod support;

use egui::Key;
use lecoq::Command;
use lecoq_core::{Property, Rgba};
use support::TestApp;

/// A 320×240 Image, shown at 100% (Image pixels are screen points and composite pixels).
fn app_with_image() -> TestApp {
    TestApp::open_fixture("small-320x240.jpg")
}

/// Restyle the selection (or the tool's default) as the property bar does.
fn set(app: &mut TestApp, property: Property) {
    app.harness.state_mut().execute(Command::SetProperty {
        property,
        merge: false,
    });
    app.settle();
}

/// Every pixel of the composite inside the rectangle (x0, y0)–(x1, y1), premultiplied RGBA.
fn region_pixels(app: &TestApp, (x0, y0): (u32, u32), (x1, y1): (u32, u32)) -> Vec<[u8; 4]> {
    let c = app.harness.state().composite().expect("a composite");
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .map(|(x, y)| {
            let i = (y * c.width + x) as usize * 4;
            c.rgba_premultiplied[i..i + 4].try_into().unwrap()
        })
        .collect()
}

// ---- Fast-path equivalence: the mid-gesture composite matches a full static render ----

#[test]
fn a_rectangle_moved_mid_gesture_matches_the_same_rectangle_rendered_static() {
    // Moving: draw a rectangle, then drag-hold it to a new position and snapshot mid-gesture.
    let mut moving = app_with_image();
    moving.press(Key::R);
    moving.drag_px((40.0, 30.0), (140.0, 90.0));
    moving.press(Key::Escape);
    moving.drag_hold_px((40.0, 50.0), (90.0, 100.0)); // +50, +50 -> (90, 80)-(190, 140)
    assert!(moving.doc().in_gesture(), "the gesture is still open");
    assert_eq!(
        moving.selection_bounds(),
        Some([90, 80, 190, 140]),
        "the rectangle moved mid-gesture"
    );
    let mid = moving.composite_image();

    // Static: the same rectangle drawn directly at the moved position.
    let mut still = app_with_image();
    still.press(Key::R);
    still.drag_px((90.0, 80.0), (190.0, 140.0));
    let full = still.composite_image();

    support::assert_golden_image_against(&mid, &full);
}

#[test]
fn a_rectangle_moving_under_a_text_highlight_stays_live_and_matches_the_static_render() {
    // The highlight (Text mode: multiplies what is below it) sits over the region; a filled
    // rectangle beneath it is dragged so part moves under the highlight. A per-pixel highlight needs
    // no freeze: the fast path composites the full z-order within the dirty rectangle.
    let region = ((60.0, 40.0), (240.0, 160.0));

    let mut moving = app_with_image();
    moving.press(Key::R);
    moving.drag_px((100.0, 120.0), (200.0, 180.0));
    set(&mut moving, Property::FillColour(Some(Rgba::WHITE)));
    moving.press(Key::H); // highlight tool, default mode Text
    moving.drag_px(region.0, region.1);
    moving.press(Key::Escape);
    // Grab the rectangle below the highlight region and drag it up, under the highlight.
    moving.drag_hold_px((150.0, 175.0), (150.0, 135.0)); // -40 in y -> (100, 80)-(200, 140)
    assert!(moving.doc().in_gesture(), "the gesture is still open");
    let mid = moving.composite_image();

    let mut still = app_with_image();
    still.press(Key::R);
    still.drag_px((100.0, 80.0), (200.0, 140.0));
    set(&mut still, Property::FillColour(Some(Rgba::WHITE)));
    still.press(Key::H);
    still.drag_px(region.0, region.1);
    let full = still.composite_image();

    support::assert_golden_image_against(&mid, &full);
}

// ---- Freeze correctness: an opaque effect never leaks, and is corrected on release ----

/// The obfuscation's region (pixelate, over the top-left of a filled rectangle).
const OBF: ((f32, f32), (f32, f32)) = ((20.0, 20.0), (180.0, 100.0));

/// A white-filled, red-lined rectangle at (100, 60)–(220, 130) with a pixelate Obfuscation over
/// [`OBF`] on top of it, selected obfuscation deselected. The rectangle's top-left is under the
/// obfuscation.
fn obfuscation_over_rectangle() -> TestApp {
    let mut app = app_with_image();
    app.press(Key::R);
    app.drag_px((100.0, 60.0), (220.0, 130.0));
    set(&mut app, Property::FillColour(Some(Rgba::WHITE)));
    app.press(Key::O); // obfuscate tool, default mode Pixelate
    app.drag_px(OBF.0, OBF.1);
    app.press(Key::Escape);
    app
}

#[test]
fn an_obfuscation_over_a_moving_rectangle_is_frozen_intact_mid_gesture() {
    let mut app = obfuscation_over_rectangle();
    // The obfuscation region as the user sees it before the drag.
    let before = region_pixels(&app, (20, 20), (180, 100));

    // Grab the rectangle outside the obfuscation (bottom-right, filled so a click inside selects it)
    // and drag it up-left so more of it passes under the obfuscation.
    app.drag_hold_px((200.0, 125.0), (180.0, 105.0)); // -20, -20 -> (80, 40)-(200, 110)
    assert!(app.doc().in_gesture(), "the gesture is still open");
    assert_eq!(
        app.selection_bounds(),
        Some([80, 40, 200, 110]),
        "the rectangle moved under the obfuscation"
    );

    // The obfuscation never recomputes mid-gesture: it is byte-identical to before, so it can never
    // reveal, even for one frame, the un-obfuscated rectangle that moved beneath it.
    let during = region_pixels(&app, (20, 20), (180, 100));
    assert!(
        before == during,
        "the obfuscation region changed mid-gesture (it must stay frozen)"
    );
}

#[test]
fn the_obfuscation_is_corrected_by_a_full_render_when_the_gesture_ends() {
    // Move the rectangle and release (drag_px releases and settles): the committed composite must be
    // the correct full render, not the frozen one.
    let mut released = obfuscation_over_rectangle();
    released.drag_px((200.0, 125.0), (180.0, 105.0)); // -20, -20 -> (80, 40)-(200, 110)
    assert!(!released.doc().in_gesture(), "the gesture committed");
    let after = released.composite_image();

    // The same Document built static: the rectangle at the moved position, the obfuscation on top.
    let mut still = app_with_image();
    still.press(Key::R);
    still.drag_px((80.0, 40.0), (200.0, 110.0));
    set(&mut still, Property::FillColour(Some(Rgba::WHITE)));
    still.press(Key::O);
    still.drag_px(OBF.0, OBF.1);
    still.press(Key::Escape);
    let full = still.composite_image();

    support::assert_golden_image_against(&after, &full);
}
