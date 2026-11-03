//! The unsaved-changes prompt (§6.4): "Save changes?" / "<name> has unsaved changes.", with
//! Cancel · Discard · **Save**. Modal; Esc is Cancel.

use crate::strings;

/// The button the user picked, if any this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Answer {
    Cancel,
    Discard,
    Save,
}

pub fn show(ctx: &egui::Context, file_name: &str) -> Option<Answer> {
    let mut answer = None;
    egui::Modal::new(egui::Id::unique("lecoq_unsaved_changes")).show(ctx, |ui| {
        ui.set_max_width(360.0);
        ui.heading(strings::SAVE_CHANGES_TITLE);
        ui.label(strings::unsaved_changes(file_name));
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Right to left: the primary button (bold) ends up last, on the right.
            if ui
                .button(egui::RichText::new(strings::SAVE).strong())
                .accessible_name(strings::SAVE)
                .clicked()
            {
                answer = Some(Answer::Save);
            }
            if ui
                .button(strings::DISCARD)
                .accessible_name(strings::DISCARD)
                .clicked()
            {
                answer = Some(Answer::Discard);
            }
            if ui
                .button(strings::CANCEL)
                .accessible_name(strings::CANCEL)
                .clicked()
            {
                answer = Some(Answer::Cancel);
            }
        });
    });
    // Esc means Cancel in every dialog. A click on the backdrop does nothing: the dialog is modal.
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        answer = Some(Answer::Cancel);
    }
    answer
}
