//! Copy the Image and paste an Inset (#36): §3.5, §4.6, §6.2, §6.3, §8.1.

mod support;

use std::sync::Arc;

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use lecoq::tools::ToolId;
use lecoq_core::{AnnotationShape, Shape};
use lecoq_platform::{ClipboardItem, MemoryClipboard, MemoryLauncher, Platform};
use support::TestApp;

const PNG: &str = "image/png";

/// A headless App on an in-memory clipboard (and launcher).
fn app() -> (TestApp, Arc<MemoryClipboard>) {
    let clipboard = Arc::new(MemoryClipboard::default());
    let platform = Platform::in_memory().with_clipboard(clipboard.clone());
    (TestApp::with_platform(platform), clipboard)
}

/// The App with `fixture` open.
fn app_with(fixture: &str) -> (TestApp, Arc<MemoryClipboard>) {
    let (mut app, clipboard) = app();
    let path = app.add_fixtures(&[fixture]);
    app.open(path);
    (app, clipboard)
}

/// A `width`×`height` PNG of one colour.
fn png(width: u32, height: u32, colour: [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba(colour));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn decode_png(data: &[u8]) -> image::RgbaImage {
    image::load_from_memory_with_format(data, image::ImageFormat::Png)
        .expect("a PNG")
        .into_rgba8()
}

fn ctrl(app: &mut TestApp, key: Key) {
    app.key_with(Modifiers::COMMAND, key);
}

/// The Insets in the Document, bottom to top, as (bounds rounded, pixel width, pixel height).
fn insets(app: &TestApp) -> Vec<([i32; 4], u32, u32)> {
    app.doc()
        .annotations()
        .iter()
        .filter_map(|a| match &a.shape {
            AnnotationShape::Inset(inset) => {
                let r = inset.bounds();
                Some((
                    [
                        r.min.x.round() as i32,
                        r.min.y.round() as i32,
                        r.max.x.round() as i32,
                        r.max.y.round() as i32,
                    ],
                    inset.pixels.width,
                    inset.pixels.height,
                ))
            }
            _ => None,
        })
        .collect()
}

// ---- Copy (§6.2) ----

#[test]
fn ctrl_c_with_nothing_selected_copies_the_flattened_image_as_png() {
    let (mut app, clipboard) = app_with("small-320x240.jpg");
    // A red rectangle, drawn then deselected.
    app.press(Key::R);
    app.drag_px((10.0, 10.0), (60.0, 40.0));
    app.press(Key::Escape);
    app.click_px((200.0, 200.0));
    assert!(app.doc().selection().is_empty());

    ctrl(&mut app, Key::C);

    let copied = decode_png(&clipboard.get(PNG).expect("an image/png on the clipboard"));
    assert_eq!(copied.dimensions(), (320, 240), "full resolution");
    // The rectangle's line is in the copy: it is the Flattened Image (§7.2).
    let line = copied.get_pixel(35, 10);
    assert!(line[0] > 200 && line[1] < 60 && line[2] < 60, "{line:?}");
    assert_eq!(app.status("Message"), "Image copied to the clipboard");
}

#[test]
fn ctrl_c_copies_the_image_matching_the_saved_flatten() {
    let (mut app, clipboard) = app_with("small-320x240.jpg");
    ctrl(&mut app, Key::C);
    let copied = decode_png(&clipboard.get(PNG).expect("an image/png on the clipboard"));
    let shown = app.pixel(100, 100).unwrap();
    let px = copied.get_pixel(100, 100);
    for (a, b) in [(px[0], shown.r()), (px[1], shown.g()), (px[2], shown.b())] {
        assert!(a.abs_diff(b) <= 10, "{px:?} vs {shown:?}");
    }
    // Copying is not an edit.
    assert!(!app.harness.state().is_modified());
}

// ---- Paste an Inset (§4.6, §6.3) ----

#[test]
fn ctrl_v_with_an_image_inserts_an_inset_centred_at_100_percent_and_selects_it() {
    let (mut app, clipboard) = app_with("small-320x240.jpg");
    clipboard.put(vec![ClipboardItem::new(PNG, png(40, 20, [0, 0, 255, 255]))]);
    // Even with a drawing tool active (§6.3).
    app.press(Key::R);

    ctrl(&mut app, Key::V);

    assert_eq!(insets(&app), vec![([140, 110, 180, 130], 40, 20)]);
    assert_eq!(app.harness.state().tool(), ToolId::Select);
    assert_eq!(app.selection_bounds(), Some([140, 110, 180, 130]));
    // One paste is one step (§4.8).
    assert_eq!(app.doc().undo_len(), 1);
    assert!(app.harness.state().is_modified());
    // It is drawn: the composite shows blue in the middle.
    let composite = app.composite_image();
    let canvas = app.harness.state().canvas().unwrap();
    let scale = composite.width() as f32 / 320.0;
    let _ = canvas;
    let px = composite.get_pixel((160.0 * scale) as u32, (120.0 * scale) as u32);
    assert!(px[2] > 200 && px[0] < 60, "{px:?}");

    ctrl(&mut app, Key::Z);
    assert!(insets(&app).is_empty());
}

#[test]
fn an_inset_larger_than_the_image_is_scaled_down_to_fit_inside_it() {
    let (mut app, clipboard) = app_with("small-320x240.jpg");
    clipboard.put(vec![ClipboardItem::new(
        PNG,
        png(640, 120, [0, 255, 0, 255]),
    )]);
    ctrl(&mut app, Key::V);
    // 640×120 halves to 320×60, centred.
    assert_eq!(insets(&app), vec![([0, 90, 320, 150], 640, 120)]);
}

#[test]
fn anything_else_on_the_clipboard_is_nothing_to_paste() {
    let (mut app, clipboard) = app_with("small-320x240.jpg");
    clipboard.put(vec![ClipboardItem::new("text/plain", b"hello".to_vec())]);
    ctrl(&mut app, Key::V);
    assert!(insets(&app).is_empty());
    assert_eq!(app.doc().undo_len(), 0);
    assert_eq!(app.status("Message"), "Nothing to paste");
}

#[test]
fn ctrl_drop_of_an_image_file_inserts_it_as_an_inset() {
    let (mut app, _clipboard) = app_with("small-320x240.jpg");
    let path = app.add_fixtures(&["png-64x48.png"]);
    app.harness
        .event(egui::Event::ModifiersChanged(Modifiers::COMMAND));
    app.drop_file(&path);
    app.settle();
    app.harness
        .event(egui::Event::ModifiersChanged(Modifiers::NONE));
    app.settle();

    assert_eq!(insets(&app), vec![([128, 96, 192, 144], 64, 48)]);
    // The Image is still the one open, now edited.
    assert_eq!(app.status("File name"), "small-320x240.jpg •");
    assert_eq!(app.harness.state().tool(), ToolId::Select);
}

#[test]
fn corner_handles_keep_an_insets_aspect_ratio_unless_shift() {
    let (mut app, clipboard) = app_with("small-320x240.jpg");
    clipboard.put(vec![ClipboardItem::new(PNG, png(40, 20, [0, 0, 255, 255]))]);
    ctrl(&mut app, Key::V);
    assert_eq!(app.selection_bounds(), Some([140, 110, 180, 130]));

    // Drag the bottom-right corner 40 px right and 10 down: the ratio stays 2:1.
    app.drag_px((180.0, 130.0), (220.0, 140.0));
    let [x0, y0, x1, y1] = app.selection_bounds().unwrap();
    assert_eq!((x0, y0), (140, 110));
    assert_eq!(x1 - x0, 2 * (y1 - y0), "{:?}", app.selection_bounds());

    // With Shift the corner goes where the pointer is.
    let corner = (x1 as f32, y1 as f32);
    app.drag_px_with(Modifiers::SHIFT, corner, (200.0, 190.0));
    assert_eq!(app.selection_bounds(), Some([140, 110, 200, 190]));
}

#[test]
fn an_inset_offers_shadow_and_border() {
    let (mut app, clipboard) = app_with("small-320x240.jpg");
    clipboard.put(vec![ClipboardItem::new(PNG, png(40, 20, [0, 0, 255, 255]))]);
    ctrl(&mut app, Key::V);
    // Shadow on and no border by default.
    let style = app.doc().annotations()[0].style.clone();
    assert!(style.shadow);
    assert_eq!(style.border, None);
    // The property bar offers the border and the shadow, and nothing for lines or fills.
    app.control("Border colour");
    app.control("No border");
    app.control("Shadow");
    assert!(app.harness.query_by_label("Fill colour").is_none());
    assert!(app.harness.query_by_label("Line colour").is_none());

    // Turning the border on restyles the Inset.
    app.control("No border").click();
    app.settle();
    assert!(app.doc().annotations()[0].style.border.is_some());
}

#[test]
fn an_inset_rotates_with_the_image() {
    let (mut app, clipboard) = app_with("small-320x240.jpg");
    clipboard.put(vec![ClipboardItem::new(PNG, png(40, 20, [0, 0, 255, 255]))]);
    ctrl(&mut app, Key::V);
    app.harness
        .state_mut()
        .execute(lecoq::Command::Rotate { clockwise: true });
    app.settle();
    // The 240×320 Image: the box turned, and its pixels with it.
    assert_eq!(insets(&app), vec![([110, 140, 130, 180], 20, 40)]);
}

// ---- The empty window (§3.5) ----

#[test]
fn ctrl_v_in_the_empty_window_makes_the_clipboard_image_the_image() {
    let dialogs = Arc::new(lecoq_platform::MemoryFileDialogs::default());
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut platform = Platform::in_memory().with_clipboard(clipboard.clone());
    platform.dialogs = dialogs.clone();
    let mut app = TestApp::with_platform(platform);
    clipboard.put(vec![ClipboardItem::new(PNG, png(64, 32, [0, 128, 0, 255]))]);

    ctrl(&mut app, Key::V);

    assert_eq!(app.status("File name"), "Clipboard image •");
    assert_eq!(app.status("Pixel size"), "64×32");
    // No Siblings until it is saved.
    assert_eq!(app.status_field("Sibling position"), None);
    assert!(app.doc().annotations().is_empty());

    // Ctrl+S acts as Save As.
    let target = app.dir.path().join("pasted.png");
    dialogs.answer_next(Some(target.clone()));
    ctrl(&mut app, Key::S);
    assert_eq!(dialogs.opened(), 1);
    let saved = image::open(&target).unwrap().into_rgba8();
    assert_eq!(saved.dimensions(), (64, 32));
    assert_eq!(saved.get_pixel(10, 10).0, [0, 128, 0, 255]);
    app.wait_for_status("File name", "pasted.png");
    assert_eq!(app.status("Sibling position"), "1 / 1");
}

#[test]
fn the_clipboard_image_can_be_edited_and_saved() {
    let dialogs = Arc::new(lecoq_platform::MemoryFileDialogs::default());
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut platform = Platform::in_memory().with_clipboard(clipboard.clone());
    platform.dialogs = dialogs.clone();
    let mut app = TestApp::with_platform(platform);
    clipboard.put(vec![ClipboardItem::new(
        PNG,
        png(100, 80, [255, 255, 255, 255]),
    )]);
    ctrl(&mut app, Key::V);

    app.press(Key::R);
    app.drag_px((10.0, 10.0), (50.0, 40.0));
    let target = app.dir.path().join("edited.png");
    dialogs.answer_next(Some(target.clone()));
    ctrl(&mut app, Key::S);
    let saved = image::open(&target).unwrap().into_rgba8();
    let line = saved.get_pixel(30, 10);
    assert!(line[0] > 200 && line[1] < 60, "{line:?}");
}

#[test]
fn ctrl_v_in_the_empty_window_without_an_image_says_so() {
    let (mut app, clipboard) = app();
    clipboard.put(vec![ClipboardItem::new("text/plain", b"hello".to_vec())]);
    ctrl(&mut app, Key::V);
    assert_eq!(app.status("Message"), "No image on the clipboard");
    assert!(app.harness.state().document().is_none());
}

// ---- New window from the clipboard (§6.1) ----

#[test]
fn ctrl_shift_v_opens_the_clipboard_image_in_a_new_window() {
    let launcher = Arc::new(MemoryLauncher::default());
    let platform = Platform::in_memory().with_launcher(launcher.clone());
    let (mut app, _) = (TestApp::with_platform(platform), ());
    app.key_with(Modifiers::COMMAND | Modifiers::SHIFT, Key::V);
    assert_eq!(launcher.windows_from_clipboard(), 1);
    // This window is left alone.
    assert!(app.harness.state().document().is_none());
}

#[test]
fn file_new_from_clipboard_opens_a_new_window() {
    let launcher = Arc::new(MemoryLauncher::default());
    let platform = Platform::in_memory().with_launcher(launcher.clone());
    let mut app = TestApp::with_platform(platform);
    app.control("File").click();
    app.harness.run();
    app.control("New from clipboard").click();
    app.settle();
    assert_eq!(launcher.windows_from_clipboard(), 1);
}

#[test]
fn a_window_started_from_the_clipboard_shows_the_clipboard_image() {
    let clipboard = Arc::new(MemoryClipboard::default());
    clipboard.put(vec![ClipboardItem::new(PNG, png(30, 20, [9, 9, 9, 255]))]);
    let mut app = TestApp::with_launch(|launch| {
        launch.platform = Platform::in_memory().with_clipboard(clipboard.clone());
        launch.from_clipboard = true;
    });
    app.settle();
    assert_eq!(app.status("File name"), "Clipboard image •");
    assert_eq!(app.status("Pixel size"), "30×20");
}

#[test]
fn leaving_an_unsaved_clipboard_image_asks_first() {
    let (mut app, clipboard) = app();
    clipboard.put(vec![ClipboardItem::new(PNG, png(64, 32, [0, 128, 0, 255]))]);
    ctrl(&mut app, Key::V);
    let other = app.add_fixtures(&["small-320x240.jpg"]);
    app.drop_file(&other);
    app.settle();
    // Its pixels exist only in this window (§3.5): "Save changes?" (§6.4).
    assert!(app.harness.query_by_label("Save changes?").is_some());
    assert_eq!(app.status("File name"), "Clipboard image •");
}
