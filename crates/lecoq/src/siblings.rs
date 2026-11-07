//! Sibling navigation (§3.1–3.3): the scan, the position in the list, moving to the next or
//! previous Sibling, the prefetch window, and following changes to the folder (the directory
//! watcher). The app owns one [`Navigation`].

use std::path::{Path, PathBuf};

use lecoq_codec::Size;
use lecoq_siblings::order::NameOrder;
use lecoq_siblings::{
    Change, Decoded, DirWatcher, Direction, PendingScan, Prefetcher, SiblingList, prefetch,
    prefetch_window,
};

enum Scan {
    NotStarted,
    Running(PendingScan),
    /// `None` when the directory couldn't be read.
    Done(Option<SiblingList>),
}

/// An absolute jump through the Siblings: to the first or the last (Home / End, §3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jump {
    First,
    Last,
}

/// What a move to the next or previous Sibling did.
pub enum Step {
    /// The Sibling was cached: show it now.
    Show(PathBuf, Decoded),
    /// The Sibling is being decoded; the previous Image stays on screen until [`Navigation::poll`]
    /// hands it over.
    Pending,
    /// Already at the end of the list: show this message.
    End(&'static str),
    /// Nothing to show: the scan hasn't finished, or a jump would stay put (already at that end,
    /// or a single-image folder).
    NotReady,
}

pub struct Navigation {
    scan: Scan,
    prefetch: Option<Prefetcher>,
    cache_cap: usize,
    /// The ordering rule every scan of the folder uses (§3.2).
    order: fn() -> Box<dyn NameOrder>,
    /// Bytes held outside the cache under its cap (the shown animation, §3.4).
    held: usize,
    direction: Direction,
    /// The Sibling being moved to while its decode is pending.
    target: Option<PathBuf>,
    /// Watches the directory once the scan has finished (§3.2).
    watcher: Option<DirWatcher>,
    /// A full re-scan after the watcher lost events. The old list stays in use until it finishes.
    rescan: Option<PendingScan>,
    /// Files trashed from the folder being scanned: left out of every scan of it (which may
    /// predate the trashing), and their removal isn't a deletion from outside (§3.2).
    trashed: Vec<PathBuf>,
}

impl Navigation {
    pub fn new(cache_cap: usize, order: fn() -> Box<dyn NameOrder>) -> Self {
        Self {
            scan: Scan::NotStarted,
            prefetch: None,
            cache_cap,
            order,
            held: 0,
            direction: Direction::Forward,
            target: None,
            watcher: None,
            rescan: None,
            trashed: Vec::new(),
        }
    }

    /// Scan and watch the directory of `shown`, the Image now on screen, and start the decode pool
    /// if it isn't running yet. Only ever called after the first image is painted (§1.3), or by
    /// [`Self::ensure_started`].
    pub fn start(&mut self, ctx: &egui::Context, shown: &Path) {
        let dir = folder_of(shown);
        let waker = ctx.clone();
        // The old directory's watcher stops here.
        self.watcher = None;
        self.rescan = None;
        self.trashed.retain(|path| folder_of(path) == dir);
        self.scan = Scan::Running(PendingScan::spawn(dir, true, self.order, move || {
            waker.request_repaint()
        }));
        if self.prefetch.is_none() {
            let waker = ctx.clone();
            self.prefetch = Some(Prefetcher::new(
                prefetch::pool_size(),
                self.cache_cap,
                move || waker.request_repaint(),
            ));
            if let Some(prefetch) = &self.prefetch {
                prefetch.hold(self.held);
            }
        }
        self.target = None;
        self.direction = Direction::Forward;
    }

    /// Forget a pending move (another file is being opened).
    pub fn cancel(&mut self) {
        self.target = None;
    }

    /// Collect background results: a finished scan starts the prefetch around `shown`; a pending
    /// move's decode is returned once it has arrived.
    pub fn poll(&mut self, shown: Option<&Path>, display_box: Size) -> Option<(PathBuf, Decoded)> {
        if let Scan::Running(pending) = &self.scan
            && let Some(scanned) = pending.take()
        {
            crate::bench::mark("siblings_scanned");
            let mut list = scanned.list.ok();
            if let Some(list) = &mut list {
                for path in &self.trashed {
                    list.remove(path);
                }
            }
            self.scan = Scan::Done(list);
            self.watcher = scanned.watcher;
            if let Some(shown) = shown {
                self.keep_window(shown, display_box);
            }
        }
        let target = self.target.as_ref()?;
        let decoded = self.prefetch.as_ref()?.get(target)?;
        let target = self.target.take()?;
        Some((target, decoded))
    }

