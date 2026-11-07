//! Decodes running on their own thread, polled by the UI: the display-size decode of an Image
//! ([`PendingDecode`]) and the full-resolution decode when editing starts ([`PendingFullDecode`]).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use lecoq_codec::{DecodeError, DisplayImage, Size};
use lecoq_core::Pixels;

pub type DecodeResult = Result<DisplayImage, DecodeError>;
pub type FullDecodeResult = Result<Pixels, DecodeError>;

/// A display-size decode (§5.2).
pub type PendingDecode = Pending<DecodeResult>;
/// A full-resolution decode (§4.7).
pub type PendingFullDecode = Pending<FullDecodeResult>;

struct Slot<T> {
    result: Option<T>,
    /// Woken when the result arrives, once the UI has handed over its context.
    waker: Option<egui::Context>,
}

struct Shared<T> {
    slot: Mutex<Slot<T>>,
    done: Condvar,
}

/// Work on a file in flight. The UI polls it with [`Pending::take`] each frame.
pub struct Pending<T> {
    path: PathBuf,
    shared: Arc<Shared<T>>,
}

impl<T: Send + 'static> Pending<T> {
    /// Run `job` on `path` on a new thread.
    pub(crate) fn spawn_job(
        path: PathBuf,
        thread_name: &str,
        job: impl FnOnce(&Path) -> T + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot {
                result: None,
                waker: None,
            }),
            done: Condvar::new(),
        });
        let thread_shared = Arc::clone(&shared);
        let thread_path = path.clone();
        std::thread::Builder::new()
            .name(thread_name.into())
            .spawn(move || {
                let result = job(&thread_path);
                let mut slot = thread_shared.slot.lock().unwrap();
                slot.result = Some(result);
                if let Some(ctx) = &slot.waker {
                    ctx.request_repaint();
                }
                thread_shared.done.notify_all();
            })
            .expect("spawn decode thread");
        Self { path, shared }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Ask for a repaint of `ctx` when the result arrives (immediately, if it already has).
    pub fn wake(&self, ctx: &egui::Context) {
        let mut slot = self.shared.slot.lock().unwrap();
        if slot.result.is_some() {
            ctx.request_repaint();
        } else if slot.waker.is_none() {
            slot.waker = Some(ctx.clone());
        }
    }

    /// Block until the work has finished or `timeout` has passed. Returns whether it finished.
    pub fn wait(&self, timeout: Duration) -> bool {
        let slot = self.shared.slot.lock().unwrap();
        let (slot, _) = self
            .shared
            .done
            .wait_timeout_while(slot, timeout, |slot| slot.result.is_none())
            .unwrap();
        slot.result.is_some()
    }

    /// The result, once the work has finished.
    pub fn take(&self) -> Option<T> {
        self.shared.slot.lock().unwrap().result.take()
    }
}

impl PendingDecode {
    /// Start decoding `path` for display. `display_box` is called only after the full decode, so
    /// the startup path can start decoding before it knows the window size (§1.3).
    pub fn spawn(path: PathBuf, display_box: impl FnOnce() -> Size + Send + 'static) -> Self {
        Self::spawn_job(path, "decode", move |path| {
            let result = lecoq_codec::decode(path).map(|d| d.into_display(display_box()));
            static FIRST: AtomicBool = AtomicBool::new(false);
            crate::bench::mark_once("decoded", &FIRST);
            result
        })
    }
}

impl PendingFullDecode {
    /// Start decoding `path` at full resolution.
    pub fn spawn_full(path: PathBuf) -> Self {
        Self::spawn_job(path, "full-decode", decode_full)
    }
}

/// Decode `path` at full resolution: oriented and colour-converted like the display decode, but
/// not reduced.
pub(crate) fn decode_full(path: &Path) -> FullDecodeResult {
    // Full-resolution pixels stay straight (not reduced, not premultiplied): the editor and the
    // save encoders work from them (§5.3, §7.2). The display path premultiplies; this one doesn't.
    lecoq_codec::decode(path).map(lecoq_codec::Decoded::into_full)
}
