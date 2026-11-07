//! Prefetch and the display-size cache (§3.3).
//!
//! The app says which Image it wants on screen and which Siblings make up the prefetch window
//! ([`Prefetcher::want`]). A pool of decode threads works through them, the wanted Image first.
//! Queued decodes that leave the window are cancelled before they start; results that arrive after
//! their file has left the window are dropped. The cache holds about [`CACHE_CAP_BYTES`] of pixels
//! and evicts the least recently used entries first, never the current Image. Pixels the app holds
//! outside the cache under the same budget (an animation's frames, [`Prefetcher::hold`]) count
//! against the cap too.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

use lecoq_codec::{DecodeError, DisplayImage, Size};

/// A display-size decode, shareable between the cache and the screen.
pub type Decoded = Result<Arc<DisplayImage>, Arc<DecodeError>>;

/// The cache cap (§1.2, §3.3): about 256 MB of display-size pixels, including those the app
/// holds outside the cache ([`Prefetcher::hold`]).
pub const CACHE_CAP_BYTES: usize = 256 * 1024 * 1024;

/// The decode pool size (§3.3, §7.3): `min(4, cores ÷ 2)`, at least 1.
pub fn pool_size() -> usize {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    (cores / 2).clamp(1, 4)
}

struct Entry {
    decoded: Decoded,
    display_box: Size,
    bytes: usize,
    /// When this entry was last shown or added; the smallest is evicted first.
    used: u64,
}

#[derive(Default)]
struct State {
    /// Waiting decodes, highest priority first. Only ever holds wanted files.
    queue: VecDeque<PathBuf>,
    running: HashSet<PathBuf>,
    /// The current Image plus the prefetch window.
    wanted: HashSet<PathBuf>,
    current: Option<PathBuf>,
    display_box: Option<Size>,
    cache: HashMap<PathBuf, Entry>,
    bytes: usize,
    /// Bytes held outside the cache under the same cap ([`Prefetcher::hold`]).
    held: usize,
    tick: u64,
    shutdown: bool,
}

impl State {
    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    fn insert(&mut self, path: PathBuf, decoded: Decoded, display_box: Size, cap: usize) {
        let bytes = decoded.as_ref().map_or(0, |image| image.pixels.byte_len());
        let used = self.next_tick();
        if let Some(old) = self.cache.insert(
            path,
            Entry {
                decoded,
                display_box,
                bytes,
                used,
            },
        ) {
            self.bytes -= old.bytes;
        }
        self.bytes += bytes;
        self.evict(cap);
    }

    fn remove(&mut self, path: &Path) {
        if let Some(old) = self.cache.remove(path) {
            self.bytes -= old.bytes;
        }
    }

    /// Evict least-recently-used entries until they and the held bytes are under `cap`, never the
    /// current Image.
    fn evict(&mut self, cap: usize) {
        while self.bytes + self.held > cap {
            let victim = self
                .cache
                .iter()
                .filter(|(path, _)| Some(*path) != self.current.as_ref())
                .min_by_key(|(_, entry)| entry.used)
                .map(|(path, _)| path.clone());
            match victim {
                Some(path) => self.remove(&path),
                None => break,
            }
        }
    }

    /// The cached decode of `path` at the current display box, marked as used.
    fn lookup(&mut self, path: &Path) -> Option<Decoded> {
        let tick = self.next_tick();
        let display_box = self.display_box;
        let entry = self.cache.get_mut(path)?;
        if Some(entry.display_box) != display_box {
            return None;
        }
        entry.used = tick;
        Some(entry.decoded.clone())
    }
}

struct Shared {
    state: Mutex<State>,
    work: Condvar,
    wake: Box<dyn Fn() + Send + Sync>,
    cap: usize,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }
}

