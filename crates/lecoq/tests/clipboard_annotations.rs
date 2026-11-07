//! Copy and paste Annotations (#37): §6.2, §6.3.

mod support;

use std::sync::Arc;

use egui::{Key, Modifiers};
use egui_kittest::kittest::NodeT;
use lecoq::tools::ToolId;
use lecoq_core::AnnotationShape;
use lecoq_platform::{ClipboardItem, MemoryClipboard, Platform};
use support::TestApp;

const PNG: &str = "image/png";
const ANNOTATIONS: &str = "application/x-lecoq-annotations+json";

/// A headless App with `fixture` open on an in-memory clipboard.
fn app_with(fixture: &str, clipboard: &Arc<MemoryClipboard>) -> TestApp {
    let platform = Platform::in_memory().with_clipboard(clipboard.clone());
    let mut app = TestApp::with_platform(platform);
    let path = app.add_fixtures(&[fixture]);
    app.open(path);
    app
}

fn ctrl(app: &mut TestApp, key: Key) {
    app.key_with(Modifiers::COMMAND, key);
}

fn decode_png(data: &[u8]) -> image::RgbaImage {
    image::load_from_memory_with_format(data, image::ImageFormat::Png)
        .expect("a PNG")
        .into_rgba8()
}

/// Draw a red rectangle (it stays selected).
fn rectangle(app: &mut TestApp, from: (f32, f32), to: (f32, f32)) {
    app.press(Key::R);
    app.drag_px(from, to);
}

#[test]
fn ctrl_c_with_annotations_selected_publishes_json_and_png_together() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    rectangle(&mut app, (10.0, 10.0), (60.0, 40.0));
    assert_eq!(app.doc().selection().len(), 1);

    ctrl(&mut app, Key::C);

    let mimes: Vec<String> = clipboard.items().into_iter().map(|i| i.mime).collect();
    assert!(mimes.contains(&ANNOTATIONS.to_owned()), "{mimes:?}");
    assert!(mimes.contains(&PNG.to_owned()), "{mimes:?}");
    assert_eq!(clipboard.writes(), 1, "both formats in one write");
    assert_eq!(app.status("Message"), "Annotations copied to the clipboard");
    // Copying is not an edit.
    assert_eq!(app.doc().undo_len(), 1);
}

#[test]
fn the_json_carries_the_annotations_with_image_relative_positions_and_styles() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    rectangle(&mut app, (20.0, 20.0), (60.0, 50.0));

    ctrl(&mut app, Key::C);

    let json: serde_json::Value =
        serde_json::from_slice(&clipboard.get(ANNOTATIONS).expect("the JSON")).unwrap();
    assert_eq!(json["lecoq_annotations"], 1);
    let annotations = json["annotations"].as_array().unwrap();
    assert_eq!(annotations.len(), 1);
    let shape = &annotations[0]["shape"];
    assert_eq!(shape["type"], "rectangle");
    assert_eq!(shape["rect"]["min"]["x"], 20.0);
    assert_eq!(shape["rect"]["max"]["y"], 50.0);
    let style = &annotations[0]["style"];
    assert_eq!(style["line"], "#ff0000");
    assert_eq!(style["thickness"], 3.0);
    assert_eq!(style["shadow"], true);
}

#[test]
fn the_png_holds_just_the_selected_annotations_on_transparency() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    // An unselected rectangle far away, then the selected one (40×30, 3 px red line, shadow).
    rectangle(&mut app, (200.0, 150.0), (300.0, 220.0));
    rectangle(&mut app, (20.0, 20.0), (60.0, 50.0));
    assert_eq!(app.doc().selection().len(), 1);

    ctrl(&mut app, Key::C);

    let png = decode_png(&clipboard.get(PNG).expect("an image/png"));
    let (w, h) = png.dimensions();
    // The rectangle and its line (and shadow), not the Image nor the other rectangle.
    assert!((43..=60).contains(&w) && (33..=50).contains(&h), "{w}×{h}");
    // Transparent inside the rectangle (no fill) and in the corner opposite the shadow.
    assert_eq!(png.get_pixel(w / 2, h / 2)[3], 0, "inside");
    assert_eq!(png.get_pixel(w - 1, 0)[3], 0, "top-right corner");
    // The red line along the top edge (below the shadow's few px of blur).
    let top: Vec<_> = (0..10).map(|y| *png.get_pixel(w / 2, y)).collect();
    assert!(
        top.iter().any(|p| p[3] > 200 && p[0] > 200 && p[1] < 60),
        "{top:?}"
    );
}

