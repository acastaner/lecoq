//! Keyboard and accessibility (#40, spec §2, §6.1, §8.6): every action is in a menu or a quick
//! action, reachable with Tab, the arrow keys and Enter, every control carries an AccessKit label,
//! and Cmd replaces Ctrl on macOS.

mod support;

use std::sync::Arc;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use lecoq_platform::{MemoryClipboard, MemoryFileDialogs, MemoryLauncher, MemoryTrash, Platform};
use support::TestApp;

/// A headless App whose file dialogs are the returned fake.
fn app_with_dialogs() -> (TestApp, Arc<MemoryFileDialogs>) {
    let dialogs = Arc::new(MemoryFileDialogs::default());
    let mut platform = Platform::in_memory();
    platform.dialogs = dialogs.clone();
    (TestApp::with_platform(platform), dialogs)
}

/// Open a menu of the menu bar, then click one of its entries. (A quick action or a property-bar
/// button may carry the same label: the entry is the one in the open menu, below the menu bar.)
fn choose(app: &mut TestApp, menu: &str, entry: &str) {
    app.control(menu).click();
    app.harness.run();
    let bar_bottom = app.control(menu).rect().bottom();
    let wanted = entry.to_owned();
    app.harness
        .get_all_by(move |node| {
            node.role() != Role::Label && node.label().as_deref() == Some(wanted.as_str())
        })
        .find(|node| node.rect().top() >= bar_bottom)
        .unwrap_or_else(|| panic!("no {entry:?} in the {menu} menu"))
        .click();
    app.settle();
}

// ---- File ----

#[test]
fn file_open_shows_the_open_dialog() {
    let (mut app, dialogs) = app_with_dialogs();
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    dialogs.answer_next(Some(path));
    app.settle();

    choose(&mut app, "File", "Open…");

    assert_eq!(dialogs.opened(), 1);
    assert_eq!(app.status("File name"), "small-320x240.jpg");
}

#[test]
fn file_save_as_shows_the_save_dialog() {
    let (mut app, dialogs) = app_with_dialogs();
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    dialogs.answer_next(None);

    choose(&mut app, "File", "Save As…");

    assert_eq!(dialogs.opened(), 1);
}

#[test]
fn file_new_from_clipboard_opens_a_new_window() {
    let launcher = Arc::new(MemoryLauncher::default());
    let mut app = TestApp::with_platform(Platform::in_memory().with_launcher(launcher.clone()));
    app.settle();

    choose(&mut app, "File", "New from clipboard");

    assert_eq!(launcher.windows_from_clipboard(), 1);
}

// ---- Edit ----

/// An Image with one rectangle drawn on it (and selected).
fn app_with_rectangle() -> TestApp {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    draw_rectangle(&mut app);
    app
}

fn draw_rectangle(app: &mut TestApp) {
    app.press(Key::R);
    app.drag_px((40.0, 30.0), (140.0, 90.0));
}

#[test]
fn edit_undo_and_redo() {
    let mut app = app_with_rectangle();

    choose(&mut app, "Edit", "Undo");
    assert_eq!(app.doc().annotations().len(), 0);

    choose(&mut app, "Edit", "Redo");
    assert_eq!(app.doc().annotations().len(), 1);
}

#[test]
fn edit_duplicate_and_delete_act_on_the_selection() {
    let mut app = app_with_rectangle();

    choose(&mut app, "Edit", "Duplicate");
    assert_eq!(app.doc().annotations().len(), 2);

    choose(&mut app, "Edit", "Delete");
    assert_eq!(app.doc().annotations().len(), 1);
}

#[test]
fn edit_copy_and_paste_go_through_the_clipboard() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = TestApp::with_platform(Platform::in_memory().with_clipboard(clipboard.clone()));
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    draw_rectangle(&mut app);

    choose(&mut app, "Edit", "Copy");
    app.wait_until("the copy", |_| clipboard.writes() == 1);
    choose(&mut app, "Edit", "Paste");

    assert_eq!(app.doc().annotations().len(), 2);
}

#[test]
fn edit_stacking_entries_move_the_selection() {
    let mut app = app_with_rectangle();
    app.drag_px((180.0, 120.0), (260.0, 200.0));
    let first = app.doc().annotations()[0].id;

    // The second rectangle is selected and on top: send it to the back, then one level up again.
    choose(&mut app, "Edit", "Send to back");
    assert_ne!(app.doc().annotations()[0].id, first);

    choose(&mut app, "Edit", "Bring forward");
    assert_eq!(app.doc().annotations()[0].id, first);
}

