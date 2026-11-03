//! Timing marks for `tools/bench` (§1.2).
//!
//! Off unless `LECOQ_BENCH_T0_NS` is set (the bench sets it to the wall-clock time just before it
//! `exec`s lecoq). Then each [`mark`] prints `<name> <ms since t0>` on stdout, and with
//! `LECOQ_BENCH_EXIT=1` the app closes as soon as the first image is painted.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::Command;

fn t0_ns() -> Option<u128> {
    static T0: OnceLock<Option<u128>> = OnceLock::new();
    *T0.get_or_init(|| std::env::var("LECOQ_BENCH_T0_NS").ok()?.parse().ok())
}

fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

/// Print a timing mark, when benchmarking.
pub fn mark(name: &str) {
    if let Some(t0) = t0_ns() {
        let ms = now_ns().saturating_sub(t0) as f64 / 1e6;
        println!("{name} {ms:.2}");
    }
}

/// Print a timing mark the first time only (e.g. `decoded`, which every later decode would repeat).
pub fn mark_once(name: &str, done: &AtomicBool) {
    if !done.swap(true, Ordering::Relaxed) {
        mark(name);
    }
}

/// Should the app close right after painting the first image?
pub fn exit_after_first_paint() -> bool {
    t0_ns().is_some() && std::env::var_os("LECOQ_BENCH_EXIT").is_some()
}

/// The Sibling-switch benchmark (§1.2 "Next/previous Sibling"): with `LECOQ_BENCH_NAV=<interval
/// ms>,<count>`, once the Siblings are scanned the app moves to the next Sibling `count` times,
/// `interval` ms after each switch was painted, and prints `switch_ms <ms>` for each: from the
/// command to the frame that draws the new Image. Then it closes.
pub struct NavBench {
    interval: Duration,
    left: u32,
    /// The switch in flight: when it was asked for, and the file it left.
    in_flight: Option<(Instant, PathBuf)>,
    last_done: Option<Instant>,
}

impl NavBench {
    pub fn from_env() -> Option<Self> {
        t0_ns()?;
        let spec = std::env::var("LECOQ_BENCH_NAV").ok()?;
        let (interval, count) = spec.split_once(',')?;
        Some(Self {
            interval: Duration::from_millis(interval.trim().parse().ok()?),
            left: count.trim().parse().ok()?,
            in_flight: None,
            last_done: None,
        })
    }

    /// Called each frame before the canvas is drawn, with whether the Siblings are known and the
    /// file on screen. Returns the next switch when it's due.
    pub fn frame(
        &mut self,
        ctx: &egui::Context,
        scanned: bool,
        shown: Option<&Path>,
    ) -> Option<Command> {
        self.check(shown);
        if !scanned || self.in_flight.is_some() {
            return None;
        }
        let now = Instant::now();
        let due = self.last_done.get_or_insert(now).to_owned() + self.interval;
        if now < due {
            ctx.request_repaint_after(due - now);
            return None;
        }
        if self.left == 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return None;
        }
        self.left -= 1;
        self.in_flight = Some((now, shown?.to_owned()));
        ctx.request_repaint();
        Some(Command::NextSibling)
    }

    /// Record the switch in flight as done if the file on screen has changed. Called again right
    /// after a switch is executed, for a cached Sibling that shows at once.
    pub fn check(&mut self, shown: Option<&Path>) {
        let Some((started, from)) = &self.in_flight else {
            return;
        };
        if shown.is_some_and(|shown| shown != from) {
            println!("switch_ms {:.2}", started.elapsed().as_secs_f64() * 1e3);
            self.in_flight = None;
            self.last_done = Some(Instant::now());
        }
    }

    /// The last Sibling was reached: there's nothing left to switch to.
    pub fn at_end(&mut self, ctx: &egui::Context) {
        self.in_flight = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