#[test]
fn a_copied_highlight_is_in_the_json_but_not_in_the_png() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    rectangle(&mut app, (20.0, 20.0), (60.0, 50.0));
    app.press(Key::H);
    app.drag_px((120.0, 100.0), (200.0, 160.0));
    // Select both (rubber band).
    app.press(Key::Escape);
    app.drag_px((5.0, 5.0), (250.0, 200.0));
    assert_eq!(app.doc().selection().len(), 2);

    ctrl(&mut app, Key::C);

    let json: serde_json::Value =
        serde_json::from_slice(&clipboard.get(ANNOTATIONS).expect("the JSON")).unwrap();
    let types: Vec<_> = json["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["shape"]["type"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(types, ["rectangle", "highlight"]);
    // The PNG is just the rectangle: the highlight (80×60, far from it) would widen it.
    let png = decode_png(&clipboard.get(PNG).expect("an image/png"));
    let (w, h) = png.dimensions();
    assert!((43..=60).contains(&w) && (33..=50).contains(&h), "{w}×{h}");
}

#[test]
fn a_pasted_magnify_highlight_keeps_its_magnification() {
    // Two windows on one clipboard; the second keeps the default magnify_factor (2).
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut first = app_with("small-320x240.jpg", &clipboard);
    let mut second = app_with("small-320x240.jpg", &clipboard);
    first.press(Key::H);
    first.drag_px((120.0, 100.0), (200.0, 160.0));
    first.control("Magnify highlight").click();
    first.settle();
    first.control("Magnification").set_value_accesskit(4.0);
    first.settle();

    ctrl(&mut first, Key::C);
    let json: serde_json::Value =
        serde_json::from_slice(&clipboard.get(ANNOTATIONS).expect("the JSON")).unwrap();
    assert_eq!(json["annotations"][0]["style"]["magnification"], 4.0);

    ctrl(&mut second, Key::V);
    assert_eq!(second.doc().selection().len(), 1);
    let shown = second
        .control("Magnification")
        .accesskit_node()
        .numeric_value();
    assert_eq!(shown, Some(4.0), "the property bar shows the pasted factor");
}

#[test]
fn a_pasted_magnification_out_of_range_is_clamped() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    let json = r#"{"lecoq_annotations": 1, "annotations": [
        {"shape": {"type": "highlight", "rect": {"min": {"x": 10.0, "y": 10.0},
         "max": {"x": 60.0, "y": 40.0}}},
         "style": {"highlight_mode": "magnify", "magnification": 1000.0}}]}"#;
    clipboard.put(vec![ClipboardItem::new(
        ANNOTATIONS,
        json.as_bytes().to_vec(),
    )]);

    ctrl(&mut app, Key::V);

    // The property bar's control clamps what it shows; the Document must hold the clamped value.
    assert_eq!(app.doc().annotations()[0].style.magnification, 10.0);
    assert_eq!(
        app.doc().undo_len(),
        1,
        "the paste alone: nothing restyled it afterwards"
    );
}

#[test]
fn a_highlight_alone_publishes_only_the_json() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    app.press(Key::H);
    app.drag_px((120.0, 100.0), (200.0, 160.0));
    assert_eq!(app.doc().selection().len(), 1);

    ctrl(&mut app, Key::C);

    assert!(clipboard.get(ANNOTATIONS).is_some());
    assert!(clipboard.get(PNG).is_none(), "no PNG of nothing");
}