    /// Follow the changes the directory watcher has seen since the last frame: the Sibling list
    /// gains and loses files, the current Image keeps its place, and the prefetch window follows.
    /// Returns whether `shown`, the file on screen, was deleted from outside lecoq (§3.2).
    pub fn follow_changes(
        &mut self,
        ctx: &egui::Context,
        shown: Option<&Path>,
        display_box: Size,
    ) -> bool {
        let mut shown_deleted = false;
        let mut changed = false;
        if let Some(rescan) = &self.rescan {
            let Some(scanned) = rescan.take() else {
                // Changes wait in the watcher's queue until the re-scan is in.
                return false;
            };
            self.rescan = None;
            if let Ok(mut list) = scanned.list {
                for path in &self.trashed {
                    list.remove(path);
                }
                self.scan = Scan::Done(Some(list));
                changed = true;
            }
            // Which files went is unknown after lost events: look at the current one.
            shown_deleted =
                shown.is_some_and(|p| !p.exists() && !self.trashed.iter().any(|t| t == p));
        }
        let (Some(watcher), Scan::Done(Some(list))) = (&self.watcher, &mut self.scan) else {
            return shown_deleted;
        };
        for change in watcher.drain() {
            match &change {
                Change::Rescan => {
                    let waker = ctx.clone();
                    self.rescan = Some(PendingScan::spawn(
                        list.dir().to_owned(),
                        false,
                        self.order,
                        move || waker.request_repaint(),
                    ));
                    // The re-scan reads the directory after these changes: drop the rest.
                    break;
                }
                // A trashed file stays out of the list, and its removal was lecoq's own.
                Change::Added(path) | Change::Removed(path) if self.trashed.contains(path) => {
                    continue;
                }
                Change::Removed(path) if shown == Some(path.as_path()) => shown_deleted = true,
                Change::Added(_) | Change::Removed(_) => {}
            }
            changed |= list.apply(&change);
        }
        if changed && let Some(shown) = shown {
            self.keep_window(shown, display_box);
        }
        shown_deleted
    }

    /// Is the directory being watched (§3.2)?
    pub fn is_watching(&self) -> bool {
        self.watcher.is_some()
    }

    /// Would [`Self::go`] move to another Sibling (rather than stop at an end of the list, or wait
    /// for the scan)? The unsaved-changes prompt (#35) only asks when it would.
    pub fn would_move(&self, shown: &Path, direction: Direction) -> bool {
        let (Scan::Done(Some(list)), Some(_)) = (&self.scan, &self.prefetch) else {
            return false;
        };
        let from = self.target.as_deref().unwrap_or(shown);
        sibling_towards(list, from, direction).is_some()
    }

    /// Would [`Self::jump`] move to another Sibling (rather than stay put at that end, or wait for
    /// the scan)? Like [`Self::would_move`], the unsaved-changes prompt (#35) only asks when it
    /// would.
    pub fn would_jump(&self, shown: &Path, jump: Jump) -> bool {
        let (Scan::Done(Some(list)), Some(_)) = (&self.scan, &self.prefetch) else {
            return false;
        };
        let from = self.target.as_deref().unwrap_or(shown);
        jump_target(list, from, jump).is_some()
    }

    /// Move one Sibling forward or back from the Image on screen (or from the pending target).
    pub fn go(&mut self, shown: &Path, direction: Direction, display_box: Size) -> Step {
        let (Scan::Done(Some(list)), Some(_)) = (&self.scan, &self.prefetch) else {
            return Step::NotReady;
        };
        let from = self.target.as_deref().unwrap_or(shown);
        let Some(index) = sibling_towards(list, from, direction) else {
            return Step::End(match direction {
                Direction::Forward => crate::strings::LAST_IMAGE,
                Direction::Backward => crate::strings::FIRST_IMAGE,
            });
        };
        self.step_to(index, direction, display_box)
    }

