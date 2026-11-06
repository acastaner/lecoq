//! The Help menu's dialogs: the keyboard shortcuts (§3.1, §4.1, §6.1, §8.6) and About. Modal, like
//! every dialog (§6.4); Esc and Close dismiss them.

use crate::input;
use crate::strings;
use crate::tools::ToolId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelpDialog {
    Shortcuts,
    About,
}

/// Lay out the dialog; true when it was dismissed this frame.
pub fn show(ctx: &egui::Context, dialog: HelpDialog) -> bool {
    let mut close = false;
    egui::Modal::new(egui::Id::unique("lecoq_help")).show(ctx, |ui| {
        ui.set_min_width(320.0);
        match dialog {
            HelpDialog::Shortcuts => {
                ui.heading(strings::KEYBOARD_SHORTCUTS);
                ui.add_space(6.0);
                // Two shortcuts a row, so the list fits without a scroll bar.
                let rows = shortcuts(ctx);
                egui::Grid::new("lecoq_shortcuts")
                    .num_columns(4)
                    .spacing([16.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for pair in rows.chunks(2) {
                            for (keys, does) in pair {
                                ui.label(keys);
                                ui.label(does);
                            }
                            ui.end_row();
                        }
                    });
            }
            HelpDialog::About => {
                ui.heading(strings::ABOUT);
                ui.add_space(6.0);
                ui.label(strings::about_text(env!("CARGO_PKG_VERSION")));
            }
        }
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(strings::CLOSE)
                .accessible_name(strings::CLOSE)
                .clicked()
            {
                close = true;
            }
        });
    });
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        close = true;
    }
    close
}

/// Every shortcut, as (keys, what it does), the keys formatted for the platform (Cmd on macOS).
pub fn shortcuts(ctx: &egui::Context) -> Vec<(String, String)> {
    let f = |s: &egui::KeyboardShortcut| crate::ui::shortcut_text(ctx, s);
    let mut rows: Vec<(String, String)> = [
        (f(&input::SAVE), strings::DOES_SAVE),
        (f(&input::SAVE_AS), strings::DOES_SAVE_AS),
        (f(&input::OPEN), strings::DOES_OPEN),
        (f(&input::COPY), strings::DOES_COPY),
        (f(&input::PASTE), strings::DOES_PASTE),
        (
            f(&input::NEW_FROM_CLIPBOARD),
            strings::DOES_NEW_FROM_CLIPBOARD,
        ),
        (f(&input::COPY_TO), strings::DOES_COPY_TO),
        (f(&input::MOVE_TO), strings::DOES_MOVE_TO),
        (f(&input::DUPLICATE), strings::DOES_DUPLICATE),
        (f(&input::UNDO), strings::DOES_UNDO),
        (
            format!("{} / {}", f(&input::REDO), f(&input::REDO_Y)),
            strings::DOES_REDO,
        ),
        (f(&input::RAISE), strings::DOES_RAISE),
        (f(&input::LOWER), strings::DOES_LOWER),
        (strings::KEY_DELETE.to_owned(), strings::DOES_DELETE),
        (strings::KEY_F11.to_owned(), strings::DOES_FULLSCREEN),
        (strings::KEY_RIGHT.to_owned(), strings::DOES_NEXT),
        (strings::KEY_LEFT.to_owned(), strings::DOES_PREVIOUS),
        (strings::KEY_PLUS.to_owned(), strings::DOES_ZOOM_IN),
        (strings::KEY_MINUS.to_owned(), strings::DOES_ZOOM_OUT),
        (strings::KEY_ZERO.to_owned(), strings::DOES_FIT),
        (strings::KEY_ENTER.to_owned(), strings::DOES_APPLY_CROP),
        (strings::KEYS_TAB.to_owned(), strings::DOES_MENUS),
        (strings::KEYS_ENTER.to_owned(), strings::DOES_ACTIVATE),
    ]
    .into_iter()
    .map(|(keys, does)| (keys, does.to_owned()))
    .collect();
    for tool in ToolId::all() {
        if let Some(key) = crate::ui::tool_strip::key_label(tool) {
            rows.push((key.to_owned(), strings::tool_shortcut(tool)));
        }
    }
    rows
}
