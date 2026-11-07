//! The directory scan (§3.2): the Image's Siblings, in the file manager's order.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::DirEntry;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::order::NameOrder;
use crate::watch::{Change, DirWatcher};

/// One Sibling: its path and the name it's ordered by.
#[derive(Clone, Debug)]
pub struct Sibling {
    pub path: PathBuf,
    /// The file name as the file manager shows it (lossily decoded).
    pub name: String,
}

/// The Siblings of a directory, in order.
pub struct SiblingList {
    dir: PathBuf,
    siblings: Vec<Sibling>,
    /// Each Sibling's path to its index in `siblings`, so [`SiblingList::locate`] answers in
    /// constant time instead of scanning the whole list on the frame the status bar asks (#62).
    /// Rebuilt by [`SiblingList::reindex`] whenever `siblings` changes, so it never goes stale.
    index: HashMap<PathBuf, usize>,
    order: Arc<dyn NameOrder>,
}

impl SiblingList {
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn len(&self) -> usize {
        self.siblings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.siblings.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Sibling> {
        self.siblings.get(index)
    }

    /// Where `path` is in the list: `Ok(index)`, or `Err(index)` where it would be if it were a
    /// Sibling (a hidden file, an unsupported extension, a file from another directory).
    pub fn locate(&self, path: &Path) -> Result<usize, usize> {
        if let Some(&index) = self.index.get(path) {
            return Ok(index);
        }
        let name = display_name(path);
        let index = self
            .siblings
            .partition_point(|s| self.order.compare(&s.name, &name) != Ordering::Greater);
        Err(index)
    }

    /// Rebuild [`Self::index`] from `siblings`. Called by every method that changes the list, so a
    /// lookup never sees a stale index.
    fn reindex(&mut self) {
        self.index = self
            .siblings
            .iter()
            .enumerate()
            .map(|(i, s)| (s.path.clone(), i))
            .collect();
    }

    /// Take `path` out of the list (it was trashed). Returns whether it was there.
    pub fn remove(&mut self, path: &Path) -> bool {
        let before = self.siblings.len();
        self.siblings.retain(|s| s.path != path);
        let changed = self.siblings.len() != before;
        if changed {
            self.reindex();
        }
        changed
    }

    /// Follow a file added to or removed from the directory (§3.2). Files that aren't Siblings are
    /// ignored. Returns whether the list changed.
    pub fn apply(&mut self, change: &Change) -> bool {
        match change {
            Change::Added(path) => {
                let name = display_name(path);
                if path.parent() != Some(self.dir.as_path()) || !is_candidate_at(&name, path) {
                    return false;
                }
                let Err(index) = self.locate(path) else {
                    return false;
                };
                self.siblings.insert(
                    index,
                    Sibling {
                        path: path.clone(),
                        name,
                    },
                );
                self.reindex();
                true
            }
            Change::Removed(path) => self.remove(path),
            Change::Rescan => false,
        }
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Is this file a Sibling candidate: with a supported extension, and not hidden (§3.2)?
fn is_candidate(entry: &DirEntry, name: &str, path: &Path) -> bool {
    lecoq_codec::has_read_extension(path) && !is_hidden(entry, name)
}

/// [`is_candidate`] for a file known only by its path (one the directory watcher reports).
fn is_candidate_at(name: &str, path: &Path) -> bool {
    lecoq_codec::has_read_extension(path) && !is_hidden_at(path, name)
}

/// [`is_hidden`] for a file known only by its path: on Windows this costs a stat.
#[cfg(windows)]
fn is_hidden_at(path: &Path, _name: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    std::fs::metadata(path).is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(windows))]
fn is_hidden_at(_path: &Path, name: &str) -> bool {
    name.starts_with('.')
}

/// Hidden as Explorer sees it: the hidden attribute (read from the directory listing, no extra
/// call). A leading `.` doesn't hide a file on Windows.
#[cfg(windows)]
fn is_hidden(entry: &DirEntry, _name: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    entry
        .metadata()
        .is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

/// Hidden as the file managers see it: a leading `.`.
#[cfg(not(windows))]
fn is_hidden(_entry: &DirEntry, name: &str) -> bool {
    name.starts_with('.')
}

/// Scan `dir` for Siblings and sort them with `order`.
pub fn scan(dir: &Path, order: Arc<dyn NameOrder>) -> io::Result<SiblingList> {
    let mut found: Vec<Sibling> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let name = display_name(&path);
        if !is_candidate(&entry, &name, &path) {
            continue;
        }
        // The file type comes from the directory listing itself; only symlinks cost a stat.
        let is_file = match entry.file_type() {
            Ok(t) if t.is_symlink() => path.is_file(),
            Ok(t) => t.is_file(),
            Err(_) => false,
        };
        if is_file {
            found.push(Sibling { path, name });
        }
    }
    let names: Vec<String> = found.iter().map(|s| s.name.clone()).collect();
    let mut taken: Vec<Option<Sibling>> = found.into_iter().map(Some).collect();
    let siblings = order
        .sorted_indices(&names)
        .into_iter()
        .filter_map(|i| taken[i].take())
        .collect();
    let mut list = SiblingList {
        dir: dir.to_owned(),
        siblings,
        index: HashMap::new(),
        order,
    };
    list.reindex();
    Ok(list)
}

/// What a [`PendingScan`] brings back.
pub struct Scanned {
    pub list: io::Result<SiblingList>,
    /// The directory watcher, when one was asked for and could be started. It was started before
    /// the directory was read, so no change falls between the scan and the watch.
    pub watcher: Option<DirWatcher>,
}

/// A scan on the directory thread (§7.3), polled by the UI.
pub struct PendingScan {
    dir: PathBuf,
    slot: Arc<Mutex<Option<Scanned>>>,
}

impl PendingScan {
    /// Scan `dir` on a new thread; `order` is built on that thread (opening a locale isn't free).
    /// With `watch`, the thread first starts a [`DirWatcher`] on `dir`. `wake` is called when the
    /// result is ready, and by the watcher whenever it has changes.
    pub fn spawn(
        dir: PathBuf,
        watch: bool,
        order: impl FnOnce() -> Box<dyn NameOrder> + Send + 'static,
        wake: impl Fn() + Clone + Send + 'static,
    ) -> Self {
        let slot = Arc::new(Mutex::new(None));
        let thread_slot = Arc::clone(&slot);
        let thread_dir = dir.clone();
        std::thread::Builder::new()
            .name("directory".into())
            .spawn(move || {
                // A directory that can't be watched (no inotify watches left, say) is still
                // scanned; the list just doesn't follow changes.
                let watcher = watch
                    .then(|| DirWatcher::start(&thread_dir, wake.clone()).ok())
                    .flatten();
                let list = scan(&thread_dir, Arc::from(order()));
                *thread_slot.lock().unwrap() = Some(Scanned { list, watcher });
                wake();
            })
            .expect("spawn directory thread");
        Self { dir, slot }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The result, once the scan has finished.
    pub fn take(&self) -> Option<Scanned> {
        self.slot.lock().unwrap().take()
    }
}
