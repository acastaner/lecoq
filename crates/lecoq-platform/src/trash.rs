//! The system trash (§8.2): the `trash` crate (freedesktop Trash spec on Linux, the Recycle Bin,
//! the macOS Trash).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Moves files to the system trash.
pub trait Trash: Send + Sync {
    /// Move `path` to the trash. The error is a reason fit for the status bar.
    fn trash(&self, path: &Path) -> Result<(), String>;
}

/// The real system trash.
#[derive(Default)]
pub struct OsTrash;

impl Trash for OsTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        trash::delete(path).map_err(|e| e.to_string())
    }
}

/// A trash for tests: it records what was trashed and leaves the file system alone.
#[derive(Default)]
pub struct MemoryTrash {
    state: Mutex<MemoryTrashState>,
}

#[derive(Default)]
struct MemoryTrashState {
    trashed: Vec<PathBuf>,
    failures: VecDeque<String>,
}

impl MemoryTrash {
    /// The paths trashed so far, oldest first.
    pub fn trashed(&self) -> Vec<PathBuf> {
        self.state.lock().unwrap().trashed.clone()
    }

    /// Make the next trash attempt fail with this reason (a read-only folder, say).
    pub fn fail_next(&self, reason: impl Into<String>) {
        self.state.lock().unwrap().failures.push_back(reason.into());
    }
}

impl Trash for MemoryTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if let Some(reason) = state.failures.pop_front() {
            return Err(reason);
        }
        state.trashed.push(path.to_owned());
        Ok(())
    }
}
