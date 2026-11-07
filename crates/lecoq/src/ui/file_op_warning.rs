//! The Copy to / Move to warning (ADR 0009): a File operation that did *not* place the file as
//! asked — a hard error, a name clash, or the file already being in that folder — interrupts with a
//! dismissable modal, carrying the outcome worded for the case. Unlike the overwrite prompt it
//! offers no choice (the operation is already over), so it has one Close button. Modal; Esc closes.

use crate::strings;

/// Show the warning, with `body` the outcome-specific message. Returns `true` on the frame its
/// Close button or Esc is hit.
pub fn show(ctx: &egui::Context, body: &str) -> bool {
    let mut closed = false;
    egui::Modal::new(egui::Id::unique("lecoq_file_op_warning")).show(ctx, |ui| {
        ui.set_max_width(360.0);
        ui.heading(strings::FILE_OP_WARNING_TITLE);
        ui.label(body);
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(egui::RichText::new(strings::CLOSE).strong())
                .accessible_name(strings::CLOSE)
                .clicked()
            {
                closed = true;
            }
        });
    });
    // Esc dismisses, as in every dialog. A click on the backdrop does nothing: the dialog is modal.
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        closed = true;
    }
    closed
}
