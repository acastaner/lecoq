//! Obfuscate (#31, §4.1 O, §4.2, §4.3): pixelate or blur, computed live from everything beneath.

mod support;

use std::sync::Arc;

use egui::Key;
use lecoq::Command;
use lecoq::tools::ToolId;
use lecoq_core::{AnnotationShape, ObfuscateMode, Property, Rgba};
use lecoq_platform::{MemoryConfigStore, MemoryFileDialogs, Platform};
use support::TestApp;

/// A 320×240 Image (dark-to-light gradient, a red box top left, a blue disc bottom right), shown
/// at 100%: Image pixels are screen points and composite pixels.
fn app_with_image() -> TestApp {
    TestApp::open_fixture("small-320x240.jpg")
}

/// The obfuscated region: over the red box's lower right corner and the gradient around it.
const REGION: ((f32, f32), (f32, f32)) = ((20.0, 20.0), (180.0, 100.0));

/// A white-filled, red-lined rectangle from (100, 60) to (220, 130), partly under [`REGION`], then
/// an obfuscation over [`REGION`] in `mode` (default strength 10) on top of it.
fn app_with_obfuscation_over_rectangle(mode: ObfuscateMode) -> TestApp {
    let mut app = app_with_image();
    app.press(Key::R);
    app.drag_px((100.0, 60.0), (220.0, 130.0));
    set(&mut app, Property::FillColour(Some(Rgba::WHITE)));
    app.press(Key::O);
    app.drag_px(REGION.0, REGION.1);
    set(&mut app, Property::ObfuscateMode(mode));
    app
}

/// Restyle the selection (or the tool's default) as the property bar does.
fn set(app: &mut TestApp, property: Property) {
    app.harness.state_mut().execute(Command::SetProperty {
        property,
        merge: false,
    });
    app.settle();
}

/// A composite pixel (premultiplied RGBA; the canvas is opaque).
fn composite_px(app: &TestApp, x: u32, y: u32) -> [u8; 4] {
    let c = app.harness.state().composite().expect("a composite");
    let i = (y * c.width + x) as usize * 4;
    c.rgba_premultiplied[i..i + 4].try_into().unwrap()
}

/// Every pixel of the `size`-px square whose top-left corner is (x, y).
fn block(app: &TestApp, (x, y): (u32, u32), size: u32) -> Vec<[u8; 4]> {
    (y..y + size)
        .flat_map(|py| (x..x + size).map(move |px| (px, py)))
        .map(|(px, py)| composite_px(app, px, py))
        .collect()
}

fn is_one_colour(pixels: &[[u8; 4]]) -> bool {
    pixels.iter().all(|p| p == &pixels[0])
}

// ---- Drawing ----

#[test]
fn o_then_a_drag_creates_a_selected_pixelate_obfuscation() {
    let mut app = app_with_image();
    app.press(Key::O);
    app.drag_px(REGION.0, REGION.1);

    assert_eq!(app.harness.state().tool(), ToolId::Obfuscate);
    let doc = app.doc();
    assert_eq!(doc.annotations().len(), 1);
    let a = &doc.annotations()[0];
    assert!(matches!(a.shape, AnnotationShape::Obfuscate(_)));
    assert_eq!(a.style.obfuscate_mode, ObfuscateMode::Pixelate);
    assert_eq!(a.style.strength, 10.0);
    assert_eq!(app.selection_bounds(), Some([20, 20, 180, 100]));
    assert_eq!(doc.undo_len(), 1);
}

#[test]
fn an_obfuscation_is_selected_by_clicking_anywhere_inside_it() {
    let mut app = app_with_image();
    app.press(Key::O);
    app.drag_px(REGION.0, REGION.1);
    app.press(Key::Escape);
    app.click_px((250.0, 200.0));
    assert!(app.doc().selection().is_empty());

    app.click_px((60.0, 50.0));
    assert_eq!(app.doc().selection().len(), 1);
}

// ---- Property bar ----

