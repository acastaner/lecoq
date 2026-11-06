//! The menu bar (§2): File, Edit, Image, View, Help, with the quick actions on its right.
//!
//! Every action of §3.1, §4.7 and §6.1 has an entry here, so the menus list everything lecoq does
//! and Tab, the arrow keys and Enter reach all of it (§8.6). Each entry shows its shortcut, formatted
//! for the platform (Cmd on macOS), and turns into a [`Command`]; the app executes it. Entries that
//! can't act right now are disabled, not hidden.

use egui::{Color32, Pos2, Rect, Stroke, pos2, vec2};

use crate::Command;
use crate::input;
use crate::strings;
use lecoq_core::Restack;

/// What the menus show this frame.
pub struct MenuBar {
    pub allow_multiple_instances: bool,
    /// View → *Follow files after copy/move* (issue #53).
    pub follow_files: bool,
    /// Can Image operations run (an editable Image is open)?
    pub can_edit: bool,
    /// Is an Image shown (save, copy, zoom)?
    pub has_image: bool,
    /// Is there a file on screen (trash, Siblings)?
    pub has_file: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Are Annotations selected (duplicate, stacking)?
    pub selection: bool,
}

/// The quick actions' button size.
const QUICK: egui::Vec2 = vec2(24.0, 20.0);
/// How many quick actions there are (§2).
const QUICK_ACTIONS: f32 = 6.0;

/// Lay out the menu bar; returns the command chosen this frame, if any.
pub fn show(ui: &mut egui::Ui, bar: &MenuBar) -> Option<Command> {
    let mut command = None;
    egui::Panel::top("menu_bar").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(strings::MENU_FILE, |ui| file_items(ui, bar, &mut command));
            ui.menu_button(strings::MENU_EDIT, |ui| edit_items(ui, bar, &mut command));
            ui.menu_button(strings::MENU_IMAGE, |ui| image_items(ui, bar, &mut command));
            ui.menu_button(strings::MENU_VIEW, |ui| view_items(ui, bar, &mut command));
            ui.menu_button(strings::MENU_HELP, |ui| help_items(ui, &mut command));
            // Right-aligned, but laid out left to right so Tab visits them in reading order.
            let width =
                QUICK_ACTIONS * QUICK.x + (QUICK_ACTIONS - 1.0) * ui.spacing().item_spacing.x;
            ui.add_space((ui.available_width() - width).max(0.0));
            quick_actions(ui, bar, &mut command);
        });
    });
    command
}

/// A menu entry with its shortcut beside it; true when chosen.
fn item(ui: &mut egui::Ui, enabled: bool, label: &str, shortcut: Option<String>) -> bool {
    let mut button = egui::Button::new(label);
    if let Some(shortcut) = shortcut {
        button = button.shortcut_text(shortcut);
    }
    ui.add_enabled(enabled, button)
        .accessible_name(label)
        .clicked()
}

fn shortcut(ui: &egui::Ui, shortcut: &egui::KeyboardShortcut) -> Option<String> {
    Some(crate::ui::shortcut_text(ui.ctx(), shortcut))
}

fn file_items(ui: &mut egui::Ui, bar: &MenuBar, command: &mut Option<Command>) {
    let entries = [
        (strings::OPEN, true, &input::OPEN, Command::OpenDialog),
        (strings::SAVE, bar.has_image, &input::SAVE, Command::Save),
        (
            strings::SAVE_AS,
            bar.has_image,
            &input::SAVE_AS,
            Command::SaveAs,
        ),
        (
            strings::NEW_FROM_CLIPBOARD,
            true,
            &input::NEW_FROM_CLIPBOARD,
            Command::NewFromClipboard,
        ),
    ];
    for (i, (label, enabled, keys, chosen)) in entries.into_iter().enumerate() {
        if i == 3 {
            ui.separator();
        }
        if item(ui, enabled, label, shortcut(ui, keys)) {
            *command = Some(chosen);
        }
    }
    ui.separator();
    file_operations(ui, bar, command);
}

