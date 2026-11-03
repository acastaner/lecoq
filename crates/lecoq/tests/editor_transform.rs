//! Rotate, flip and resize (#33): the Image menu and its quick action, the Resize dialog (§6.4),
//! the Image operations and their effect on Annotations (§4.7, ADR 0001), one History step each.

mod support;

use std::sync::Arc;

use egui::accesskit::{Role, Toggled};
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use lecoq::Command;
use lecoq::tools::ToolId;
use lecoq_core::{AnnotationShape, PixelData, Pixels, Point};
use support::TestApp;

/// A 320×240 Image, shown at 100%.
fn app_with_image() -> TestApp {
    TestApp::open_fixture("small-320x240.jpg")
}

/// The full-resolution pixels, waiting for the decode if needed.
fn full_resolution(app: &mut TestApp) -> Arc<Pixels> {
    if app.doc().full_resolution().is_none() {
        // The select tool doesn't start the decode; the rectangle tool does.
        app.press(Key::R);
        app.press(Key::Escape);
    }
    app.doc()
        .full_resolution()
        .expect("full-resolution pixels")
        .clone()
}

/// The RGB of pixel (x, y).
fn rgb_at(pixels: &Pixels, x: u32, y: u32) -> [u8; 3] {
    let i = (y * pixels.width + x) as usize;
    match &pixels.data {
        PixelData::Rgb(d) => [d[i * 3], d[i * 3 + 1], d[i * 3 + 2]],
        PixelData::Rgba(d) => [d[i * 4], d[i * 4 + 1], d[i * 4 + 2]],
    }
}

/// Every pixel (x, y) of `after` is pixel `from(x, y)` of `before`.
fn assert_pixels_moved(after: &Pixels, before: &Pixels, from: impl Fn(u32, u32) -> (u32, u32)) {
    for y in 0..after.height {
        for x in 0..after.width {
            let (fx, fy) = from(x, y);
            assert_eq!(
                rgb_at(after, x, y),
                rgb_at(before, fx, fy),
                "pixel ({x}, {y}) should be ({fx}, {fy})"
            );
        }
    }
}

/// Choose `item` in the Image menu.
fn image_menu(app: &mut TestApp, item: &str) {
    app.control("Image").click();
    app.harness.run();
    app.control(item).click();
    app.settle();
}

fn size(app: &TestApp) -> (u32, u32) {
    (app.doc().width(), app.doc().height())
}

// ---- Rotate and flip: the pixels ----

#[test]
fn rotate_right_turns_the_image_a_quarter_clockwise_as_one_step() {
    let mut app = app_with_image();
    let original = full_resolution(&mut app);

    image_menu(&mut app, "Rotate right");

    assert_eq!(size(&app), (240, 320));
    // The old bottom-left corner is the new top-left one.
    assert_pixels_moved(&full_resolution(&mut app), &original, |x, y| (y, 239 - x));
    assert_eq!(app.status("Pixel size"), "240×320");
    assert_eq!(app.status("File name"), "small-320x240.jpg •");
    assert_eq!(app.doc().undo_len(), 1);
    let composite = app.harness.state().composite().expect("a composite");
    assert_eq!((composite.width, composite.height), (240, 320));

    app.key_with(Modifiers::COMMAND, Key::Z);

    assert_eq!(size(&app), (320, 240));
    assert_pixels_moved(&full_resolution(&mut app), &original, |x, y| (x, y));
    assert!(!app.doc().is_modified());
}

#[test]
fn rotate_left_turns_the_image_a_quarter_anticlockwise() {
    let mut app = app_with_image();
    let original = full_resolution(&mut app);

    image_menu(&mut app, "Rotate left");

    assert_eq!(size(&app), (240, 320));
    // The old top-right corner is the new top-left one.
    assert_pixels_moved(&full_resolution(&mut app), &original, |x, y| (319 - y, x));
}

#[test]
fn flips_mirror_the_image_horizontally_or_vertically() {
    let mut app = app_with_image();
    let original = full_resolution(&mut app);

    image_menu(&mut app, "Flip horizontal");
    assert_eq!(size(&app), (320, 240));
    assert_pixels_moved(&full_resolution(&mut app), &original, |x, y| (319 - x, y));

    image_menu(&mut app, "Flip vertical");
    assert_pixels_moved(&full_resolution(&mut app), &original, |x, y| {
        (319 - x, 239 - y)
    });
    assert_eq!(app.doc().undo_len(), 2);
}

