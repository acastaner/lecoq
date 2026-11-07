//! The one modal dialog up at a time (§6.4).
//!
//! The unsaved-changes prompt, the Resize dialog, the "Overwrite animated image?" prompt, the Help
//! dialogs and the Copy to / Move to warning (ADR 0009) are all modal: while one is up the window's
//! own input (shortcuts, wheel, drops, pan) waits and Esc is its Cancel. One [`Modal`] slot stands
//! in for what used to be five separate `Option` fields spread across the app and the save flow, so
//! "is a modal up?" ([`LecoqApp::dialog_up`]) cannot drift and only one can ever show. Each opener
//! sets `self.modal`; the per-frame [`LecoqApp::show_modal`] lays out whichever is up and routes its
//! outcome.

use std::path::PathBuf;

use lecoq_codec::{SaveFormat, Size};

use super::{LecoqApp, Proceed};
use crate::Command;
use crate::editor::MAX_EDIT_PIXELS;
use crate::ui::help_dialog::{self, HelpDialog};
use crate::ui::resize_dialog::{self, ResizeDialog};
use crate::ui::{file_op_warning, overwrite_dialog, unsaved_dialog};

/// The unsaved-changes prompt while it is up (#35, §6.4): the file name it names, and what to do
/// once Save (after the write) or Discard resolves it.
pub(super) struct UnsavedPrompt {
    pub(super) file_name: String,
    pub(super) then: Proceed,
}

/// "Overwrite animated image?" while it is up (#38, §6.4), and the save it holds.
pub(super) struct OverwritePrompt {
    pub(super) file_name: String,
    pub(super) target: PathBuf,
    pub(super) format: SaveFormat,
}

/// The one modal dialog up at a time (§6.4). Only one variant can be set, so mutual exclusion is a
/// property of the type rather than a convention spread across the app.
pub(super) enum Modal {
    /// "Save changes?" (#35): Cancel · Discard · Save.
    Unsaved(UnsavedPrompt),
    /// Image › Resize… (§4.7): a stateful dialog with its own fields.
    Resize(ResizeDialog),
    /// "Overwrite animated image?" (#38): Cancel · Save As… · Overwrite.
    Overwrite(OverwritePrompt),
    /// Help › Keyboard shortcuts or About.
    Help(HelpDialog),
    /// The Copy to / Move to warning (ADR 0009): a dismiss-only modal carrying its worded body.
    FileOpWarning(String),
}

/// What a frame's modal interaction resolved to, computed while `self.modal` is borrowed and acted
/// on once the borrow is released (routing touches the rest of the app).
enum Act {
    Nothing,
    Unsaved(unsaved_dialog::Answer),
    ResizeCancel,
    Resize(Size),
    Overwrite(overwrite_dialog::Answer),
    Close,
}

impl LecoqApp {
    /// Is a modal dialog up? Window input (shortcuts, wheel, drops, pan) waits meanwhile (§6.4).
    pub(super) fn dialog_up(&self) -> bool {
        self.modal.is_some()
    }

    /// Is the unsaved-changes prompt up (#35)?
    pub(super) fn unsaved_prompt_up(&self) -> bool {
        matches!(self.modal, Some(Modal::Unsaved(_)))
    }

    /// Is the Resize dialog up? A test seam.
    pub fn is_resize_dialog_open(&self) -> bool {
        matches!(self.modal, Some(Modal::Resize(_)))
    }

    /// Is the "Overwrite animated image?" prompt up (#38)?
    pub(super) fn overwrite_prompt_up(&self) -> bool {
        matches!(self.modal, Some(Modal::Overwrite(_)))
    }

    /// Is the Copy to / Move to warning up (ADR 0009)?
    pub(super) fn file_op_warning_up(&self) -> bool {
        matches!(self.modal, Some(Modal::FileOpWarning(_)))
    }

    /// The Copy to / Move to warning's message, while it is up (`None` otherwise): for tests.
    pub fn file_op_warning_message(&self) -> Option<&str> {
        match &self.modal {
            Some(Modal::FileOpWarning(body)) => Some(body),
            _ => None,
        }
    }

