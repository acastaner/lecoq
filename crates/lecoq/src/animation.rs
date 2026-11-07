//! Playing an animated GIF or WebP (§3.4).
//!
//! The frames are decoded on their own thread, composited by the decoder and reduced to the display
//! size, and stream in while the animation already plays. They are kept as CPU pixels; the shown
//! Image's display texture is rewritten when the frame changes, so the GPU holds one frame.
//!
//! Dropping an [`Animation`] (a new Image replaces the shown one) stops its decode thread and frees
//! its frames.
//!
//! Memory (§1.2): the frames share the display-size cache's budget. An animation decodes only the
//! frames that fit the budget it is given, and loops over those; the app counts what it holds
//! against the prefetch cache's cap, which evicts Siblings to make room.
//!
//! Freezing on the first edit (#38, `ShownImage::stop_animation`): drop the shown Image's
//! `Animation`, put [`Animation::first_frame`]'s pixels back on screen, and show the frozen state in
//! the status bar.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use lecoq_codec::Size;

/// After a stall longer than this (the window was hidden, the machine slept), the animation goes
/// on from the current frame instead of racing through the ones it missed.
const MAX_CATCH_UP: f64 = 1.0;

enum Message {
    Frame(Arc<lecoq_core::Pixels>, Duration),
    /// No more frames: the end of the file, an error, or the budget.
    Done,
}

/// The frames of the shown Image, and which one is on screen.
pub struct Animation {
    rx: mpsc::Receiver<Message>,
    cancel: Arc<AtomicBool>,
    frames: Vec<(Arc<lecoq_core::Pixels>, Duration)>,
    /// The bytes of `frames`' pixels.
    bytes: usize,
    decoding: bool,
    current: usize,
    /// When the current frame went on screen (egui time, seconds), once the first frame is in.
    since: Option<f64>,
}

impl Animation {
    /// Start decoding the frames of `path`, reduced to fit `max` physical pixels. Frames past
    /// `budget` bytes of pixels aren't decoded: the animation loops over those it has.
    pub fn spawn(path: PathBuf, max: Size, budget: usize, ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel);
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("animation".into())
            .spawn(move || {
                decode(&path, max, budget, &tx, &thread_cancel, &ctx);
                let _ = tx.send(Message::Done);
                ctx.request_repaint();
            })
            .expect("spawn animation thread");
        Self {
            rx,
            cancel,
            frames: Vec::new(),
            bytes: 0,
            decoding: true,
            current: 0,
            since: None,
        }
    }

    /// Are frames still being decoded?
    pub fn is_decoding(&self) -> bool {
        self.decoding
    }

    /// Is this an animation (more than one frame)? Unknown until the second frame is in.
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// Is it known not to be an animation (one frame, or none decoded)?
    pub fn is_still(&self) -> bool {
        !self.decoding && !self.is_animated()
    }

    /// How many frames have been decoded so far.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// The bytes of pixels the decoded frames hold.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Frame 1's pixels, once decoded.
    pub fn first_frame(&self) -> Option<&Arc<lecoq_core::Pixels>> {
        self.frames.first().map(|(pixels, _)| pixels)
    }

    /// Take in the frames decoded since the last call, without moving on.
    pub fn receive(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                Message::Frame(pixels, delay) => {
                    self.bytes += frame_bytes(&pixels);
                    self.frames.push((pixels, delay));
                }
                Message::Done => self.decoding = false,
            }
        }
    }

    /// Take in the frames decoded since the last call and move on to the frame due at `now` (egui
    /// time, seconds). Returns the frame to put on screen when it changed, and asks `ctx` for a
    /// repaint when the next one is due.
    pub fn tick(&mut self, now: f64, ctx: &egui::Context) -> Option<Arc<lecoq_core::Pixels>> {
        self.receive();
        if self.frames.is_empty() {
            return None;
        }
        // The first frame is already on screen: the display-size decode of the file.
        let animated = self.is_animated();
        let since = self.since.get_or_insert(now);
        if !animated {
            return None;
        }
        if now - *since > MAX_CATCH_UP {
            *since = now;
        }
        let before = self.current;
        loop {
            let delay = self.frames[self.current].1.as_secs_f64();
            let next = self.current + 1;
            let next = if next < self.frames.len() {
                next
            } else if self.decoding {
                // Not decoded yet: stay on this frame until it is.
                break;
            } else {
                0
            };
            if now < *since + delay {
                ctx.request_repaint_after(Duration::from_secs_f64(*since + delay - now));
                break;
            }
            *since += delay;
            self.current = next;
        }
        (self.current != before).then(|| Arc::clone(&self.frames[self.current].0))
    }
}

impl Drop for Animation {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn decode(
    path: &std::path::Path,
    max: Size,
    budget: usize,
    tx: &mpsc::Sender<Message>,
    cancel: &AtomicBool,
    ctx: &egui::Context,
) {
    let Ok(Some(frames)) = lecoq_codec::frames(path, max) else {
        return;
    };
    let mut bytes = 0;
    for frame in frames {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        // A broken frame ends the animation; the frames before it still play.
        let Ok(frame) = frame else {
            return;
        };
        let pixels = crate::app::display_pixels(&frame.image);
        bytes += frame_bytes(&pixels);
        if bytes > budget
            || tx
                .send(Message::Frame(Arc::new(pixels), frame.delay))
                .is_err()
        {
            return;
        }
        ctx.request_repaint();
    }
}

/// The bytes of pixels a frame holds (premultiplied RGBA).
pub fn frame_bytes(pixels: &lecoq_core::Pixels) -> usize {
    pixels.byte_len()
}