// ---- View ----

#[test]
fn view_zoom_entries_and_fullscreen() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");

    choose(&mut app, "View", "Zoom in");
    assert_ne!(app.status("Zoom"), "Fit");

    choose(&mut app, "View", "Fit to window");
    assert_eq!(app.status("Zoom"), "Fit");

    choose(&mut app, "View", "Fullscreen");
    assert!(app.harness.state().is_fullscreen());
}

#[test]
fn view_next_and_previous_image() {
    let mut app = TestApp::open_in_folder("small-320x240.jpg", &["a.jpg", "b.jpg"], "a.jpg");

    choose(&mut app, "View", "Next image");
    assert_eq!(app.status("File name"), "b.jpg");

    choose(&mut app, "View", "Previous image");
    assert_eq!(app.status("File name"), "a.jpg");
}

// ---- Help ----

#[test]
fn help_lists_the_keyboard_shortcuts_and_esc_closes_it() {
    let mut app = TestApp::new();
    app.settle();

    choose(&mut app, "Help", "Keyboard shortcuts");
    app.harness.get_by_label("Ctrl+Shift+S");
    app.harness
        .get_by_label("Open the clipboard image in a new window");
    app.harness.get_by_label("Rectangle tool");

    app.press(Key::Escape);
    assert!(app.harness.query_by_label("Ctrl+Shift+S").is_none());
}

#[test]
fn help_about_names_the_version_and_close_closes_it() {
    let mut app = TestApp::new();
    app.settle();

    choose(&mut app, "Help", "About lecoq");
    let version = format!("lecoq {}", env!("CARGO_PKG_VERSION"));
    app.harness.get_by_label_contains(&version);

    app.control("Close").click();
    app.settle();
    assert!(app.harness.query_by_label_contains(&version).is_none());
}

// ---- Quick actions (§2) ----

#[test]
fn the_quick_actions_are_labelled_and_in_the_order_of_the_spec() {
    let app = TestApp::open_fixture("small-320x240.jpg");

    let lefts: Vec<f32> = [
        "Save",
        "Save as",
        "Copy",
        "Undo",
        "Rotate, flip or resize",
        "Move to trash",
    ]
    .iter()
    .map(|label| app.control(label).rect().left())
    .collect();

    assert!(
        lefts.windows(2).all(|w| w[0] < w[1]),
        "quick actions out of order: {lefts:?}"
    );
}

#[test]
fn the_save_as_and_undo_quick_actions_act() {
    let (mut app, dialogs) = app_with_dialogs();
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    draw_rectangle(&mut app);

    app.control("Undo").click();
    app.settle();
    assert_eq!(app.doc().annotations().len(), 0);

    dialogs.answer_next(None);
    app.control("Save as").click();
    app.settle();
    assert_eq!(dialogs.opened(), 1);
}

#[test]
fn the_save_quick_action_saves() {
    let mut app = app_with_rectangle();
    assert_eq!(app.status("File name"), "small-320x240.jpg •");

    app.control("Save").click();
    app.wait_until("the save", |app| {
        app.status("File name") == "small-320x240.jpg"
    });
}

#[test]
fn the_copy_quick_action_copies_the_image() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let mut app = TestApp::with_platform(Platform::in_memory().with_clipboard(clipboard.clone()));
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);

    app.control("Copy").click();
    app.wait_until("the copy", |_| clipboard.writes() == 1);

    assert!(clipboard.get("image/png").is_some());
}

#[test]
fn the_trash_quick_action_trashes_the_file_even_with_a_selection() {
    let trash = Arc::new(MemoryTrash::default());
    let mut app = TestApp::open_in_folder_on(
        Platform::in_memory().with_trash(trash.clone()),
        "small-320x240.jpg",
        &["a.jpg", "b.jpg"],
        "a.jpg",
    );
    draw_rectangle(&mut app);

    app.control("Move to trash").click();
    app.settle();

    assert_eq!(trash.trashed().len(), 1);
    assert_eq!(app.status("File name"), "b.jpg");
}

// ---- Tab, the arrow keys, Enter (§8.6) ----

/// The accessibility label of the control with keyboard focus.
fn focused(app: &TestApp) -> Option<String> {
    app.harness
        .query_by(|node| node.is_focused())
        .map(|node| node.accesskit_node().label().unwrap_or_default())
}