    /// Jump to the first or last Sibling from the Image on screen (or the pending target). The
    /// prefetch then faces the way browsing naturally continues: forward from the first, back from
    /// the last. A no-op ([`Step::NotReady`]) when already at that end, or in a single-image folder.
    pub fn jump(&mut self, shown: &Path, jump: Jump, display_box: Size) -> Step {
        let (Scan::Done(Some(list)), Some(_)) = (&self.scan, &self.prefetch) else {
            return Step::NotReady;
        };
        let from = self.target.as_deref().unwrap_or(shown);
        let Some(index) = jump_target(list, from, jump) else {
            return Step::NotReady;
        };
        let direction = match jump {
            Jump::First => Direction::Forward,
            Jump::Last => Direction::Backward,
        };
        self.step_to(index, direction, display_box)
    }

    /// Move to `index`, facing `direction` for the prefetch window: show it now if it is cached,
    /// else start its decode and leave the previous Image on screen until [`Self::poll`] hands the
    /// decode over. Shared by [`Self::go`] and [`Self::jump`].
    fn step_to(&mut self, index: usize, direction: Direction, display_box: Size) -> Step {
        let (Scan::Done(Some(list)), Some(prefetch)) = (&self.scan, &self.prefetch) else {
            return Step::NotReady;
        };
        self.direction = direction;
        let path = list.get(index).expect("index in range").path.clone();
        let window = window_paths(list, index, direction);
        match prefetch.show(&path, &window, display_box) {
            Some(decoded) => {
                self.target = None;
                Step::Show(path, decoded)
            }
            None => {
                self.target = Some(path);
                Step::Pending
            }
        }
    }

    /// Prefetch around `shown`, which is on screen already.
    fn keep_window(&self, shown: &Path, display_box: Size) {
        let (Scan::Done(Some(list)), Some(prefetch)) = (&self.scan, &self.prefetch) else {
            return;
        };
        let Ok(index) = list.locate(shown) else {
            return;
        };
        prefetch.keep(
            shown,
            &window_paths(list, index, self.direction),
            display_box,
        );
    }

    /// The status-bar position (§2): `n / N`, `? / ?` until the scan has finished, `– / N` for an
    /// Image that isn't one of the Siblings (a hidden file, say).
    pub fn position(&self, shown: &Path) -> String {
        match &self.scan {
            Scan::Done(Some(list)) => match list.locate(shown) {
                Ok(i) => format!("{} / {}", i + 1, list.len()),
                Err(_) => format!("– / {}", list.len()),
            },
            _ => "? / ?".to_owned(),
        }
    }

    /// Take a trashed file out of the Siblings, now and from any scan landing later; the watcher's
    /// report of its removal isn't a deletion from outside. A move from it afterwards goes to the
    /// Sibling that followed it (or preceded it, backward).
    pub fn forget(&mut self, path: &Path) {
        self.target = None;
        if let Scan::Done(Some(list)) = &mut self.scan {
            list.remove(path);
        }
        self.trashed.push(path.to_owned());
    }

    /// Scan the folder of `path` unless a scan has been started already (an Image that failed to
    /// open at launch is never painted, so nothing else starts one).
    pub fn ensure_started(&mut self, ctx: &egui::Context, path: &Path) {
        if matches!(self.scan, Scan::NotStarted) {
            self.start(ctx, path);
        }
    }

    /// Has the scan finished, so that moves work?
    pub fn is_scanned(&self) -> bool {
        matches!(self.scan, Scan::Done(_))
    }

    /// Is a scan, a pending move or a prefetch decode still running?
    pub fn is_busy(&self) -> bool {
        matches!(self.scan, Scan::Running(_))
            || self.rescan.is_some()
            || self.target.is_some()
            || self.prefetch.as_ref().is_some_and(|p| !p.is_idle())
    }

    /// The display-size cache's cap (§1.2): the shown animation shares it.
    pub fn cache_cap(&self) -> usize {
        self.cache_cap
    }

    /// Count `bytes` held outside the cache (the shown animation) against its cap.
    pub fn hold(&mut self, bytes: usize) {
        if bytes == self.held {
            return;
        }
        self.held = bytes;
        if let Some(prefetch) = &self.prefetch {
            prefetch.hold(bytes);
        }
    }

