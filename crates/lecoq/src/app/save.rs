//! The save/export flow (#34, #35, #38, §5.1, §5.3, §6.4): the Save As dialog, the worker-thread
//! save, the "Overwrite animated image?" prompt, and the action a prompt's Save holds until the
//! file is written. The worker-thread Flatten-and-write itself lives in [`crate::save`]; this is
//! the UI side that drives it and owns [`SaveState`].

use std::path::PathBuf;

use lecoq_codec::SaveFormat;

use super::dialog::DialogSlot;
use super::job::{self, Job};
use super::modal::{Modal, OverwritePrompt};
use super::{LecoqApp, Proceed, file_name, same_file};
use crate::editor::SaveStamp;
use crate::save::{FlattenInput, SaveJob, SaveResult};
use crate::strings;

/// The save/export flow's state (#34, #35, #38): the Save As dialog, a save on the worker thread,
/// a save queued behind a pending crop, the overwrite prompt, and the action a prompt's Save holds
/// until the file is written. These fields always travel together.
#[derive(Default)]
pub(super) struct SaveState {
    /// The Save As dialog while it is up (#34).
    pub(super) dialog: Option<DialogSlot>,
    pub(super) saving: Option<Saving>,
    /// A save asked for while the Document waits for its pixels (a queued crop): started then.
    pub(super) queued: Option<(PathBuf, SaveFormat)>,
    /// The action the prompt's Save is holding until the save (and any Save As dialog) is done.
    pub(super) after_save: Option<Proceed>,
}

/// A save running on the worker thread (#34).
pub(super) struct Saving {
    job: Job<SaveResult>,
    stamp: SaveStamp,
    /// The file the save writes: the "Can't save" name and the follow-up (#21) read it.
    target: PathBuf,
}

impl Saving {
    /// Is the worker-thread save still outstanding?
    pub(super) fn is_running(&self) -> bool {
        self.job.is_running()
    }
}

impl LecoqApp {
    /// Ctrl+S: write the Image back in its own format; Save As (defaulting to PNG) for a read-only
    /// one (§5.1) or a clipboard image (§6.1).
    pub(super) fn save(&mut self) {
        let Some(shown) = &self.shown else {
            return;
        };
        let Some(path) = shown.path.clone() else {
            self.show_save_dialog();
            return;
        };
        match crate::save::in_place_format(&path) {
            Some(format) => self.save_to(path, format),
            None => self.show_save_dialog(),
        }
    }

    /// Ctrl+Shift+S, or Ctrl+S on a read-only format. One save (or dialog) at a time.
    pub(super) fn show_save_dialog(&mut self) {
        if self.save.dialog.is_some() || self.save.saving.is_some() {
            return;
        }
        let Some(shown) = &self.shown else {
            return;
        };
        let suggested = crate::save::suggestion(shown.path.as_deref());
        let ctx = self.ctx.clone();
        self.save.dialog = Some(DialogSlot::open(&ctx, |reply| {
            self.platform
                .dialogs
                .save_image(strings::SAVE_AS_DIALOG_TITLE, &suggested, reply);
        }));
    }

    pub(super) fn poll_save_dialog(&mut self) {
        let Some(slot) = &self.save.dialog else {
            return;
        };
        let Some(answer) = slot.answer() else {
            return;
        };
        self.save.dialog = None;
        if let Some(chosen) = answer {
            let (target, format) = crate::save::chosen_target(chosen);
            self.save_to(target, format);
        }
    }

    /// Save the Image to `target`. Over its own animated file, ask "Overwrite animated image?"
    /// first (§3.4, §6.4): the file written is a still image.
    fn save_to(&mut self, target: PathBuf, format: SaveFormat) {
        if self.save.saving.is_some() || self.overwrite_prompt_up() {
            return;
        }
        if let Some(shown) = &self.shown
            && shown.is_animation()
            && shown
                .path
                .as_deref()
                .is_some_and(|own| same_file(own, &target))
        {
            // `modal` and `shown` are disjoint fields, so this set is fine inside the `shown` borrow.
            self.modal = Some(Modal::Overwrite(OverwritePrompt {
                file_name: shown.file_name.clone(),
                target,
                format,
            }));
            return;
        }
        self.write_to(target, format);
    }

    /// Flatten and write the Image to `target` on the worker thread (§7.3).
    pub(super) fn write_to(&mut self, target: PathBuf, format: SaveFormat) {
        if self.save.saving.is_some() {
            return;
        }
        if self.editor.save_waits() {
            self.save.queued = Some((target, format));
            return;
        }
        // A text box being edited is finished first, so it is saved (#28).
        self.editor.finish_text_editing();
        let Some(source) = self.editor.save_source() else {
            return;
        };
        let saved_target = target.clone();
        let saving = crate::save::spawn_save(
            SaveJob {
                input: FlattenInput {
                    doc: source.doc,
                    source: source.path,
                    fallback: self.full_size_display_pixels(),
                },
                target,
                format,
                jpeg_quality: self.config.jpeg_quality,
            },
            Some(job::waker(&self.ctx)),
        );
        self.save.saving = Some(Saving {
            job: saving,
            stamp: source.stamp,
            target: saved_target,
        });
    }

    pub(super) fn poll_save(&mut self, ctx: &egui::Context) {
        if self.save.queued.is_some() && !self.editor.save_waits() {
            let (target, format) = self.save.queued.take().expect("checked above");
            self.write_to(target, format);
        }
        let Some(saving) = &mut self.save.saving else {
            return;
        };
        let Some(result) = saving.job.poll() else {
            return;
        };
        let saving = self.save.saving.take().expect("checked above");
        let target = saving.target;
        match result {
            Ok(saved) => {
                self.editor.saved(saving.stamp, &target, saved.decoded);
                // What was written is a still image (§3.4): the file on screen is one now.
                if let Some(shown) = &mut self.shown {
                    shown.stop_animation();
                    shown.frozen = false;
                }
                self.saved_as(ctx, &target);
            }
            Err(reason) => {
                self.post_message(strings::cant_save(&file_name(&target), &reason));
            }
        }
    }
}