/// Press Tab until `label` has the focus (at most 60 times).
fn tab_to(app: &mut TestApp, label: &str) {
    for _ in 0..60 {
        app.press(Key::Tab);
        if focused(app).as_deref() == Some(label) {
            return;
        }
    }
    panic!("Tab never reached {label:?}");
}

#[test]
fn tab_moves_through_the_menus_the_quick_actions_and_the_tools_in_order() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");

    let mut seen = Vec::new();
    for _ in 0..12 {
        app.press(Key::Tab);
        seen.push(focused(&app).unwrap_or_default());
    }

    // Undo is disabled (nothing to undo), so Tab skips it.
    assert_eq!(
        seen,
        [
            "File",
            "Edit",
            "Image",
            "View",
            "Help",
            "Save",
            "Save as",
            "Copy",
            "Rotate, flip or resize",
            "Move to trash",
            "Select",
            "Rectangle",
        ]
    );
}

#[test]
fn a_menu_entry_is_reached_with_tab_the_arrow_keys_and_enter() {
    let (mut app, dialogs) = app_with_dialogs();
    dialogs.answer_next(None);
    app.settle();

    app.press(Key::Tab);
    assert_eq!(focused(&app).as_deref(), Some("File"));
    app.press(Key::Enter);
    app.press(Key::ArrowDown);
    assert_eq!(focused(&app).as_deref(), Some("Open…"));
    app.press(Key::Enter);

    assert_eq!(dialogs.opened(), 1);
}

#[test]
fn the_arrow_keys_move_along_the_menu_bar() {
    let mut app = TestApp::new();
    app.settle();

    app.press(Key::Tab);
    app.press(Key::ArrowRight);
    app.press(Key::ArrowRight);

    assert_eq!(focused(&app).as_deref(), Some("Image"));
}

#[test]
fn the_property_bar_controls_are_reached_with_tab_and_the_arrow_keys() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::R);

    tab_to(&mut app, "Line colour");
    app.press(Key::ArrowRight);
    assert_eq!(focused(&app).as_deref(), Some("Fill colour"));

    tab_to(&mut app, "Shadow");
    app.press(Key::Space);
    assert!(
        !app.harness
            .state()
            .tool_style(lecoq::tools::ToolId::Rectangle)
            .shadow
    );
}

#[test]
fn the_obfuscate_controls_are_reached_with_tab() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::O);

    tab_to(&mut app, "Blur");
    app.press(Key::Enter);
    tab_to(&mut app, "Strength");

    assert_eq!(
        app.harness
            .state()
            .tool_style(lecoq::tools::ToolId::Obfuscate)
            .obfuscate_mode,
        lecoq_core::ObfuscateMode::Blur
    );
}

#[test]
fn ctrl_shortcuts_still_work_while_a_control_has_the_focus() {
    let mut app = app_with_rectangle();
    app.press(Key::Tab);
    assert_eq!(focused(&app).as_deref(), Some("File"));

    app.key_with(Modifiers::COMMAND, Key::Z);

    assert_eq!(app.doc().annotations().len(), 0);
}

#[test]
fn while_a_control_has_the_focus_the_arrow_keys_move_the_focus_not_the_image() {
    let mut app = TestApp::open_in_folder("small-320x240.jpg", &["a.jpg", "b.jpg"], "a.jpg");
    app.press(Key::Tab);

    app.press(Key::ArrowRight);

    assert_eq!(focused(&app).as_deref(), Some("Edit"));
    assert_eq!(app.status("File name"), "a.jpg");
    // Esc lets go of the focus: the arrows are the Siblings' again.
    app.press(Key::Escape);
    app.press(Key::ArrowRight);
    assert_eq!(app.status("File name"), "b.jpg");
}

// ---- Cmd on macOS (§6.1) ----

/// What winit reports for the Cmd key on macOS.
const CMD: Modifiers = Modifiers::MAC_CMD.plus(Modifiers::COMMAND);

fn on_macos(app: &mut TestApp) {
    app.harness.ctx.set_os(egui::os::OperatingSystem::Mac);
    app.harness.run();
}

#[test]
fn on_macos_cmd_opens_and_ctrl_does_not() {
    let (mut app, dialogs) = app_with_dialogs();
    on_macos(&mut app);

    app.key_with(Modifiers::CTRL, Key::O);
    assert_eq!(dialogs.opened(), 0);

    dialogs.answer_next(None);
    app.key_with(CMD, Key::O);
    assert_eq!(dialogs.opened(), 1);
}