#[test]
fn an_operation_chosen_before_the_full_resolution_decode_waits_for_it() {
    let mut app = app_with_image();
    assert!(app.doc().full_resolution().is_none());

    // Both chosen in the same frame, before the decode can land.
    app.harness
        .state_mut()
        .execute(Command::Rotate { clockwise: true });
    app.harness
        .state_mut()
        .execute(Command::Flip { horizontal: false });
    assert_eq!(size(&app), (320, 240));
    app.settle();

    assert_eq!(size(&app), (240, 320));
    assert_eq!(app.doc().undo_len(), 2);
}

#[test]
fn the_quick_action_offers_the_image_menu() {
    let mut app = app_with_image();

    app.control("Rotate, flip or resize").click();
    app.harness.run();
    app.control("Rotate right").click();
    app.settle();

    assert_eq!(size(&app), (240, 320));
}

// ---- Annotations follow (ADR 0001) ----

fn rounded(r: lecoq_core::Rect) -> [i32; 4] {
    [
        r.min.x.round() as i32,
        r.min.y.round() as i32,
        r.max.x.round() as i32,
        r.max.y.round() as i32,
    ]
}

fn rounded_point(p: Point) -> (i32, i32) {
    (p.x.round() as i32, p.y.round() as i32)
}

/// A rectangle (40, 30)–(140, 90), an ellipse (200, 150)–(260, 200), a line (60, 50)→(140, 90),
/// an arrow (80, 160)→(200, 120) and a freehand stroke, all 3 px thick.
fn app_with_annotations() -> TestApp {
    let mut app = app_with_image();
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    app.press(Key::E);
    app.drag_px((200.0, 150.0), (260.0, 200.0));
    app.press(Key::L);
    app.drag_px((60.0, 50.0), (140.0, 90.0));
    app.press(Key::A);
    app.drag_px((80.0, 160.0), (200.0, 120.0));
    app.press(Key::F);
    app.drag_path_px(&[(100.0, 60.0), (120.0, 75.0), (150.0, 110.0)]);
    app.press(Key::Escape);
    app
}

/// Each Annotation's geometry: rect corners for boxes, end points for lines and arrows, every
/// point for freehand strokes.
fn geometry(app: &TestApp) -> Vec<Vec<(i32, i32)>> {
    app.doc()
        .annotations()
        .iter()
        .map(|a| match &a.shape {
            AnnotationShape::Rectangle(r) => {
                let [x0, y0, x1, y1] = rounded(r.rect);
                vec![(x0, y0), (x1, y1)]
            }
            AnnotationShape::Highlight(h) => {
                let [x0, y0, x1, y1] = rounded(h.rect);
                vec![(x0, y0), (x1, y1)]
            }
            AnnotationShape::Obfuscate(o) => {
                let [x0, y0, x1, y1] = rounded(o.rect);
                vec![(x0, y0), (x1, y1)]
            }
            AnnotationShape::Ellipse(e) => {
                let [x0, y0, x1, y1] = rounded(e.rect);
                vec![(x0, y0), (x1, y1)]
            }
            AnnotationShape::Line(l) => vec![rounded_point(l.from), rounded_point(l.to)],
            AnnotationShape::Arrow(l) => vec![rounded_point(l.from), rounded_point(l.to)],
            AnnotationShape::Freehand(f) => f.points.iter().copied().map(rounded_point).collect(),
            AnnotationShape::Inset(i) => {
                let [x0, y0, x1, y1] = rounded(i.rect);
                vec![(x0, y0), (x1, y1)]
            }
            AnnotationShape::Text(t) => vec![rounded_point(t.origin)],
            AnnotationShape::StepCounter(s) => vec![rounded_point(s.centre)],
        })
        .collect()
}

fn thicknesses(app: &TestApp) -> Vec<f32> {
    app.doc()
        .annotations()
        .iter()
        .map(|a| a.style.thickness)
        .collect()
}

#[test]
fn every_shape_turns_with_the_image_and_keeps_its_thickness() {
    let mut app = app_with_annotations();
    let stroke = |app: &TestApp| match &app.doc().annotations()[4].shape {
        AnnotationShape::Freehand(f) => f.points.clone(),
        other => panic!("not a freehand stroke: {other:?}"),
    };
    let stroke_before = stroke(&app);

    image_menu(&mut app, "Rotate right");

    // Clockwise in a 320×240 Image: (x, y) → (240 − y, x).
    let after = geometry(&app);
    assert_eq!(after[0], vec![(150, 40), (210, 140)]);
    assert_eq!(after[1], vec![(40, 200), (90, 260)]);
    assert_eq!(after[2], vec![(190, 60), (150, 140)]);
    assert_eq!(after[3], vec![(80, 80), (120, 200)]);
    let stroke_after = stroke(&app);
    assert_eq!(stroke_after.len(), stroke_before.len());
    for (b, a) in stroke_before.iter().zip(&stroke_after) {
        assert!(
            (a.x - (240.0 - b.y)).abs() < 1e-3 && (a.y - b.x).abs() < 1e-3,
            "{b:?} -> {a:?}"
        );
    }
    assert_eq!(thicknesses(&app), vec![3.0; 5]);
    app.assert_golden("rotate-right-annotations");
}