/// The File operations group (§2, issue #53): Copy to…, Move to…, Move to trash, each enabled when
/// there is a file on screen. Shared by the File menu and the Context menu so the two never drift
/// and every Context-menu action also lives in the menu bar (ADR 0008).
fn file_operations(ui: &mut egui::Ui, bar: &MenuBar, command: &mut Option<Command>) {
    if item(
        ui,
        bar.has_file,
        strings::COPY_TO,
        shortcut(ui, &input::COPY_TO),
    ) {
        *command = Some(Command::CopyTo);
    }
    if item(
        ui,
        bar.has_file,
        strings::MOVE_TO,
        shortcut(ui, &input::MOVE_TO),
    ) {
        *command = Some(Command::MoveTo);
    }
    if item(ui, bar.has_file, strings::MOVE_TO_TRASH, None) {
        *command = Some(Command::Trash);
    }
}

/// The Context menu (ADR 0008, issue #53), opened by a right-click on the canvas in Select mode with
/// no Annotation selected: the clipboard actions then the File operations, mirroring the menu bar
/// with the same labels, shortcuts and enabling rules. The empty window shows only Paste enabled.
pub fn context_menu(ui: &mut egui::Ui, bar: &MenuBar, command: &mut Option<Command>) {
    if item(ui, bar.has_image, strings::COPY, shortcut(ui, &input::COPY)) {
        *command = Some(Command::Copy);
    }
    if item(ui, true, strings::PASTE, shortcut(ui, &input::PASTE)) {
        *command = Some(Command::Paste);
    }
    ui.separator();
    file_operations(ui, bar, command);
}

fn edit_items(ui: &mut egui::Ui, bar: &MenuBar, command: &mut Option<Command>) {
    if item(ui, bar.can_undo, strings::UNDO, shortcut(ui, &input::UNDO)) {
        *command = Some(Command::Undo);
    }
    if item(ui, bar.can_redo, strings::REDO, shortcut(ui, &input::REDO)) {
        *command = Some(Command::Redo);
    }
    ui.separator();
    if item(ui, bar.has_image, strings::COPY, shortcut(ui, &input::COPY)) {
        *command = Some(Command::Copy);
    }
    if item(ui, true, strings::PASTE, shortcut(ui, &input::PASTE)) {
        *command = Some(Command::Paste);
    }
    if item(
        ui,
        bar.selection,
        strings::DUPLICATE,
        shortcut(ui, &input::DUPLICATE),
    ) {
        *command = Some(Command::Duplicate);
    }
    // Edit › Delete is the Annotation command: enabled only with a selection (issue #53). The
    // Delete key still trashes the file when nothing is selected (§3.1); that is unchanged.
    let delete = Some(strings::KEY_DELETE.to_owned());
    if item(ui, bar.selection, strings::DELETE, delete) {
        *command = Some(Command::Delete);
    }
    ui.separator();
    let stacking = [
        (
            strings::BRING_FORWARD,
            shortcut(ui, &input::RAISE),
            Restack::Up,
        ),
        (
            strings::SEND_BACKWARD,
            shortcut(ui, &input::LOWER),
            Restack::Down,
        ),
        (strings::BRING_TO_FRONT, None, Restack::Front),
        (strings::SEND_TO_BACK, None, Restack::Back),
    ];
    for (label, keys, restack) in stacking {
        if item(ui, bar.selection, label, keys) {
            *command = Some(Command::Restack(restack));
        }
    }
}

/// The Image menu's entries (§4.7), also behind its quick action.
fn image_items(ui: &mut egui::Ui, bar: &MenuBar, command: &mut Option<Command>) {
    let entries = [
        (strings::ROTATE_LEFT, Command::Rotate { clockwise: false }),
        (strings::ROTATE_RIGHT, Command::Rotate { clockwise: true }),
        (strings::FLIP_HORIZONTAL, Command::Flip { horizontal: true }),
        (strings::FLIP_VERTICAL, Command::Flip { horizontal: false }),
        (strings::RESIZE_MENU, Command::ShowResizeDialog),
    ];
    for (i, (label, chosen)) in entries.into_iter().enumerate() {
        if i == 4 {
            ui.separator();
        }
        if item(ui, bar.can_edit, label, None) {
            *command = Some(chosen);
        }
    }
}