#[test]
fn a_copied_obfuscation_is_in_the_json_but_not_in_the_png() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    rectangle(&mut app, (20.0, 20.0), (60.0, 50.0));
    app.press(Key::O);
    app.drag_px((120.0, 100.0), (200.0, 160.0));
    app.press(Key::Escape);
    app.drag_px((5.0, 5.0), (250.0, 200.0));
    assert_eq!(app.doc().selection().len(), 2);

    ctrl(&mut app, Key::C);

    let json: serde_json::Value =
        serde_json::from_slice(&clipboard.get(ANNOTATIONS).expect("the JSON")).unwrap();
    let types: Vec<_> = json["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["shape"]["type"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(types, ["rectangle", "obfuscate"]);
    // The PNG is just the rectangle: the obfuscation (80×60, far from it) would widen it.
    let png = decode_png(&clipboard.get(PNG).expect("an image/png"));
    let (w, h) = png.dimensions();
    assert!((43..=60).contains(&w) && (33..=50).contains(&h), "{w}×{h}");

    // Pasted back, it keeps its mode and strength.
    app.press(Key::Escape);
    ctrl(&mut app, Key::V);
    let pasted = &app.doc().annotations()[3];
    assert!(matches!(pasted.shape, AnnotationShape::Obfuscate(_)));
    assert_eq!(pasted.style, app.doc().annotations()[1].style);
}

#[test]
fn ctrl_v_on_the_same_image_adds_and_selects_copies_offset_by_10_px() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    rectangle(&mut app, (20.0, 20.0), (60.0, 50.0));
    ctrl(&mut app, Key::C);

    ctrl(&mut app, Key::V);

    assert_eq!(app.doc().annotations().len(), 2);
    assert_eq!(app.selection_bounds(), Some([30, 30, 70, 60]));
    // A lecoq paste is Annotations, not an Inset of the PNG beside them.
    assert!(
        app.doc()
            .annotations()
            .iter()
            .all(|a| matches!(a.shape, AnnotationShape::Rectangle(_)))
    );
    assert_eq!(app.doc().undo_len(), 2, "one step");
    assert_eq!(app.harness.state().tool(), ToolId::Select);

    // Pasting again lands on the first copy, so it moves on again.
    ctrl(&mut app, Key::V);
    assert_eq!(app.selection_bounds(), Some([40, 40, 80, 70]));

    ctrl(&mut app, Key::Z);
    ctrl(&mut app, Key::Z);
    assert_eq!(app.doc().annotations().len(), 1);
}

#[test]
fn annotations_paste_onto_a_sibling_where_they_were_without_offset() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let platform = Platform::in_memory().with_clipboard(clipboard.clone());
    let mut app =
        TestApp::open_in_folder_on(platform, "small-320x240.jpg", &["a.jpg", "b.jpg"], "a.jpg");
    rectangle(&mut app, (20.0, 20.0), (60.0, 50.0));
    ctrl(&mut app, Key::C);
    app.press(Key::ArrowRight);
    app.control("Discard").click();
    app.settle();
    assert_eq!(app.status("File name"), "b.jpg");
    assert!(app.doc().annotations().is_empty());

    ctrl(&mut app, Key::V);

    assert_eq!(app.doc().annotations().len(), 1);
    assert_eq!(app.selection_bounds(), Some([20, 20, 60, 50]));
    assert_eq!(app.doc().selection().len(), 1);
}

#[test]
fn annotations_paste_into_another_lecoq_window() {
    // Two windows: two Apps on the same (system) clipboard.
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut first = app_with("small-320x240.jpg", &clipboard);
    let mut second = app_with("large-2400x1600.jpg", &clipboard);
    rectangle(&mut first, (20.0, 20.0), (60.0, 50.0));
    first.press(Key::I);
    first.click_px((100.0, 100.0));
    // Select both (rubber band).
    first.press(Key::Escape);
    first.drag_px((5.0, 5.0), (150.0, 150.0));
    assert_eq!(first.doc().selection().len(), 2);
    ctrl(&mut first, Key::C);

    ctrl(&mut second, Key::V);

    let doc = second.doc();
    assert_eq!(doc.annotations().len(), 2);
    assert_eq!(doc.selection().len(), 2);
    assert!(matches!(
        doc.annotations()[0].shape,
        AnnotationShape::Rectangle(_)
    ));
    let AnnotationShape::StepCounter(counter) = &doc.annotations()[1].shape else {
        panic!("a Step counter on top");
    };
    assert_eq!((counter.centre.x, counter.centre.y), (100.0, 100.0));
    // Styles travel too: the Step counter's fill and size are its tool's defaults.
    assert_eq!(
        doc.annotations()[1].style,
        first.doc().annotations()[1].style
    );
}

