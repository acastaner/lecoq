//! The clipboard-copy flow (#36, §6.2): Ctrl+C Flattens the Image (or serialises the selected
//! Annotations) on the worker thread and puts it on the clipboard. The worker-thread work lives in
//! [`crate::save`]; this is the UI side that drives it and owns [`CopyState`]. Paste and the
//! incoming-image flow stay in [`super`].

use super::LecoqApp;
use crate::editor::SaveStamp;
use crate::save::{AnnotationsCopyJob, CopyJob, FlattenInput, PendingCopy};
use crate::strings;

/// A copy running on the worker thread (§6.2): the pending work, the Document edit it holds a
/// decode for, and the status message to post when it lands. Replaces the anonymous tuple the app
/// used to carry.
pub(super) struct CopyInProgress {
    pending: PendingCopy,
    stamp: SaveStamp,
    message: &'static str,
}

/// The clipboard-copy flow's state (§6.2): a copy running on the worker thread, and a copy asked
/// for while the Document waits for its pixels (a queued Image operation). These travel together.
#[derive(Default)]
pub(super) struct CopyState {
    pub(super) in_progress: Option<CopyInProgress>,
    pub(super) queued: bool,
}

impl LecoqApp {
    /// Ctrl+C, from the worker thread (§7.3). With Annotations selected, they are copied as
    /// clipboard JSON and a PNG of them, together (§6.2). With nothing selected, the Flattened Image
    /// goes to the clipboard as PNG.
    pub(super) fn copy(&mut self) {
        if self.shown.is_none() || self.copy.in_progress.is_some() || self.editor.in_gesture() {
            return;
        }
        if self.editor.save_waits() {
            self.copy.queued = true;
            return;
        }
        let Some(source) = self.editor.save_source() else {
            return;
        };
        let selection = self
            .document()
            .map(|d| d.selection().to_vec())
            .unwrap_or_default();
        let (pending, message) = if selection.is_empty() {
            let job = CopyJob {
                input: FlattenInput {
                    doc: source.doc,
                    source: source.path,
                    fallback: self.full_size_display_pixels(),
                },
                clipboard: self.platform.clipboard.clone(),
            };
            (PendingCopy::spawn_copy(job), strings::IMAGE_COPIED)
        } else {
            let job = AnnotationsCopyJob {
                doc: source.doc,
                ids: selection,
                clipboard: self.platform.clipboard.clone(),
            };
            (
                PendingCopy::spawn_copy_annotations(job),
                strings::ANNOTATIONS_COPIED,
            )
        };
        pending.wake(&self.ctx);
        self.copy.in_progress = Some(CopyInProgress {
            pending,
            stamp: source.stamp,
            message,
        });
    }

    pub(super) fn poll_copy(&mut self, ctx: &egui::Context) {
        if self.copy.queued && !self.editor.save_waits() {
            self.copy.queued = false;
            self.copy();
        }
        let Some(copying) = &self.copy.in_progress else {
            return;
        };
        copying.pending.wake(ctx);
        let Some(result) = copying.pending.take() else {
            return;
        };
        let copying = self.copy.in_progress.take().expect("checked above");
        match result {
            Ok(copied) => {
                self.editor.keep_decoded(copying.stamp, copied.decoded);
                self.post_message(copying.message);
            }
            Err(reason) if copying.message == strings::IMAGE_COPIED => {
                self.post_message(strings::cant_copy_image(&reason))
            }
            Err(reason) => self.post_message(strings::cant_copy_annotations(&reason)),
        }
    }
}