#[test]
fn the_mode_buttons_switch_the_mode_one_step_each() {
    let mut app = app_with_image();
    app.press(Key::O);
    app.drag_px(REGION.0, REGION.1);
    for (label, mode) in [
        ("Blur", ObfuscateMode::Blur),
        ("Pixelate", ObfuscateMode::Pixelate),
    ] {
        app.control(label).click();
        app.settle();
        assert_eq!(app.doc().annotations()[0].style.obfuscate_mode, mode);
    }
    assert_eq!(app.doc().undo_len(), 3);
}

#[test]
fn the_strength_restyles_the_selection_and_persists_as_the_tools_default() {
    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(Platform::in_memory().with_config(store.clone()));
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    app.press(Key::O);
    app.drag_px(REGION.0, REGION.1);
    app.control("Blur").click();
    app.settle();

    app.control("Strength").set_value_accesskit(24.0);
    app.settle();

    assert_eq!(app.doc().annotations()[0].style.strength, 24.0);
    let text = store.text().unwrap();
    assert!(text.contains("[tools.obfuscate]"), "{text}");
    assert!(text.contains("obfuscate_mode = \"blur\""), "{text}");
    assert!(text.contains("strength = 24.0"), "{text}");
    // And the next obfuscation uses them.
    app.drag_px((200.0, 150.0), (300.0, 220.0));
    let next = &app.doc().annotations()[1].style;
    assert_eq!(
        (next.obfuscate_mode, next.strength),
        (ObfuscateMode::Blur, 24.0)
    );
}

#[test]
fn a_saved_strength_out_of_range_is_brought_back_into_it() {
    let store = Arc::new(MemoryConfigStore::with_text(
        "version = 1\n[tools.obfuscate]\nstrength = 1000.0\n",
    ));
    let mut app = TestApp::with_platform(Platform::in_memory().with_config(store));
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    app.press(Key::O);
    app.drag_px(REGION.0, REGION.1);
    assert_eq!(app.doc().annotations()[0].style.strength, 100.0);
}

// ---- Effects: golden images, each mode over the Image and a rectangle ----

#[test]
fn pixelate_shows_blocks_of_the_strength_size_averaging_whats_below() {
    let app = app_with_obfuscation_over_rectangle(ObfuscateMode::Pixelate);

    // Blocks are 10 px, on a grid from the region's corner (20, 20). Each is one colour.
    for corner in [(20, 20), (90, 50), (100, 70), (170, 90)] {
        assert!(
            is_one_colour(&block(&app, corner, 10)),
            "block at {corner:?} is one colour"
        );
    }
    // Wholly inside the white fill: white.
    assert_eq!(composite_px(&app, 145, 85), [255, 255, 255, 255]);
    // (100, 70)–(110, 80) holds 1.5 px of the 3 px red line and 8.5 px of white fill: pink.
    let [r, g, b, _] = composite_px(&app, 105, 75);
    assert!(
        r > 250 && (180..=240).contains(&g) && (180..=240).contains(&b),
        "pink {r} {g} {b}"
    );
    // Outside the region nothing changes: the line keeps its place and colour.
    let [r, g, b, _] = composite_px(&app, 100, 120);
    assert!(r > 200 && g < 60 && b < 60, "line outside");
    assert_eq!(composite_px(&app, 104, 120), [255, 255, 255, 255]);
    app.assert_golden("obfuscate-pixelate");
}

#[test]
fn a_larger_strength_makes_larger_blocks() {
    let mut app = app_with_obfuscation_over_rectangle(ObfuscateMode::Pixelate);
    set(&mut app, Property::Strength(40.0));

    // (20, 20)–(60, 60) is one block now.
    assert!(is_one_colour(&block(&app, (20, 20), 40)));
    assert_ne!(composite_px(&app, 59, 59), composite_px(&app, 60, 60));
}

#[test]
fn blur_mixes_neighbouring_pixels_over_the_strength_radius() {
    let app = app_with_obfuscation_over_rectangle(ObfuscateMode::Blur);

    // The red line is smeared into the white fill beside it, and lightened by it.
    let [r, g, b, _] = composite_px(&app, 100, 85);
    assert!(
        r > 200 && (60..=220).contains(&g) && g.abs_diff(b) <= 5,
        "line blurred {r} {g} {b}"
    );
    // 6 px into the fill: reddened (and a little darkened by the gradient left of the line).
    let [r, g, _, _] = composite_px(&app, 106, 85);
    assert!(r > 240 && g < 230, "red reaches 6 px in: {r} {g}");
    // Far (more than 3 sigma) from any edge, the white fill stays white.
    assert!(composite_px(&app, 150, 92).iter().all(|&c| c >= 250));
    // Outside the region nothing changes.
    let [r, g, b, _] = composite_px(&app, 100, 120);
    assert!(r > 200 && g < 60 && b < 60, "line outside");
    assert_eq!(composite_px(&app, 106, 120), [255, 255, 255, 255]);
    app.assert_golden("obfuscate-blur");
}

