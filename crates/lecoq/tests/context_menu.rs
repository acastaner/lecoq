//! The right-click Context menu (ADR 0008, issue #53): a secondary press on the canvas in Select
//! mode with no Annotation selected opens Copy, Paste and the File operations, mirroring the menu
//! bar. It never opens with a drawing tool active, an Annotation selected, mid-drag, or under a
//! modal. Right-click's drawing-tool escape (#47, ADR 0006) is unchanged — see
//! `editor_right_click.rs`.

mod support;

use std::sync::Arc;

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use lecoq::tools::ToolId;
use lecoq_platform::{ClipboardItem, MemoryClipboard, Platform};
use support::TestApp;

const SMALL: &str = "small-320x240.jpg";
const PNG: &str = "image/png";

/// Right-click at an absolute screen position (points), then settle.
fn right_click_screen(app: &mut TestApp, pos: egui::Pos2) {
    {
        let h = &app.harness;
        h.hover_at(pos);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.remove_cursor();
    }
    app.settle();
}

/// Click a Context-menu entry: the node with `label` below the menu bar (the quick actions and the
/// closed menus carry the same labels up top). Mirrors `keyboard_accessibility`'s `choose`.
fn click_context(app: &mut TestApp, label: &str) {
    let bar_bottom = app.control("File").rect().bottom();
    let wanted = label.to_owned();
    app.harness
        .get_all_by(move |node| {
            node.role() != Role::Label && node.label().as_deref() == Some(wanted.as_str())
        })
        .find(|node| node.rect().top() >= bar_bottom)
        .unwrap_or_else(|| panic!("no {label:?} in the Context menu"))
        .click();
    app.settle();
}

fn menu_is_open(app: &TestApp) -> bool {
    // "Copy to…" lives only in the File menu (closed here) and the Context menu.
    app.harness.query_by_label("Copy to…").is_some()
}

// ---- Opening (stories 40, 44) ----

#[test]
fn right_click_in_select_with_no_selection_opens_the_context_menu() {
    let mut app = TestApp::open_fixture(SMALL);
    assert_eq!(app.harness.state().tool(), ToolId::Select);

    app.secondary_press_px((160.0, 120.0));

    // The entries unique to a menu (the menus are closed) prove the Context menu is up.
    app.harness.get_by_label("Paste");
    app.harness.get_by_label("Copy to…");
    app.harness.get_by_label("Move to…");
}

#[test]
fn right_click_on_the_grey_margin_opens_the_context_menu_too() {
    let mut app = TestApp::open_fixture(SMALL);
    let canvas = app.harness.state().canvas().expect("an image");
    // A point in the canvas area but outside the Image: the Image is fit, so there is a margin on
    // one axis. Pick the left margin if there is one, else the top.
    let pos = if canvas.image.min.x - canvas.area.min.x > 4.0 {
        egui::pos2(canvas.area.min.x + 2.0, canvas.area.center().y)
    } else {
        egui::pos2(canvas.area.center().x, canvas.area.min.y + 2.0)
    };

    right_click_screen(&mut app, pos);

    assert!(menu_is_open(&app), "the grey margin opens the menu");
}

// ---- Not opening (stories 47–50) ----

#[test]
fn right_click_with_a_drawing_tool_active_does_not_open_the_menu() {
    let mut app = TestApp::open_fixture(SMALL);
    app.press(Key::R);

    app.secondary_press_px((160.0, 120.0));

    assert_eq!(
        app.harness.state().tool(),
        ToolId::Select,
        "it returned to Select (#47)"
    );
    assert!(!menu_is_open(&app), "no menu opened");
}

#[test]
fn right_click_with_an_annotation_selected_does_not_open_the_menu() {
    let mut app = TestApp::open_fixture(SMALL);
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
    app.press(Key::Escape);
    app.click_px((45.0, 60.0));
    assert_eq!(app.doc().selection().len(), 1);

    app.secondary_press_px((45.0, 60.0));

    assert!(
        !menu_is_open(&app),
        "with a selection, right-click does nothing (story 48)"
    );
    assert_eq!(app.doc().selection().len(), 1, "the selection is kept");
}

#[test]
fn right_click_mid_rubber_band_does_not_open_the_menu() {
    let mut app = TestApp::open_fixture(SMALL);
    // Begin a rubber-band selection on empty canvas and leave the drag open.
    app.drag_hold_px((20.0, 20.0), (120.0, 100.0));

    let pos = app.screen(120.0, 100.0);
    {
        let h = &app.harness;
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    for _ in 0..3 {
        app.harness.step();
    }
    assert!(!menu_is_open(&app), "no menu mid-drag (story 49)");

    // End the gesture.
    app.press(Key::Escape);
}

#[test]
fn right_click_under_a_modal_does_not_open_the_menu() {
    let mut app = TestApp::open_fixture(SMALL);
    app.press(Key::C); // crop tool, so Image › Resize… is available
    app.harness
        .state_mut()
        .execute(lecoq::Command::ShowResizeDialog);
    app.settle();
    assert!(app.harness.state().is_resize_dialog_open());

    app.secondary_press_px((160.0, 120.0));

    assert!(!menu_is_open(&app), "no menu under a modal (story 50)");
}

// ---- Enabling (stories 41, 45, 46) ----

#[test]
fn in_the_empty_window_only_paste_is_enabled() {
    let mut app = TestApp::new();
    app.settle();

    let pos = {
        let canvas = app
            .harness
            .state()
            .canvas_area()
            .expect("the empty canvas has an area");
        canvas.center()
    };
    right_click_screen(&mut app, pos);

    // Every entry is shown (not hidden), with only Paste enabled (stories 45, 46).
    assert!(menu_is_open(&app), "the empty window opens the menu");
    assert!(app.control("Copy to…").accesskit_node().is_disabled());
    assert!(app.control("Move to…").accesskit_node().is_disabled());
    assert!(!app.control("Paste").accesskit_node().is_disabled());
}

// ---- The entries act (stories 42, 43) ----

#[test]
fn context_menu_copy_puts_the_flattened_image_on_the_clipboard() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = TestApp::with_platform(Platform::in_memory().with_clipboard(clipboard.clone()));
    let path = app.add_fixtures(&[SMALL]);
    app.open(path);

    app.secondary_press_px((160.0, 120.0));
    click_context(&mut app, "Copy");
    app.wait_until("the copy", |_| clipboard.writes() == 1);

    assert!(
        clipboard.get(PNG).is_some(),
        "the Flattened Image is on the clipboard (story 42)"
    );
}

#[test]
fn context_menu_paste_inserts_the_clipboard_image() {
    let clipboard = Arc::new(MemoryClipboard::default());
    clipboard.put(vec![ClipboardItem::new(PNG, tiny_png())]);
    let mut app = TestApp::with_platform(Platform::in_memory().with_clipboard(clipboard));

    let pos = {
        let canvas = app.harness.state().canvas_area().expect("empty canvas");
        canvas.center()
    };
    right_click_screen(&mut app, pos);
    click_context(&mut app, "Paste");

    // Paste into the empty window makes the clipboard image the Image (story 43, §3.5).
    assert_eq!(app.status("File name"), "Clipboard image •");
}

fn tiny_png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}
