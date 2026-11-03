//! The directory watcher (§3.2): the `notify` crate watches the Image's directory, so the Sibling
//! list follows files added, deleted and renamed while lecoq is open.
//!
//! Events are reduced to what the file system looks like *now*: for each file an event names, the
//! watcher's thread checks whether it exists and reports [`Change::Added`] or [`Change::Removed`].
//! That makes every event kind (create, delete, either half of a rename, a backend's "something
//! changed") the same two cases, and applying them is idempotent.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use notify::event::{EventKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// What happened to the directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// This file exists (it was created, renamed or moved in).
    Added(PathBuf),
    /// This file no longer exists (it was deleted, renamed or moved out).
    Removed(PathBuf),
    /// Events were lost (the kernel queue overflowed, say): scan the directory again.
    Rescan,
}

/// A running watch on one directory. Dropping it stops watching.
pub struct DirWatcher {
    _watcher: RecommendedWatcher,
    changes: mpsc::Receiver<Change>,
}

impl DirWatcher {
    /// Watch `dir` (not its subdirectories). `wake` is called, on the watcher's thread, after
    /// changes are queued.
    pub fn start(dir: &Path, wake: impl Fn() + Send + 'static) -> notify::Result<Self> {
        let (tx, changes) = mpsc::channel();
        let watched = Watched {
            dir: dir.to_owned(),
            canonical: dir.canonicalize().ok(),
        };
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let found = match event {
                    Ok(event) => reduce(&watched, &event),
                    Err(_) => vec![Change::Rescan],
                };
                if found.is_empty() {
                    return;
                }
                for change in found {
                    if tx.send(change).is_err() {
                        return;
                    }
                }
                wake();
            })?;
        watcher.watch(dir, RecursiveMode::NonRecursive)?;
        Ok(Self {
            _watcher: watcher,
            changes,
        })
    }

    /// The changes queued since the last call, oldest first.
    pub fn drain(&self) -> Vec<Change> {
        self.changes.try_iter().collect()
    }
}

/// The watched directory as lecoq names it, and canonically (some backends, like macOS FSEvents,
/// report canonical paths: `/private/var/…` for `/var/…`).
struct Watched {
    dir: PathBuf,
    canonical: Option<PathBuf>,
}

impl Watched {
    /// `path` as lecoq names it, if it is directly in the watched directory.
    fn child(&self, path: &Path) -> Option<PathBuf> {
        let parent = path.parent()?;
        let ours = parent == self.dir || self.canonical.as_deref() == Some(parent);
        Some(self.dir.join(path.file_name()?)).filter(|_| ours)
    }
}

/// The changes one event stands for. Every file directly in the directory is reported, Sibling or
/// not: the current Image may be a file that isn't one (a hidden file, say), and its deletion
/// matters too.
fn reduce(watched: &Watched, event: &notify::Event) -> Vec<Change> {
    if event.need_rescan() || matches!(event.kind, EventKind::Other) {
        return vec![Change::Rescan];
    }
    match event.kind {
        // Reads, and writes or attribute changes to a file that stays where it is, don't change
        // which files there are.
        EventKind::Access(_)
        | EventKind::Modify(ModifyKind::Data(_))
        | EventKind::Modify(ModifyKind::Metadata(_)) => return Vec::new(),
        _ => {}
    }
    event
        .paths
        .iter()
        .filter_map(|path| watched.child(path))
        .map(|path| {
            if path.is_file() {
                Change::Added(path)
            } else {
                Change::Removed(path)
            }
        })
        .collect()
}
