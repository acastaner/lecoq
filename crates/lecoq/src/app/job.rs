//! A unit of work on its own thread, polled by the UI each frame ([`Job`]).
//!
//! One slot behind a small interface stands in for the background-job choreography the app used to
//! spell out at every site (a decode, the zoom upgrade, an incoming paste, a running save, a
//! running copy, the editor's full decode): spawn it with a waker, poll it for the result once,
//! ask whether it is still running. The worker requests the repaint itself when it finishes, so
//! the UI never re-registers a waker per frame.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Called from the worker thread when the job finishes, to wake the UI. Usually an
/// `egui::Context`'s repaint (see [`waker`]), but any `Fn()` will do.
pub(crate) type Waker = Box<dyn Fn() + Send>;

/// A [`Waker`] that requests a repaint of `ctx`.
pub(crate) fn waker(ctx: &egui::Context) -> Waker {
    let ctx = ctx.clone();
    Box::new(move || ctx.request_repaint())
}

struct Slot<T> {
    /// The worker's result, once it has finished and until [`Job::poll`] takes it.
    result: Option<T>,
    /// The result has been handed to the UI (so the job is no longer running).
    taken: bool,
    /// Called once when the result arrives; set at spawn, or later for the first-image decode.
    waker: Option<Waker>,
}

struct Shared<T> {
    slot: Mutex<Slot<T>>,
    /// Lets the startup path block for the first decode within a budget ([`Job::wait`]).
    done: Condvar,
}

/// Work in flight on its own thread.
pub(crate) struct Job<T> {
    shared: Arc<Shared<T>>,
}

impl<T: Send + 'static> Job<T> {
    /// Run `work` on a new thread named `thread_name`, waking `waker` (if any) when it finishes.
    pub fn spawn(
        thread_name: &str,
        waker: Option<Waker>,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot {
                result: None,
                taken: false,
                waker,
            }),
            done: Condvar::new(),
        });
        let thread_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name(thread_name.into())
            .spawn(move || {
                let result = work();
                let mut slot = thread_shared.slot.lock().unwrap();
                slot.result = Some(result);
                if let Some(waker) = &slot.waker {
                    waker();
                }
                thread_shared.done.notify_all();
            })
            .expect("spawn job thread");
        Self { shared }
    }

    /// Register a waker after the job was spawned: the first-image decode starts before the UI
    /// exists (§1.3), and is given the UI's repaint once the app is created. Wakes at once if the
    /// result has already arrived.
    pub fn set_waker(&self, waker: Waker) {
        let mut slot = self.shared.slot.lock().unwrap();
        if slot.result.is_some() {
            waker();
        } else {
            slot.waker = Some(waker);
        }
    }

    /// The result, once the work has finished, handed over exactly once; `None` until then and
    /// after.
    pub fn poll(&mut self) -> Option<T> {
        let mut slot = self.shared.slot.lock().unwrap();
        let result = slot.result.take();
        if result.is_some() {
            slot.taken = true;
        }
        result
    }

    /// Is the job still outstanding (spawned, its result not yet handed over by [`Self::poll`])?
    pub fn is_running(&self) -> bool {
        !self.shared.slot.lock().unwrap().taken
    }

    /// Block until the work has finished or `timeout` has passed; returns whether it finished. The
    /// startup path uses it on the first-image decode so the first frame is the image (§1.3).
    pub fn wait(&self, timeout: Duration) -> bool {
        let slot = self.shared.slot.lock().unwrap();
        let (slot, _) = self
            .shared
            .done
            .wait_timeout_while(slot, timeout, |slot| slot.result.is_none())
            .unwrap();
        slot.result.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    #[test]
    fn result_lands_once_then_slot_is_empty() {
        let mut job = Job::spawn("test", None, || 42);
        assert!(job.wait(Duration::from_secs(1)));
        assert_eq!(job.poll(), Some(42));
        assert_eq!(job.poll(), None);
    }

    #[test]
    fn running_until_the_result_is_taken() {
        let mut job = Job::spawn("test", None, || 7);
        // Still running the moment it is spawned, before anything is polled.
        assert!(job.is_running());
        assert!(job.wait(Duration::from_secs(1)));
        // The result has arrived but not been handed over: still running.
        assert!(job.is_running());
        assert_eq!(job.poll(), Some(7));
        // Handed over once: no longer running.
        assert!(!job.is_running());
    }

    #[test]
    fn the_waker_fires_exactly_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let mut job = Job::spawn(
            "test",
            Some(Box::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })),
            || 1,
        );
        assert!(job.wait(Duration::from_secs(1)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Polling the result does not wake again.
        assert_eq!(job.poll(), Some(1));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_waker_set_after_the_result_arrived_wakes_at_once() {
        let (tx, rx) = mpsc::channel();
        let job = Job::spawn("test", None, || 5);
        assert!(job.wait(Duration::from_secs(1)));
        job.set_waker(Box::new(move || {
            let _ = tx.send(());
        }));
        // The result was already in: the waker is called immediately on registration.
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn wait_times_out_while_the_work_is_still_running() {
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let job = Job::spawn("test", None, move || {
            let _ = release_rx.recv();
            9
        });
        assert!(!job.wait(Duration::from_millis(10)));
        // Let the worker finish so its thread does not linger.
        let _ = release_tx.send(());
        assert!(job.wait(Duration::from_secs(1)));
    }
}