#[test]
fn every_shape_follows_rotate_left_and_both_flips() {
    let mut app = app_with_annotations();

    image_menu(&mut app, "Rotate left");
    // Anticlockwise in a 320×240 Image: (x, y) → (y, 320 − x).
    assert_eq!(geometry(&app)[0], vec![(30, 180), (90, 280)]);
    assert_eq!(geometry(&app)[3], vec![(160, 240), (120, 120)]);
    app.key_with(Modifiers::COMMAND, Key::Z);

    image_menu(&mut app, "Flip horizontal");
    // (x, y) → (320 − x, y).
    assert_eq!(geometry(&app)[0], vec![(180, 30), (280, 90)]);
    assert_eq!(geometry(&app)[2], vec![(260, 50), (180, 90)]);
    app.key_with(Modifiers::COMMAND, Key::Z);

    image_menu(&mut app, "Flip vertical");
    // (x, y) → (x, 240 − y).
    assert_eq!(geometry(&app)[1], vec![(200, 40), (260, 90)]);
    assert_eq!(geometry(&app)[3], vec![(80, 80), (200, 120)]);
}

#[test]
fn undo_puts_the_annotations_back_where_they_were() {
    let mut app = app_with_annotations();
    let before = app.doc().annotations().to_vec();
    let steps = app.doc().undo_len();

    image_menu(&mut app, "Rotate right");
    assert_eq!(app.doc().undo_len(), steps + 1);
    app.key_with(Modifiers::COMMAND, Key::Z);

    assert_eq!(app.doc().annotations(), &before[..]);
}

// ---- The Resize dialog (§6.4) ----

fn open_resize_dialog(app: &mut TestApp) {
    image_menu(app, "Resize…");
    assert!(app.harness.state().is_resize_dialog_open());
}

fn field(app: &TestApp, label: &str) -> String {
    app.control(label).value().unwrap_or_default()
}

fn set_field(app: &mut TestApp, label: &str, text: &str) {
    app.control(label).set_text_value_accesskit(text);
    app.harness.run();
}

/// The text of a read-out (a label with a fixed accessible name).
fn readout(app: &TestApp, label: &str) -> String {
    let wanted = label.to_owned();
    app.harness
        .get_by(move |node| {
            node.role() == Role::Label && node.label().as_deref() == Some(wanted.as_str())
        })
        .value()
        .unwrap_or_default()
}

#[test]
fn the_resize_dialog_starts_at_the_image_size_in_pixels_with_the_aspect_ratio_locked() {
    let mut app = app_with_image();
    open_resize_dialog(&mut app);

    assert_eq!(field(&app, "Width"), "320");
    assert_eq!(field(&app, "Height"), "240");
    assert_eq!(
        app.control("Lock aspect ratio").accesskit_node().toggled(),
        Some(Toggled::True)
    );
    assert_eq!(readout(&app, "New size"), "320×240");
    app.control("Cancel");
    app.control("Resize");
}

#[test]
fn resize_in_pixels_scales_the_image_and_every_annotation_with_its_thickness_as_one_step() {
    let mut app = app_with_annotations();
    let steps = app.doc().undo_len();
    let before = app.doc().annotations().to_vec();
    open_resize_dialog(&mut app);

    set_field(&mut app, "Width", "160");
    // Locked: the height follows.
    assert_eq!(field(&app, "Height"), "120");
    assert_eq!(readout(&app, "New size"), "160×120");
    app.control("Resize").click();
    app.settle();

    assert!(!app.harness.state().is_resize_dialog_open());
    assert_eq!(size(&app), (160, 120));
    assert_eq!(app.status("Pixel size"), "160×120");
    let px = full_resolution(&mut app);
    assert_eq!((px.width, px.height), (160, 120));
    let after = geometry(&app);
    assert_eq!(after[0], vec![(20, 15), (70, 45)]);
    assert_eq!(after[2], vec![(30, 25), (70, 45)]);
    assert_eq!(thicknesses(&app), vec![1.5; 5]);
    assert_eq!(app.doc().undo_len(), steps + 1);

    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!(size(&app), (320, 240));
    assert_eq!(app.doc().annotations(), &before[..]);
}

#[test]
fn resize_in_percent_and_enter_resizes() {
    let mut app = app_with_image();
    open_resize_dialog(&mut app);

    app.control("Percent").click();
    app.harness.run();
    assert_eq!(field(&app, "Width"), "100");
    assert_eq!(field(&app, "Height"), "100");
    set_field(&mut app, "Height", "50");
    assert_eq!(field(&app, "Width"), "50");
    assert_eq!(readout(&app, "New size"), "160×120");

    app.press(Key::Enter);

    assert_eq!(size(&app), (160, 120));
    assert!(!app.harness.state().is_resize_dialog_open());
}