#[test]
fn pasted_step_counters_take_the_next_numbers() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    app.press(Key::I);
    app.click_px((50.0, 50.0));
    app.click_px((100.0, 50.0));
    // A Step counter numbered 1 elsewhere, as another window would publish it.
    let json = r##"{"lecoq_annotations": 1, "annotations": [
        {"shape": {"type": "step_counter", "centre": {"x": 200.0, "y": 150.0}}, "style": {}},
        {"shape": {"type": "text", "origin": {"x": 20.0, "y": 180.0}, "text": "Hello", "width": null},
         "style": {"font_size": 30.0, "line": "#0000ff"}}
    ]}"##;
    clipboard.put(vec![ClipboardItem::new(
        ANNOTATIONS,
        json.as_bytes().to_vec(),
    )]);

    ctrl(&mut app, Key::V);

    let doc = app.doc();
    let pasted: Vec<_> = doc.selected().collect();
    assert_eq!(pasted.len(), 2);
    assert_eq!(doc.step_number(pasted[0].id), Some(3));
    let AnnotationShape::Text(text) = &pasted[1].shape else {
        panic!("the text");
    };
    assert_eq!(text.text(), "Hello");
    assert_eq!(pasted[1].style.font_size, 30.0);
    // Measured on arrival: the box has a size.
    assert!(
        pasted[1].bounds().width() > 20.0,
        "{:?}",
        pasted[1].bounds()
    );
}

#[test]
fn an_inset_round_trips_through_the_json_with_its_pixels() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    let png = {
        let image = image::RgbaImage::from_pixel(8, 6, image::Rgba([0, 0, 255, 128]));
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    clipboard.put(vec![ClipboardItem::new(PNG, png)]);
    ctrl(&mut app, Key::V);
    ctrl(&mut app, Key::C);

    ctrl(&mut app, Key::V);

    let insets: Vec<_> = app
        .doc()
        .annotations()
        .iter()
        .filter_map(|a| match &a.shape {
            AnnotationShape::Inset(inset) => Some(inset.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(insets.len(), 2);
    let (a, b) = (&insets[0], &insets[1]);
    assert_eq!((b.pixels.width, b.pixels.height), (8, 6));
    let bytes = |p: &lecoq_core::Pixels| match &p.data {
        lecoq_core::PixelData::Rgb(d)
        | lecoq_core::PixelData::Rgba(d)
        | lecoq_core::PixelData::RgbaPremultiplied(d) => d.clone(),
    };
    assert_eq!(bytes(&a.pixels), bytes(&b.pixels));
    assert_eq!(b.rect.min.x - a.rect.min.x, 10.0, "offset");
}

#[test]
fn the_empty_window_takes_an_image_not_annotations() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let platform = Platform::in_memory().with_clipboard(clipboard.clone());
    let mut app = TestApp::with_platform(platform);
    let json = r#"{"lecoq_annotations": 1, "annotations": [
        {"shape": {"type": "step_counter", "centre": {"x": 20.0, "y": 20.0}}, "style": {}}]}"#;
    clipboard.put(vec![ClipboardItem::new(
        ANNOTATIONS,
        json.as_bytes().to_vec(),
    )]);

    ctrl(&mut app, Key::V);

    assert_eq!(app.status("Message"), "No image on the clipboard");
}

#[test]
fn unreadable_annotation_json_is_nothing_to_paste() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = app_with("small-320x240.jpg", &clipboard);
    clipboard.put(vec![ClipboardItem::new(ANNOTATIONS, b"{not json".to_vec())]);

    ctrl(&mut app, Key::V);

    assert!(app.doc().annotations().is_empty());
    assert_eq!(app.status("Message"), "Nothing to paste");
}