/// The decode pool and the display-size cache.
pub struct Prefetcher {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl Prefetcher {
    /// Start `threads` decode threads with a cache of `cap` bytes. `wake` is called (from a decode
    /// thread) whenever a decode finishes.
    pub fn new(threads: usize, cap: usize, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            work: Condvar::new(),
            wake: Box::new(wake),
            cap,
        });
        let workers = (0..threads.max(1))
            .map(|i| {
                let shared = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name(format!("decode-pool-{i}"))
                    .spawn(move || worker(&shared))
                    .expect("spawn decode pool thread")
            })
            .collect();
        Self { shared, workers }
    }

    /// Move to `current`: make it the Image wanted on screen and `window` the Siblings to keep
    /// ready, all decoded to fit `display_box`. Returns `current`'s decode if it's cached;
    /// otherwise it's decoded at the highest priority, ahead of the window. Queued decodes outside
    /// the new window are cancelled.
    pub fn show(&self, current: &Path, window: &[PathBuf], display_box: Size) -> Option<Decoded> {
        self.want(current, true, window, display_box)
    }

    /// Like [`Self::show`], for a `current` that is already on screen from elsewhere (the Image
    /// opened by path): only the window is decoded.
    pub fn keep(&self, current: &Path, window: &[PathBuf], display_box: Size) {
        self.want(current, false, window, display_box);
    }

    fn want(
        &self,
        current: &Path,
        decode_current: bool,
        window: &[PathBuf],
        display_box: Size,
    ) -> Option<Decoded> {
        let mut state = self.shared.lock();
        if state.display_box != Some(display_box) {
            // Decodes for another window size are no use any more.
            state.display_box = Some(display_box);
            let stale: Vec<PathBuf> = state
                .cache
                .iter()
                .filter(|(_, entry)| entry.display_box != display_box)
                .map(|(path, _)| path.clone())
                .collect();
            for path in stale {
                state.remove(&path);
            }
        }
        state.current = Some(current.to_owned());
        state.wanted = std::iter::once(current)
            .chain(window.iter().map(PathBuf::as_path))
            .map(Path::to_owned)
            .collect();
        state.queue.clear();
        let first = decode_current.then_some(current);
        for path in first.into_iter().chain(window.iter().map(PathBuf::as_path)) {
            if !state.cache.contains_key(path)
                && !state.running.contains(path)
                && !state.queue.iter().any(|p| p == path)
            {
                state.queue.push_back(path.to_owned());
            }
        }
        let cached = state.lookup(current);
        state.evict(self.shared.cap);
        drop(state);
        self.shared.work.notify_all();
        cached
    }

    /// Count `bytes` the app holds outside the cache (an animation's frames, §3.4) against the
    /// cap, evicting what no longer fits. Replaces the previous amount.
    pub fn hold(&self, bytes: usize) {
        let mut state = self.shared.lock();
        state.held = bytes;
        state.evict(self.shared.cap);
    }

    /// The cached decode of `path`, if it has arrived.
    pub fn get(&self, path: &Path) -> Option<Decoded> {
        self.shared.lock().lookup(path)
    }

    /// Nothing queued or decoding.
    pub fn is_idle(&self) -> bool {
        let state = self.shared.lock();
        state.queue.is_empty() && state.running.is_empty()
    }

    /// The files in the cache, and their total size in bytes (for diagnostics and tests).
    pub fn cached(&self) -> (Vec<PathBuf>, usize) {
        let state = self.shared.lock();
        (state.cache.keys().cloned().collect(), state.bytes)
    }
}

impl Drop for Prefetcher {
    fn drop(&mut self) {
        self.shared.lock().shutdown = true;
        self.shared.work.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn worker(shared: &Shared) {
    loop {
        let (path, display_box) = {
            let mut state = shared.lock();
            loop {
                if state.shutdown {
                    return;
                }
                if let Some(path) = state.queue.pop_front() {
                    let display_box = state.display_box.expect("set by want() before queueing");
                    state.running.insert(path.clone());
                    break (path, display_box);
                }
                state = shared.work.wait(state).unwrap();
            }
        };

        let decoded: Decoded = lecoq_codec::decode_for_display(&path, display_box)
            .map(Arc::new)
            .map_err(Arc::new);

        let mut state = shared.lock();
        state.running.remove(&path);
        if state.wanted.contains(&path) {
            if state.display_box == Some(display_box) {
                state.insert(path, decoded, display_box, shared.cap);
            } else {
                // The window was resized meanwhile: decode again, first.
                state.queue.push_front(path);
                shared.work.notify_one();
            }
        }
        // Otherwise the file left the window while decoding: drop the result.
        drop(state);
        (shared.wake)();
    }
}
