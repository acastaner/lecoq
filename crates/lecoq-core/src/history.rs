//! The History (§4.8): one linear undo/redo sequence of steps; one gesture is one step.
//!
//! A step stores the Document's editable state before and after it ([`Snapshot`]). Snapshots
//! make every edit undoable without per-edit undo code: a new Annotation type, a new property or
//! a new kind of edit gets undo and redo for free.
//!
//! Image-operation steps also carry the Image ([`ImageState`]: pixels behind an `Arc`, shared
//! between one step's `after` and the next one's `before`) in both snapshots. Other steps carry
//! `None`: the History is linear, so the Image when undoing or redoing them is the one they had.
//! The pixels held by steps are capped ([`HISTORY_PIXEL_CAP`]); beyond it the oldest steps are
//! dropped.

use std::sync::Arc;

use crate::annotation::{Annotation, AnnotationId};
use crate::document::Pixels;
use crate::style::PropertyKind;

/// §4.8: pixels stored in the History are capped at about 512 MB.
pub const HISTORY_PIXEL_CAP: usize = 512 * 1024 * 1024;

/// The Image as an Image operation found or left it.
#[derive(Clone, Debug)]
pub(crate) struct ImageState {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Pixels>,
    /// Identifies these pixels within the Document; 0 is the decoded file.
    pub generation: u64,
}

impl PartialEq for ImageState {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation
    }
}

/// The Document's editable state, as stored in a step.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Snapshot {
    pub annotations: Vec<Annotation>,
    pub selection: Vec<AnnotationId>,
    /// Only in Image-operation steps.
    pub image: Option<ImageState>,
}

/// What a step was, for merging property changes made in one widget interaction.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StepKind {
    Gesture,
    ImageOperation,
    Property {
        kind: PropertyKind,
        targets: Vec<AnnotationId>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Step {
    pub kind: StepKind,
    pub before: Snapshot,
    pub after: Snapshot,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    steps: Vec<Step>,
    /// Steps `..position` are done; `position..` can be redone.
    position: usize,
    /// The position when the Image was last saved (or opened). `None`: that state is no longer
    /// reachable (it was undone and then replaced by a new step).
    saved_at: Option<usize>,
}

impl History {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            position: 0,
            saved_at: Some(0),
        }
    }

    /// Record a done step. Drops the redo tail.
    pub fn push(&mut self, step: Step) {
        self.steps.truncate(self.position);
        if self.saved_at.is_some_and(|s| s > self.position) {
            self.saved_at = None;
        }
        self.steps.push(step);
        self.position += 1;
    }

    /// The last done step, if it is the newest one (nothing to redo).
    pub fn last_mut(&mut self) -> Option<&mut Step> {
        if self.position == self.steps.len() {
            self.steps.last_mut()
        } else {
            None
        }
    }

    pub fn undo(&mut self) -> Option<&Step> {
        if self.position == 0 {
            return None;
        }
        self.position -= 1;
        Some(&self.steps[self.position])
    }

    pub fn redo(&mut self) -> Option<&Step> {
        let step = self.steps.get(self.position)?;
        self.position += 1;
        Some(step)
    }

    pub fn can_undo(&self) -> bool {
        self.position > 0
    }

    pub fn can_redo(&self) -> bool {
        self.position < self.steps.len()
    }

    /// The number of done steps.
    pub fn undo_len(&self) -> usize {
        self.position
    }

    pub fn redo_len(&self) -> usize {
        self.steps.len() - self.position
    }

    pub fn is_modified(&self) -> bool {
        self.saved_at != Some(self.position)
    }

    pub fn mark_saved(&mut self) {
        self.saved_at = Some(self.position);
    }

    /// Drop the oldest steps until the pixels they hold, other than `current` (the Document's own),
    /// fit in `cap` bytes. Only done steps are dropped.
    pub fn enforce_pixel_cap(&mut self, cap: usize, current: Option<&Arc<Pixels>>) {
        while self.position > 0 && self.stored_pixel_bytes(current) > cap {
            self.steps.remove(0);
            self.position -= 1;
            self.saved_at = match self.saved_at {
                Some(0) | None => None,
                Some(n) => Some(n - 1),
            };
        }
    }

    /// Bytes of the distinct pixel buffers held by steps, except `current`.
    fn stored_pixel_bytes(&self, current: Option<&Arc<Pixels>>) -> usize {
        let mut seen: Vec<&Arc<Pixels>> = Vec::new();
        let images = self
            .steps
            .iter()
            .flat_map(|s| [&s.before.image, &s.after.image])
            .flatten();
        for image in images {
            let p = &image.pixels;
            let counted = seen.iter().any(|s| Arc::ptr_eq(s, p));
            if !counted && !current.is_some_and(|c| Arc::ptr_eq(c, p)) {
                seen.push(p);
            }
        }
        seen.iter().map(|p| p.byte_len()).sum()
    }
}