#[test]
fn on_macos_every_editing_shortcut_takes_cmd() {
    let clipboard = Arc::new(MemoryClipboard::default());
    let launcher = Arc::new(MemoryLauncher::default());
    let dialogs = Arc::new(MemoryFileDialogs::default());
    let mut platform = Platform::in_memory()
        .with_clipboard(clipboard.clone())
        .with_launcher(launcher.clone());
    platform.dialogs = dialogs.clone();
    let mut app = TestApp::with_platform(platform);
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    on_macos(&mut app);
    draw_rectangle(&mut app);
    let count = |app: &TestApp| app.doc().annotations().len();

    app.key_with(CMD, Key::D);
    assert_eq!(count(&app), 2, "Cmd+D duplicates");
    app.key_with(CMD, Key::Z);
    assert_eq!(count(&app), 1, "Cmd+Z undoes");
    app.key_with(CMD | Modifiers::SHIFT, Key::Z);
    assert_eq!(count(&app), 2, "Cmd+Shift+Z redoes");
    app.key_with(CMD, Key::Z);
    app.key_with(CMD, Key::Y);
    assert_eq!(count(&app), 2, "Cmd+Y redoes");

    // The duplicate is selected and on top.
    let top = app.doc().annotations()[1].id;
    app.key_with(CMD, Key::OpenBracket);
    assert_eq!(app.doc().annotations()[0].id, top, "Cmd+[ moves it down");
    app.key_with(CMD, Key::CloseBracket);
    assert_eq!(app.doc().annotations()[1].id, top, "Cmd+] moves it up");

    app.key_with(CMD, Key::C);
    app.wait_until("Cmd+C", |_| clipboard.writes() == 1);
    app.key_with(CMD, Key::V);
    assert_eq!(count(&app), 3, "Cmd+V pastes");

    app.key_with(CMD | Modifiers::SHIFT, Key::V);
    assert_eq!(launcher.windows_from_clipboard(), 1, "Cmd+Shift+V");

    dialogs.answer_next(None);
    app.key_with(CMD | Modifiers::SHIFT, Key::S);
    assert_eq!(dialogs.opened(), 1, "Cmd+Shift+S");

    app.key_with(CMD, Key::S);
    app.wait_until("Cmd+S", |app| {
        app.status("File name") == "small-320x240.jpg"
    });
}

#[test]
fn on_macos_the_shortcuts_read_cmd() {
    let mut app = TestApp::new();
    app.settle();
    on_macos(&mut app);

    app.harness
        .get_by_label("Open (Cmd+O), drop or paste (Cmd+V) an image");
    choose(&mut app, "Help", "Keyboard shortcuts");
    // macOS order: Shift before Cmd, as its own menus write it.
    app.harness.get_by_label("Shift+Cmd+S");
    assert!(app.harness.query_by_label_contains("Ctrl").is_none());
}

// ---- Every control has a label (§8.6) ----

/// The controls on screen without an accessibility label: anything a screen reader can press,
/// focus, toggle or edit (the canvas included). Each is described by its role and position, to find it.
fn unlabelled(app: &TestApp) -> Vec<String> {
    use egui::accesskit::Action;
    const CONTROLS: [Role; 10] = [
        Role::Button,
        Role::CheckBox,
        Role::ComboBox,
        Role::TextInput,
        Role::SpinButton,
        Role::Slider,
        Role::ColorWell,
        Role::MenuItem,
        Role::RadioButton,
        Role::Tab,
    ];
    app.harness
        .query_all_by(|node| {
            // Text (a caption, a status-bar field) is read by its value, not a label.
            if matches!(node.role(), Role::Label | Role::TextRun) {
                return false;
            }
            let data = node.data();
            let control = CONTROLS.contains(&node.role())
                || data.supports_action(Action::Click)
                || data.supports_action(Action::Focus);
            control && node.label().is_none_or(|l| l.trim().is_empty())
        })
        .map(|node| {
            let node = node.accesskit_node();
            format!("{:?} at {:?}", node.role(), node.bounding_box())
        })
        .collect()
}

#[track_caller]
fn assert_all_labelled(app: &TestApp, state: &str) {
    let missing = unlabelled(app);
    assert!(
        missing.is_empty(),
        "{state}: unlabelled controls {missing:?}"
    );
}

#[test]
fn every_control_of_the_empty_window_is_labelled() {
    let mut app = TestApp::new();
    app.settle();
    assert_all_labelled(&app, "empty window");

    for menu in ["File", "Edit", "Image", "View", "Help"] {
        app.control(menu).click();
        app.harness.run();
        assert_all_labelled(&app, &format!("{menu} menu"));
        app.press(Key::Escape);
    }
}

