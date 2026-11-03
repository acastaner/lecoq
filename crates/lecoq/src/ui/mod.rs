//! The egui chrome (§2), one module per part of the window. Dialogs get their own modules here when
//! their tickets land.

pub mod canvas;
pub mod help_dialog;
pub mod menu_bar;
pub mod overwrite_dialog;
pub mod property_bar;
pub mod resize_dialog;
pub mod status_bar;
pub mod tool_strip;
pub mod unsaved_dialog;

/// A shortcut as menus, tooltips and the shortcuts list show it: `Ctrl+Shift+S`, `Cmd+S` on macOS
/// (§6.1), `Ctrl+]` rather than egui's `Ctrl+CloseBracket`.
pub fn shortcut_text(ctx: &egui::Context, shortcut: &egui::KeyboardShortcut) -> String {
    let names = egui::ModifierNames::NAMES;
    let mut s = names.format(&shortcut.modifiers, ctx.os().is_mac());
    if !s.is_empty() {
        s += names.concat;
    }
    s += match shortcut.logical_key {
        egui::Key::OpenBracket => "[",
        egui::Key::CloseBracket => "]",
        key => key.name(),
    };
    s
}