    /// The files in the display-size cache.
    pub fn cached(&self) -> Vec<PathBuf> {
        self.prefetch
            .as_ref()
            .map_or_else(Vec::new, |p| p.cached().0)
    }
}

/// The index of the Sibling after (or before) `from`, which may have left the list: `None` at an
/// end of it.
fn sibling_towards(list: &SiblingList, from: &Path, direction: Direction) -> Option<usize> {
    let len = list.len();
    match (list.locate(from), direction) {
        (Ok(i), Direction::Forward) => Some(i + 1).filter(|&i| i < len),
        (Ok(i), Direction::Backward) => i.checked_sub(1),
        (Err(i), Direction::Forward) => Some(i).filter(|&i| i < len),
        (Err(i), Direction::Backward) => i.checked_sub(1),
    }
}

/// The index a [`Jump`] lands on, or `None` when it would stay put: an empty folder, or `from`
/// already sitting at that end. No wrap-around. `from` may have left the list.
fn jump_target(list: &SiblingList, from: &Path, jump: Jump) -> Option<usize> {
    if list.is_empty() {
        return None;
    }
    let target = match jump {
        Jump::First => 0,
        Jump::Last => list.len() - 1,
    };
    // Already there (a Sibling at `target`): nothing to do. A `from` that isn't a Sibling always
    // moves, since it sits at neither end.
    match list.locate(from) {
        Ok(index) if index == target => None,
        _ => Some(target),
    }
}

fn folder_of(path: &Path) -> PathBuf {
    path.parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_owned()
}

fn window_paths(list: &SiblingList, index: usize, direction: Direction) -> Vec<PathBuf> {
    prefetch_window(list.len(), index, direction)
        .into_iter()
        .filter_map(|i| list.get(i).map(|s| s.path.clone()))
        .collect()
}

/// The jump index maths, over real [`SiblingList`]s (the ordering is irrelevant, so a plain
/// code-point order keeps the tests off any installed locale). [`Navigation::would_jump`] is just
/// `jump_target(..).is_some()`, so these cover it too.
#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::fs::File;
    use std::sync::Arc;

    use lecoq_siblings::order::NameOrder;
    use lecoq_siblings::scan::scan;
    use tempfile::TempDir;

    use super::*;

    struct ByteOrder;
    impl NameOrder for ByteOrder {
        fn compare(&self, a: &str, b: &str) -> Ordering {
            a.cmp(b)
        }
    }

    /// Scan a fresh directory of `names` (empty `.jpg` files) into a [`SiblingList`].
    fn list_of(names: &[&str]) -> (TempDir, SiblingList) {
        let dir = TempDir::new().unwrap();
        for name in names {
            File::create(dir.path().join(name)).unwrap();
        }
        let list = scan(dir.path(), Arc::new(ByteOrder)).unwrap();
        (dir, list)
    }

    #[test]
    fn jump_lands_on_the_first_and_last_from_a_middle_image() {
        let (dir, list) = list_of(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
        let middle = dir.path().join("c.jpg");
        assert_eq!(jump_target(&list, &middle, Jump::First), Some(0));
        assert_eq!(
            jump_target(&list, &middle, Jump::Last),
            Some(list.len() - 1)
        );
    }

    #[test]
    fn jump_stays_put_at_the_matching_end_but_moves_from_the_far_one() {
        let (dir, list) = list_of(&["a.jpg", "b.jpg", "c.jpg"]);
        let (first, last) = (dir.path().join("a.jpg"), dir.path().join("c.jpg"));
        assert_eq!(jump_target(&list, &first, Jump::First), None);
        assert_eq!(jump_target(&list, &last, Jump::Last), None);
        assert_eq!(jump_target(&list, &first, Jump::Last), Some(2));
        assert_eq!(jump_target(&list, &last, Jump::First), Some(0));
    }

    #[test]
    fn a_single_image_folder_never_moves() {
        let (dir, list) = list_of(&["only.jpg"]);
        let only = dir.path().join("only.jpg");
        assert_eq!(jump_target(&list, &only, Jump::First), None);
        assert_eq!(jump_target(&list, &only, Jump::Last), None);
    }

    #[test]
    fn an_empty_folder_never_moves() {
        let (_dir, list) = list_of(&[]);
        let ghost = Path::new("/nowhere/ghost.jpg");
        assert_eq!(jump_target(&list, ghost, Jump::First), None);
        assert_eq!(jump_target(&list, ghost, Jump::Last), None);
    }
}
