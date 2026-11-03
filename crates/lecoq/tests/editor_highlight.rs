//! Highlight (#30, §4.1 H, §4.2, §4.3): four modes computed live from everything beneath.

mod support;

use std::sync::Arc;

use egui::Key;
use egui_kittest::kittest::Queryable;
use lecoq::Command;
use lecoq::tools::ToolId;
use lecoq_core::{AnnotationShape, HighlightMode, Property, Rgba};
use lecoq_platform::{MemoryConfigStore, Platform};
use support::TestApp;

/// A 320×240 Image (dark-to-light gradient, a red box top left, a blue disc bottom right), shown
/// at 100%: Image pixels are screen points and composite pixels.
fn app_with_image() -> TestApp {
    TestApp::open_fixture("small-320x240.jpg")
}

/// The highlight region: over the red box's lower right corner and the gradient around it.
const REGION: ((f32, f32), (f32, f32)) = ((20.0, 20.0), (180.0, 100.0));

/// An Image with a highlight over [`REGION`], selected, in `mode`.
fn app_with_highlight(mode: HighlightMode) -> TestApp {
    let mut app = app_with_image();
    app.press(Key::H);
    app.drag_px(REGION.0, REGION.1);
    set(&mut app, Property::HighlightMode(mode));
    app
}