fn view_items(ui: &mut egui::Ui, bar: &MenuBar, command: &mut Option<Command>) {
    let key = |k: &str| Some(k.to_owned());
    let entries = [
        (strings::ZOOM_IN, key(strings::KEY_PLUS), Command::ZoomIn),
        (strings::ZOOM_OUT, key(strings::KEY_MINUS), Command::ZoomOut),
        (
            strings::FIT_TO_WINDOW,
            key(strings::KEY_ZERO),
            Command::ZoomFit,
        ),
    ];
    for (label, keys, chosen) in entries {
        if item(ui, bar.has_image, label, keys) {
            *command = Some(chosen);
        }
    }
    if item(ui, true, strings::FULLSCREEN, key(strings::KEY_F11)) {
        *command = Some(Command::ToggleFullscreen);
    }
    ui.separator();
    if item(
        ui,
        bar.has_file,
        strings::PREVIOUS_IMAGE,
        key(strings::KEY_LEFT),
    ) {
        *command = Some(Command::PreviousSibling);
    }
    if item(
        ui,
        bar.has_file,
        strings::NEXT_IMAGE,
        key(strings::KEY_RIGHT),
    ) {
        *command = Some(Command::NextSibling);
    }
    ui.separator();
    let mut allow = bar.allow_multiple_instances;
    if ui
        .checkbox(&mut allow, strings::ALLOW_MULTIPLE_INSTANCES)
        .accessible_name(strings::ALLOW_MULTIPLE_INSTANCES)
        .changed()
    {
        *command = Some(Command::SetAllowMultipleInstances(allow));
    }
    let mut follow = bar.follow_files;
    if ui
        .checkbox(&mut follow, strings::FOLLOW_FILES)
        .accessible_name(strings::FOLLOW_FILES)
        .changed()
    {
        *command = Some(Command::SetFollowFiles(follow));
    }
}

fn help_items(ui: &mut egui::Ui, command: &mut Option<Command>) {
    if item(ui, true, strings::KEYBOARD_SHORTCUTS, None) {
        *command = Some(Command::ShowShortcuts);
    }
    if item(ui, true, strings::ABOUT, None) {
        *command = Some(Command::ShowAbout);
    }
}

/// The quick actions (§2), left to right: save, save as, copy, undo, the Image menu, trash.
fn quick_actions(ui: &mut egui::Ui, bar: &MenuBar, command: &mut Option<Command>) {
    let ctx = ui.ctx().clone();
    let tip = |label: &str, keys: &egui::KeyboardShortcut| {
        strings::with_shortcut(label, &crate::ui::shortcut_text(&ctx, keys))
    };
    let actions: [(&str, String, bool, Icon, Command); 4] = [
        (
            strings::QUICK_SAVE,
            tip(strings::QUICK_SAVE, &input::SAVE),
            bar.has_image,
            Icon::Save,
            Command::Save,
        ),
        (
            strings::QUICK_SAVE_AS,
            tip(strings::QUICK_SAVE_AS, &input::SAVE_AS),
            bar.has_image,
            Icon::SaveAs,
            Command::SaveAs,
        ),
        (
            strings::QUICK_COPY,
            tip(strings::QUICK_COPY, &input::COPY),
            bar.has_image,
            Icon::Copy,
            Command::Copy,
        ),
        (
            strings::QUICK_UNDO,
            tip(strings::QUICK_UNDO, &input::UNDO),
            bar.can_undo,
            Icon::Undo,
            Command::Undo,
        ),
    ];
    for (label, tooltip, enabled, icon, chosen) in actions {
        if quick(ui, enabled, label, &tooltip, icon) {
            *command = Some(chosen);
        }
    }
    let button = egui::Button::new("").min_size(QUICK);
    let (response, _) = egui::containers::menu::MenuButton::from_button(button)
        .ui(ui, |ui| image_items(ui, bar, command));
    let response = response
        .on_hover_text(strings::QUICK_IMAGE)
        .accessible_name(strings::QUICK_IMAGE);
    paint(ui, &response, true, Icon::Rotate);
    if quick(
        ui,
        bar.has_file,
        strings::QUICK_TRASH,
        strings::QUICK_TRASH,
        Icon::Trash,
    ) {
        *command = Some(Command::Trash);
    }
}

/// A quick-action button with a painted icon; true when pressed.
fn quick(ui: &mut egui::Ui, enabled: bool, label: &str, tooltip: &str, icon: Icon) -> bool {
    let response = ui
        .add_enabled(enabled, egui::Button::new("").min_size(QUICK))
        .on_hover_text(tooltip)
        .on_disabled_hover_text(tooltip)
        .accessible_name(label);
    paint(ui, &response, enabled, icon);
    response.clicked()
}

#[derive(Clone, Copy)]
enum Icon {
    Save,
    SaveAs,
    Copy,
    Undo,
    Rotate,
    Trash,
}