    /// Open the unsaved-changes prompt (#35) for `file_name`, holding `then` for its Save/Discard.
    pub(super) fn open_unsaved_prompt(&mut self, file_name: String, then: Proceed) {
        self.modal = Some(Modal::Unsaved(UnsavedPrompt { file_name, then }));
    }

    /// Help › Keyboard shortcuts or About.
    pub(super) fn open_help_dialog(&mut self, dialog: HelpDialog) {
        self.modal = Some(Modal::Help(dialog));
    }

    /// Raise the Copy to / Move to warning modal (ADR 0009) with `body`: an outcome that did not
    /// place the file as asked, worded for the case.
    pub(super) fn warn_file_op(&mut self, body: String) {
        self.modal = Some(Modal::FileOpWarning(body));
    }

    /// Image › Resize…: open the dialog on the Image's current size (§6.4). Starts the
    /// full-resolution decode meanwhile (§4.7: it starts with the first Image operation).
    pub(super) fn show_resize_dialog(&mut self) {
        if !self.editor.can_edit() || self.editor.in_gesture() {
            return;
        }
        if let Some(size) = self.image_size() {
            self.editor.prepare_image_operation();
            self.modal = Some(Modal::Resize(ResizeDialog::new(size, MAX_EDIT_PIXELS)));
        }
    }

    /// Lay out whichever modal is up and act on how it ended. Esc is every dialog's Cancel (§6.4);
    /// the layout itself consumes it, so the window's own shortcuts never see it meanwhile.
    pub(super) fn show_modal(&mut self, ctx: &egui::Context) {
        // Phase one: lay the modal out (a stateful dialog mutates in place) and read its outcome,
        // holding only the borrow of `self.modal`. Phase two routes, which needs the rest of `self`.
        let act = match &mut self.modal {
            None => Act::Nothing,
            Some(Modal::Unsaved(p)) => match unsaved_dialog::show(ctx, &p.file_name) {
                Some(answer) => Act::Unsaved(answer),
                None => Act::Nothing,
            },
            Some(Modal::Resize(dialog)) => match dialog.show(ctx) {
                Some(resize_dialog::Outcome::Cancel) => Act::ResizeCancel,
                Some(resize_dialog::Outcome::Resize(size)) => Act::Resize(size),
                None => Act::Nothing,
            },
            Some(Modal::Overwrite(p)) => match overwrite_dialog::show(ctx, &p.file_name) {
                Some(answer) => Act::Overwrite(answer),
                None => Act::Nothing,
            },
            Some(Modal::Help(dialog)) => {
                if help_dialog::show(ctx, *dialog) {
                    Act::Close
                } else {
                    Act::Nothing
                }
            }
            Some(Modal::FileOpWarning(body)) => {
                if file_op_warning::show(ctx, body) {
                    Act::Close
                } else {
                    Act::Nothing
                }
            }
        };

        match act {
            Act::Nothing => {}
            Act::Close | Act::ResizeCancel => self.modal = None,
            Act::Unsaved(answer) => {
                let Some(Modal::Unsaved(prompt)) = self.modal.take() else {
                    unreachable!("the unsaved prompt was up this frame");
                };
                match answer {
                    unsaved_dialog::Answer::Cancel => {}
                    unsaved_dialog::Answer::Discard => self.proceed(prompt.then),
                    unsaved_dialog::Answer::Save => {
                        self.save.after_save = Some(prompt.then);
                        self.save();
                    }
                }
            }
            Act::Resize(size) => {
                self.modal = None;
                self.execute(Command::Resize {
                    width: size.width,
                    height: size.height,
                });
            }
            Act::Overwrite(answer) => {
                let Some(Modal::Overwrite(prompt)) = self.modal.take() else {
                    unreachable!("the overwrite prompt was up this frame");
                };
                match answer {
                    overwrite_dialog::Answer::Cancel => {}
                    overwrite_dialog::Answer::SaveAs => self.show_save_dialog(),
                    overwrite_dialog::Answer::Overwrite => {
                        self.write_to(prompt.target, prompt.format)
                    }
                }
            }
        }
    }
}
