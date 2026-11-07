//! A native file dialog while it is up ([`DialogSlot`]): the open dialog, the Save As dialog and
//! the Copy to / Move to folder picker.
//!
//! The app never blocks on a dialog (§8.2): it opens one and polls each frame for the answer, which
//! arrives through a channel from whatever thread the dialog finished on. One slot behind a small
//! interface stands in for the channel receiver and the `try_recv` match that each of the three
//! dialogs used to repeat.

use std::path::PathBuf;
use std::sync::mpsc;

use lecoq_platform::DialogReply;

/// A file dialog waiting for its answer.
pub(crate) struct DialogSlot {
    answer: mpsc::Receiver<Option<PathBuf>>,
}

impl DialogSlot {
    /// Open a dialog and hold its answer channel. `present` is handed the [`DialogReply`] to give
    /// the platform picker; the picker calls it once with the path chosen (or `None` on Cancel),
    /// which wakes `ctx` and lands in [`Self::answer`].
    pub fn open(ctx: &egui::Context, present: impl FnOnce(DialogReply)) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        present(Box::new(move |chosen| {
            let _ = tx.send(chosen);
            ctx.request_repaint();
        }));
        Self { answer: rx }
    }

    /// The answer, once it has come: `Some(path)` chosen, or `Some(None)` for Cancel (or a dialog
    /// that went away). `None` while the dialog is still up. The caller drops the slot once it has
    /// an answer.
    pub fn answer(&self) -> Option<Option<PathBuf>> {
        match self.answer.try_recv() {
            Ok(chosen) => Some(chosen),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot_with_reply() -> (DialogSlot, DialogReply) {
        let ctx = egui::Context::default();
        let mut reply = None;
        let slot = DialogSlot::open(&ctx, |r| reply = Some(r));
        (slot, reply.expect("present is called synchronously"))
    }

    #[test]
    fn no_answer_while_the_dialog_is_up() {
        let (slot, _reply) = slot_with_reply();
        assert_eq!(slot.answer(), None);
    }

    #[test]
    fn a_chosen_path_is_the_answer() {
        let (slot, reply) = slot_with_reply();
        reply(Some(PathBuf::from("/tmp/picked.png")));
        assert_eq!(slot.answer(), Some(Some(PathBuf::from("/tmp/picked.png"))));
    }

    #[test]
    fn cancel_is_an_answer_of_none() {
        let (slot, reply) = slot_with_reply();
        reply(None);
        assert_eq!(slot.answer(), Some(None));
    }

    #[test]
    fn a_dialog_that_went_away_answers_none() {
        let (slot, reply) = slot_with_reply();
        drop(reply);
        assert_eq!(slot.answer(), Some(None));
    }
}
