---
Status: accepted
---

# Background work runs through one Job slot, and the worker wakes the UI itself

The app ran the same background-job choreography six times (the display decode, the zoom upgrade,
an incoming paste, a running save, a running copy and the editor's full decode): each an optional
pending job with its own per-frame poll that woke the repaint waker, took the result, unwrapped
"checked above" and cleared, plus its own term in `is_busy`. Three native dialogs each held a
channel receiver and repeated the same `try_recv` match. We replaced the job half with one deep
module, `Job<T>` (spawn with a waker, `poll` for the result once, `is_running`), and the dialog
half with `DialogSlot` (open, `answer`); `is_busy` is now a fold over the slots.

Crucially, `Job` is spawned with its waker in hand, and the worker thread requests the repaint
itself when it finishes. The old per-frame `wake` calls existed only because some spawn sites had
no UI context when they spawned, so every poll re-registered the waker each frame, taking a mutex
per job per frame. Giving the three context-less sites (the open, the first-image decode and the
editor's full decode) a context at spawn lets those per-frame calls go. The first-image decode is
the one job spawned before the window exists: it starts without a waker and is handed the UI's
repaint when the app is created, and it is also the one job that keeps a timed `wait` for the
startup first frame (§1.3).

Decided while cleaning up the app module (issue 65, a follow-up to the performance review's
finding 11).

## Considered Options

- **Leave the six sites as they were.** Rejected: the app module, whose own header says it should
  be thin, carried ~150 lines of duplicated wake/take/clear and three copies of the dialog
  `try_recv` match, and a seventh background task would have had to copy the pattern again and be
  remembered in `is_busy` by hand.
- **Keep re-registering the waker each frame.** Rejected: it takes a mutex per job per frame for a
  waker that never changes, and the only reason it was there — spawn sites without a context — went
  away once those sites were given one.
- **A general async runtime (tokio or similar).** Rejected: disproportionate for a handful of
  one-shot jobs on their own threads, and a larger dependency and threading-model change than the
  cleanup warranted (the worker-per-save threading model is explicitly out of scope).

## Consequences

- A new background task declares a `Job` slot and polls it; it writes no wake/take/clear code, and
  `is_busy`'s fold picks it up through `Job::is_running`.
- The worker thread calls the waker, so `Job` holds the waker behind its mutex; `set_waker`
  registers one after the fact for the first-image decode, waking at once if the result already
  landed.
- The editor is handed the UI context once, when the app is created, so its full-resolution decode
  wakes the UI without the editor polling on a timer.
- The font listing (finding 12) still polls on a 30 ms timer; moving it to this pattern is left to
  issue 67, together with folding the startup timed `wait` into `Job`.