/// A white-filled, red-lined rectangle from (100, 60) to (220, 130), partly under [`REGION`], then a
/// highlight over [`REGION`] in `mode` on top of it.
fn app_with_highlight_over_rectangle(mode: HighlightMode) -> TestApp {
    let mut app = app_with_image();
    app.press(Key::R);
    app.drag_px((100.0, 60.0), (220.0, 130.0));
    set(&mut app, Property::FillColour(Some(Rgba::WHITE)));
    app.press(Key::H);
    app.drag_px(REGION.0, REGION.1);
    set(&mut app, Property::HighlightMode(mode));
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

fn is_gray([r, g, b, _]: [u8; 4]) -> bool {
    r.abs_diff(g) <= 3 && g.abs_diff(b) <= 3
}

// ---- Drawing ----

#[test]
fn h_then_a_drag_creates_a_selected_text_highlight() {
    let mut app = app_with_image();
    app.press(Key::H);
    app.drag_px(REGION.0, REGION.1);

    assert_eq!(app.harness.state().tool(), ToolId::Highlight);
    let doc = app.doc();
    assert_eq!(doc.annotations().len(), 1);
    let a = &doc.annotations()[0];
    assert!(matches!(a.shape, AnnotationShape::Highlight(_)));
    assert_eq!(a.style.highlight_mode, HighlightMode::Text);
    assert_eq!(app.selection_bounds(), Some([20, 20, 180, 100]));
    assert_eq!(doc.undo_len(), 1);
}

#[test]
fn a_highlight_is_selected_by_clicking_anywhere_inside_it() {
    let mut app = app_with_highlight(HighlightMode::Area);
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
    app.press(Key::H);
    app.drag_px(REGION.0, REGION.1);
    for (label, mode) in [
        ("Area highlight", HighlightMode::Area),
        ("Grayscale highlight", HighlightMode::Grayscale),
        ("Magnify highlight", HighlightMode::Magnify),
        ("Text highlight", HighlightMode::Text),
    ] {
        app.control(label).click();
        app.settle();
        assert_eq!(app.doc().annotations()[0].style.highlight_mode, mode);
    }
    assert_eq!(app.doc().undo_len(), 5);
}

#[test]
fn the_colour_shows_in_text_mode_and_the_magnification_in_magnify_mode() {
    let mut app = app_with_highlight(HighlightMode::Text);
    app.control("Highlight colour");
    assert!(app.harness.query_by_label("Magnification").is_none());

    set(&mut app, Property::HighlightMode(HighlightMode::Magnify));
    app.control("Magnification");
    assert!(app.harness.query_by_label("Highlight colour").is_none());

    set(&mut app, Property::HighlightMode(HighlightMode::Area));
    assert!(app.harness.query_by_label("Magnification").is_none());
    assert!(app.harness.query_by_label("Highlight colour").is_none());
}

#[test]
fn with_nothing_selected_the_mode_sets_the_tools_default() {
    let mut app = app_with_image();
    app.press(Key::H);
    app.control("Grayscale highlight").click();
    app.settle();
    app.drag_px(REGION.0, REGION.1);

    assert_eq!(
        app.doc().annotations()[0].style.highlight_mode,
        HighlightMode::Grayscale
    );
}

// ---- Magnification setting (§8.3 magnify_factor) ----

#[test]
fn the_magnification_starts_at_the_magnify_factor_setting() {
    let store = Arc::new(MemoryConfigStore::with_text(
        "version = 1\nmagnify_factor = 3.0\n",
    ));
    let mut app = TestApp::with_platform(Platform::in_memory().with_config(store));
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    app.press(Key::H);
    app.drag_px(REGION.0, REGION.1);

    assert_eq!(app.doc().annotations()[0].style.magnification, 3.0);
}

#[test]
fn changing_the_magnification_persists_it_as_magnify_factor() {
    let store = Arc::new(MemoryConfigStore::default());
    let mut app = TestApp::with_platform(Platform::in_memory().with_config(store.clone()));
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    app.press(Key::H);
    app.drag_px(REGION.0, REGION.1);
    set(&mut app, Property::HighlightMode(HighlightMode::Magnify));
    assert_eq!(app.doc().annotations()[0].style.magnification, 2.0);

    app.control("Magnification").set_value_accesskit(4.0);
    app.settle();

    assert_eq!(app.doc().annotations()[0].style.magnification, 4.0);
    assert_eq!(app.harness.state().config().magnify_factor, 4.0);
    assert!(store.text().unwrap().contains("magnify_factor = 4.0"));
    // And the next highlight uses it.
    app.drag_px((200.0, 150.0), (300.0, 220.0));
    assert_eq!(app.doc().annotations()[1].style.magnification, 4.0);
}

// ---- Effects: golden images, each mode over the Image and a rectangle ----

#[test]
fn text_highlight_multiplies_its_colour_over_the_region() {
    let app = app_with_highlight_over_rectangle(HighlightMode::Text);

    // The white rectangle fill inside the region turns the highlight's yellow; outside it stays.
    assert_eq!(composite_px(&app, 150, 80), [255, 255, 0, 255]);
    assert_eq!(composite_px(&app, 200, 110), [255, 255, 255, 255]);
    app.assert_golden("highlight-text");
}

#[test]
fn area_highlight_dims_everything_outside_the_region() {
    let app = app_with_highlight_over_rectangle(HighlightMode::Area);

    // White fill: undimmed inside, half dimmed outside.
    assert_eq!(composite_px(&app, 150, 80), [255, 255, 255, 255]);
    let [r, g, b, _] = composite_px(&app, 200, 110);
    assert!((120..=135).contains(&r) && r == g && g == b, "dimmed white");
    app.assert_golden("highlight-area");
}

#[test]
fn grayscale_highlight_grays_everything_outside_the_region() {
    let app = app_with_highlight_over_rectangle(HighlightMode::Grayscale);

    // The red box is still red inside the region, the blue disc and the rectangle's red line are
    // gray outside it.
    let [r, g, b, _] = composite_px(&app, 60, 60);
    assert!(r > 150 && g < 80 && b < 80, "red inside");
    assert!(is_gray(composite_px(&app, 220, 160)), "blue disc gray");
    assert!(is_gray(composite_px(&app, 220, 100)), "red line gray");
    app.assert_golden("highlight-grayscale");
}

#[test]
fn magnify_highlight_shows_whats_under_it_enlarged() {
    let app = app_with_highlight_over_rectangle(HighlightMode::Magnify);

    // At 2×, the region (20, 20)–(180, 100) shows (60, 40)–(140, 80): the rectangle's left line at
    // x = 100 runs from y = 60 to the bottom, so on screen from (100, 60) down, twice as thick.
    let [r, g, b, _] = composite_px(&app, 101, 90);
    assert!(r > 200 && g < 60 && b < 60, "magnified line");
    // Its white fill, which starts at x = 101.5, shows from x = 103 on.
    assert_eq!(composite_px(&app, 120, 90), [255, 255, 255, 255]);
    // Outside the region, nothing changes: the line is at its own place and thickness.
    let [r, g, b, _] = composite_px(&app, 100, 120);
    assert!(r > 200 && g < 60 && b < 60, "line outside");
    assert_eq!(composite_px(&app, 104, 120), [255, 255, 255, 255]);
    app.assert_golden("highlight-magnify");
}

// ---- Live ----

#[test]
fn the_effect_recomputes_when_something_beneath_changes() {
    let mut app = app_with_highlight_over_rectangle(HighlightMode::Magnify);
    let before = composite_px(&app, 101, 90);

    // Select the rectangle beneath (outside the highlight) and restyle it.
    app.press(Key::Escape);
    app.click_px((200.0, 110.0));
    set(&mut app, Property::LineColour(Rgba::rgb(0, 0, 255)));

    let [r, g, b, _] = composite_px(&app, 101, 90);
    assert!(
        b > 200 && r < 60 && g < 60,
        "magnified line now blue, was {before:?}"
    );
}

#[test]
fn the_effect_follows_the_highlight_when_it_moves() {
    let mut app = app_with_highlight_over_rectangle(HighlightMode::Area);
    assert_eq!(composite_px(&app, 150, 80), [255, 255, 255, 255]);

    // Drag the selected highlight down by 50: the white fill at (150, 120) is now inside it, at
    // (150, 65) outside.
    app.press(Key::Escape);
    app.drag_px((60.0, 50.0), (60.0, 100.0));

    assert_eq!(app.selection_bounds(), Some([20, 70, 180, 150]));
    assert_eq!(composite_px(&app, 150, 120), [255, 255, 255, 255]);
    let [r, _, _, _] = composite_px(&app, 150, 65);
    assert!(r < 140, "white above the moved region is dimmed now");
}

#[test]
fn a_highlight_follows_a_rotation_and_keeps_its_effect() {
    let mut app = app_with_highlight(HighlightMode::Magnify);
    app.harness
        .state_mut()
        .execute(Command::Rotate { clockwise: true });
    app.settle();

    // 320×240 turned clockwise: (x, y) → (240 - y, x), so (20, 20)–(180, 100) → (140, 20)–(220, 180).
    let a = &app.doc().annotations()[0];
    assert_eq!(a.style.highlight_mode, HighlightMode::Magnify);
    let r = a.bounds();
    assert_eq!(
        [r.min.x, r.min.y, r.max.x, r.max.y].map(|v| v.round() as i32),
        [140, 20, 220, 180]
    );
}

#[test]
fn annotations_above_a_highlight_are_not_affected() {
    let mut app = app_with_highlight(HighlightMode::Area);
    app.press(Key::R);
    app.drag_px((220.0, 140.0), (300.0, 220.0));
    set(&mut app, Property::FillColour(Some(Rgba::WHITE)));

    // Outside the area highlight, but drawn above it: not dimmed.
    assert_eq!(composite_px(&app, 260, 180), [255, 255, 255, 255]);
}