#[test]
fn unlocking_the_aspect_ratio_sets_each_side_on_its_own() {
    let mut app = app_with_image();
    open_resize_dialog(&mut app);

    app.control("Lock aspect ratio").click();
    app.harness.run();
    set_field(&mut app, "Width", "100");
    assert_eq!(field(&app, "Height"), "240");
    app.control("Resize").click();
    app.settle();

    assert_eq!(size(&app), (100, 240));
}

#[test]
fn esc_cancels_the_resize_dialog_and_the_dialog_keeps_the_keyboard_to_itself() {
    let mut app = app_with_image();
    app.press(Key::R);
    open_resize_dialog(&mut app);
    set_field(&mut app, "Width", "100");

    // A tool key goes nowhere while the dialog is up.
    app.press(Key::E);
    assert_eq!(app.harness.state().tool(), ToolId::Rectangle);
    app.press(Key::Escape);

    assert!(!app.harness.state().is_resize_dialog_open());
    assert_eq!(size(&app), (320, 240));
    assert!(!app.doc().can_undo());
    // Esc closed the dialog only: it didn't also switch to the select tool.
    assert_eq!(app.harness.state().tool(), ToolId::Rectangle);
}

#[test]
fn cancel_closes_the_resize_dialog_without_resizing() {
    let mut app = app_with_image();
    open_resize_dialog(&mut app);
    set_field(&mut app, "Width", "100");

    app.control("Cancel").click();
    app.settle();

    assert!(!app.harness.state().is_resize_dialog_open());
    assert_eq!(size(&app), (320, 240));
    assert!(!app.doc().can_undo());
}

#[test]
fn resize_is_offered_only_for_a_valid_size() {
    let mut app = app_with_image();
    open_resize_dialog(&mut app);

    set_field(&mut app, "Width", "0");
    assert!(app.control("Resize").accesskit_node().is_disabled());
    set_field(&mut app, "Width", "abc");
    assert!(app.control("Resize").accesskit_node().is_disabled());
    set_field(&mut app, "Width", "64");
    assert!(!app.control("Resize").accesskit_node().is_disabled());
}

// ---- Resampling (§4.7) ----

/// Open a generated grey-level Image: `value(x, y)` for each pixel.
fn app_with_generated(width: u32, height: u32, value: impl Fn(u32, u32) -> u8) -> TestApp {
    let mut app = TestApp::new();
    let path = app.dir.path().join("generated.png");
    image::RgbImage::from_fn(width, height, |x, y| {
        let v = value(x, y);
        image::Rgb([v, v, v])
    })
    .save(&path)
    .expect("write the generated Image");
    app.open(path);
    app
}

#[test]
fn downscaling_averages_everything_first_mipmap_then_cubic() {
    // Columns: 2 white, 6 black, repeated. A cubic alone, 8× down, would only see black.
    let mut app = app_with_generated(64, 8, |x, _| if x % 8 < 2 { 255 } else { 0 });

    app.harness.state_mut().execute(Command::Resize {
        width: 8,
        height: 1,
    });
    app.settle();

    let px = full_resolution(&mut app);
    assert_eq!((px.width, px.height), (8, 1));
    for x in 0..8 {
        let [v, ..] = rgb_at(&px, x, 0);
        assert!(
            (60..=68).contains(&v),
            "pixel {x} is {v}, not a quarter white"
        );
    }
}

#[test]
fn upscaling_is_mitchell_cubic() {
    // Black, black, white, white; 3× up puts new pixel 4 on the centre of old pixel 1 and new
    // pixel 7 on old pixel 2. Mitchell (B = C = 1/3) weighs a pixel's neighbours 1/18 each there,
    // so the edge bleeds 255/18 ≈ 14 into the black pixel and leaves 255·17/18 ≈ 241 in the white
    // one (nearest, linear and Catmull-Rom would all give 0 and 255).
    let mut app = app_with_generated(4, 2, |x, _| if x < 2 { 0 } else { 255 });

    app.harness.state_mut().execute(Command::Resize {
        width: 12,
        height: 6,
    });
    app.settle();

    let px = full_resolution(&mut app);
    let [black, ..] = rgb_at(&px, 4, 3);
    let [white, ..] = rgb_at(&px, 7, 3);
    assert!(
        (10..=18).contains(&black),
        "edge-side black pixel is {black}"
    );
    assert!(
        (237..=245).contains(&white),
        "edge-side white pixel is {white}"
    );
}