#[test]
fn the_blur_strength_is_its_radius() {
    let mut app = app_with_obfuscation_over_rectangle(ObfuscateMode::Blur);
    let at_10 = composite_px(&app, 110, 85);
    set(&mut app, Property::Strength(2.0));
    let at_2 = composite_px(&app, 110, 85);

    // 10 px from the line: reached at radius 10, not at radius 2.
    assert!(at_10[1] < 250, "{at_10:?}");
    assert!(at_2[1] >= 253, "{at_2:?}");
}

#[test]
fn the_saved_file_matches_the_screen_in_both_modes() {
    for mode in ObfuscateMode::ALL {
        let dialogs = Arc::new(MemoryFileDialogs::default());
        let mut platform = Platform::in_memory();
        platform.dialogs = dialogs.clone();
        let mut app = TestApp::with_platform(platform);
        let path = app.add_fixtures(&["small-320x240.jpg"]);
        app.open(path.clone());
        app.press(Key::R);
        app.drag_px((100.0, 60.0), (220.0, 130.0));
        app.press(Key::O);
        app.drag_px(REGION.0, REGION.1);
        set(&mut app, Property::ObfuscateMode(mode));
        let target = path.with_file_name("flattened.png");
        dialogs.answer_next(Some(target.clone()));
        app.key_with(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::S);

        let saved = image::open(&target).unwrap().into_rgba8();
        support::assert_golden_image_against(&saved, &app.composite_image());
    }
}

// ---- Live ----

#[test]
fn the_effect_recomputes_when_something_beneath_changes() {
    let mut app = app_with_obfuscation_over_rectangle(ObfuscateMode::Pixelate);

    // Select the rectangle beneath (outside the obfuscation) and restyle it.
    app.press(Key::Escape);
    app.click_px((200.0, 110.0));
    set(&mut app, Property::LineColour(Rgba::rgb(0, 0, 255)));

    let [r, g, b, _] = composite_px(&app, 105, 75);
    assert!(b > 250 && r < 245 && g < 245, "now light blue {r} {g} {b}");
}

#[test]
fn the_blocks_move_with_the_obfuscation() {
    let mut app = app_with_obfuscation_over_rectangle(ObfuscateMode::Pixelate);

    // Drag it 5 px right: the grid follows, so (25, 20)–(35, 30) is one block now.
    app.press(Key::Escape);
    app.drag_px((60.0, 50.0), (65.0, 50.0));
    assert_eq!(app.selection_bounds(), Some([25, 20, 185, 100]));
    assert!(is_one_colour(&block(&app, (25, 20), 10)));
}

#[test]
fn annotations_above_an_obfuscation_are_not_affected() {
    let mut app = app_with_image();
    app.press(Key::O);
    app.drag_px(REGION.0, REGION.1);
    app.press(Key::R);
    app.drag_px((40.0, 40.0), (150.0, 90.0));
    set(&mut app, Property::FillColour(Some(Rgba::WHITE)));

    assert_eq!(composite_px(&app, 100, 70), [255, 255, 255, 255]);
}

// ---- Image operations ----

#[test]
fn a_resize_scales_the_region_and_the_strength() {
    let mut app = app_with_image();
    app.press(Key::O);
    app.drag_px(REGION.0, REGION.1);
    app.harness.state_mut().execute(Command::Resize {
        width: 160,
        height: 120,
    });
    app.settle();

    let a = &app.doc().annotations()[0];
    assert_eq!(a.style.strength, 5.0);
    let r = a.bounds();
    assert_eq!(
        [r.min.x, r.min.y, r.max.x, r.max.y].map(|v| v.round() as i32),
        [10, 10, 90, 50]
    );
}