#[test]
fn every_tool_and_its_property_bar_controls_are_labelled() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    assert_all_labelled(&app, "image, select tool");

    // The strip lists every tool of §4.1, by name.
    for tool in [
        "Select",
        "Rectangle",
        "Ellipse",
        "Line",
        "Arrow",
        "Freehand",
        "Text",
        "Step counter",
        "Highlight",
        "Obfuscate",
        "Crop",
    ] {
        app.control(tool).click();
        app.settle();
        assert_all_labelled(&app, tool);
    }
    // The mode-dependent controls: the highlight's colour and magnification, the obfuscate modes.
    app.control("Highlight").click();
    app.settle();
    for mode in ["Area", "Grayscale", "Magnify", "Text"] {
        app.control(&format!("{mode} highlight")).click();
        app.settle();
        assert_all_labelled(&app, &format!("{mode} highlight"));
    }
    app.control("Obfuscate").click();
    app.settle();
    app.control("Blur").click();
    app.settle();
    app.control("Strength");
    app.control("Pixelate");
    assert_all_labelled(&app, "obfuscate, blur");
}

#[test]
fn the_selection_crop_and_open_lists_are_labelled() {
    let mut app = app_with_rectangle();
    app.press(Key::Escape);
    app.click_px((90.0, 30.0));
    app.control("Delete");
    app.control("Bring to front");
    app.control("Send to back");
    assert_all_labelled(&app, "a selected rectangle");

    // egui's colour picker has a copy button drawn as a glyph the bundled fonts lack: the app draws
    // a box (egui's default), kittest panics. Allow the box here.
    app.harness
        .ctx
        .set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    app.control("Line colour").click();
    app.harness.run();
    // egui's picker labels its sliders and values, but not its saturation/value square (a mouse
    // affordance; the keyboard sets the same colour through the RGBA values and the hue slider).
    let missing = unlabelled(&app);
    assert!(
        missing.len() <= 1 && missing.iter().all(|m| m.starts_with("Unknown")),
        "the colour picker open: unlabelled controls {missing:?}"
    );
    app.press(Key::Escape);

    app.press(Key::A);
    app.control("Arrowheads").click();
    app.harness.run();
    assert_all_labelled(&app, "the arrowheads list open");
    app.press(Key::Escape);

    app.press(Key::C);
    app.drag_px((20.0, 20.0), (200.0, 150.0));
    assert_eq!(app.status_field("Crop size").as_deref(), Some("180×130"));
    app.control("Apply");
    app.control("Cancel");
    assert_all_labelled(&app, "crop");

    app.control("Rotate, flip or resize").click();
    app.harness.run();
    assert_all_labelled(&app, "the Image quick action open");
}

#[test]
fn every_dialog_is_labelled() {
    let mut app = app_with_rectangle();

    choose(&mut app, "Image", "Resize…");
    assert_all_labelled(&app, "Resize");
    app.press(Key::Escape);

    choose(&mut app, "Help", "Keyboard shortcuts");
    assert_all_labelled(&app, "Keyboard shortcuts");
    app.press(Key::Escape);

    choose(&mut app, "Help", "About lecoq");
    assert_all_labelled(&app, "About");
    app.press(Key::Escape);

    // Moving on with unsaved changes asks first (§6.4).
    let next = app.add_fixture_as("small-320x240.jpg", "other.jpg");
    app.harness.state_mut().execute(lecoq::Command::Open(next));
    app.harness.run();
    app.dialog_control("Save");
    app.dialog_control("Discard");
    app.dialog_control("Cancel");
    assert_all_labelled(&app, "Save changes?");
}

#[test]
fn the_overwrite_dialog_is_labelled() {
    let mut app = TestApp::open_fixture("anim-64x48.webp");
    app.press(Key::R);
    app.drag_px((10.0, 10.0), (30.0, 30.0));

    app.key_with(Modifiers::COMMAND, Key::S);

    app.control("Overwrite");
    app.control("Save As…");
    assert_all_labelled(&app, "Overwrite animated image?");
}

#[test]
fn editing_text_the_canvas_is_labelled() {
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    app.press(Key::T);
    app.click_px((60.0, 60.0));

    assert!(app.harness.state().is_busy() || focused(&app).as_deref() == Some("Image canvas"));
    assert_all_labelled(&app, "text editing");
}