/// The icons are painted: egui's built-in fonts lack these symbols, and a missing glyph panics.
fn paint(ui: &egui::Ui, response: &egui::Response, enabled: bool, icon: Icon) {
    let c = if enabled {
        ui.visuals().widgets.inactive.fg_stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    let p = ui.painter();
    let r = response.rect.shrink2(vec2(6.0, 3.0));
    let s = Stroke::new(1.5, c);
    let at = |u: f32, v: f32| -> Pos2 { pos2(r.min.x + u * r.width(), r.min.y + v * r.height()) };
    match icon {
        Icon::Save | Icon::SaveAs => {
            // A floppy disk: the body, the label and the shutter.
            p.rect_stroke(r, 1.0, s, egui::StrokeKind::Inside);
            p.rect_filled(Rect::from_min_max(at(0.25, 0.0), at(0.75, 0.35)), 0.0, c);
            p.rect_stroke(
                Rect::from_min_max(at(0.2, 0.55), at(0.8, 1.0)),
                0.0,
                Stroke::new(1.0, c),
                egui::StrokeKind::Inside,
            );
            if matches!(icon, Icon::SaveAs) {
                // A pen stroke across the corner.
                let thick = Stroke::new(2.0, c);
                p.line_segment([at(0.55, 1.15), at(1.15, 0.55)], thick);
            }
        }
        Icon::Copy => {
            let back = Rect::from_min_max(at(0.0, 0.0), at(0.65, 0.7));
            let front = Rect::from_min_max(at(0.35, 0.3), at(1.0, 1.0));
            p.rect_stroke(back, 1.0, s, egui::StrokeKind::Inside);
            p.rect_filled(front, 1.0, ui.visuals().widgets.inactive.weak_bg_fill);
            p.rect_stroke(front, 1.0, s, egui::StrokeKind::Inside);
        }
        Icon::Undo => {
            // A hook curving back to the left, with its arrowhead.
            let centre = at(0.55, 0.6);
            let radius = r.height() * 0.4;
            let points: Vec<_> = (0..=16)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2 + i as f32 / 16.0 * std::f32::consts::PI;
                    centre + radius * vec2(a.cos(), a.sin())
                })
                .collect();
            let top = points[0];
            let tip = top - vec2(radius * 0.8, 0.0);
            p.add(egui::Shape::line(points, s));
            p.line_segment([top, tip], s);
            p.line_segment([tip, tip + vec2(3.5, -3.0)], s);
            p.line_segment([tip, tip + vec2(3.5, 3.0)], s);
        }
        Icon::Rotate => paint_rotate_icon(p, response.rect.shrink2(vec2(6.0, 4.0)), c),
        Icon::Trash => {
            // A bin: the lid, the body and two ribs.
            p.line_segment([at(0.0, 0.15), at(1.0, 0.15)], s);
            p.line_segment([at(0.35, 0.0), at(0.65, 0.0)], s);
            let body = [at(0.12, 0.15), at(0.22, 1.0), at(0.78, 1.0), at(0.88, 0.15)];
            p.add(egui::Shape::line(body.to_vec(), s));
            p.line_segment([at(0.42, 0.35), at(0.42, 0.85)], Stroke::new(1.0, c));
            p.line_segment([at(0.58, 0.35), at(0.58, 0.85)], Stroke::new(1.0, c));
        }
    }
}

/// A clockwise arrow round a circle (⟳).
fn paint_rotate_icon(p: &egui::Painter, r: Rect, c: Color32) {
    let s = Stroke::new(1.5, c);
    let centre = r.center();
    let radius = r.width().min(r.height()) / 2.0;
    let start = -std::f32::consts::FRAC_PI_2;
    let end = start + 1.6 * std::f32::consts::PI;
    let points: Vec<_> = (0..=20)
        .map(|i| {
            let a = start + i as f32 / 20.0 * (end - start);
            centre + radius * vec2(a.cos(), a.sin())
        })
        .collect();
    let tip = points[points.len() - 1];
    p.add(egui::Shape::line(points, s));
    // The arrowhead: two strokes back from the tip, either side of the direction of travel.
    let along = vec2(-end.sin(), end.cos());
    let across = vec2(-along.y, along.x);
    p.line_segment([tip, tip - 4.0 * along + 3.0 * across], s);
    p.line_segment([tip, tip - 4.0 * along - 3.0 * across], s);
}
