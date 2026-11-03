//! Debounced writes of `config.toml` (§8.3).
//!
//! Every settings change calls [`ConfigSaver::changed`]. The file is written once the settings have
//! been quiet for [`DEBOUNCE`], on a thread of its own (the store's write is atomic and may sync to
//! disk). [`ConfigSaver::flush`] writes a pending change at once, on exit.

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use lecoq_platform::{Config, ConfigStore};

/// How long the settings must stay unchanged before they are written.
pub const DEBOUNCE: Duration = Duration::from_millis(500);

pub(crate) struct ConfigSaver {
    store: Arc<dyn ConfigStore>,
    /// When the pending change is due to be written.
    due: Option<Instant>,
    writing: Option<JoinHandle<()>>,
}

impl ConfigSaver {
    pub fn new(store: Arc<dyn ConfigStore>) -> Self {
        Self {
            store,
            due: None,
            writing: None,
        }
    }

    /// The settings changed: write them once they've been quiet for [`DEBOUNCE`].
    pub fn changed(&mut self) {
        self.due = Some(Instant::now() + DEBOUNCE);
    }

    /// Is a write pending or running?
    pub fn is_busy(&self) -> bool {
        self.due.is_some() || self.writing.is_some()
    }

    /// Call every frame: starts the write when it's due, and asks for a repaint until then.
    pub fn poll(&mut self, ctx: &egui::Context, config: &Config) {
        if self.writing.as_ref().is_some_and(|w| w.is_finished()) {
            self.join_writer();
        }
        let Some(due) = self.due else {
            return;
        };
        let now = Instant::now();
        if now < due {
            ctx.request_repaint_after(due - now);
            return;
        }
        if self.writing.is_some() {
            // The previous write is still running; try again shortly.
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        self.due = None;
        let store = self.store.clone();
        let text = config.to_toml();
        let ctx = ctx.clone();
        self.writing = std::thread::Builder::new()
            .name("config-write".into())
            .spawn(move || {
                write(store.as_ref(), &text);
                ctx.request_repaint();
            })
            .map_err(|e| eprintln!("lecoq: can't start the config write: {e}"))
            .ok();
    }

    /// Write a pending change now and wait for any running write (on exit).
    pub fn flush(&mut self, config: &Config) {
        self.join_writer();
        if self.due.take().is_some() {
            write(self.store.as_ref(), &config.to_toml());
        }
    }

    fn join_writer(&mut self) {
        if let Some(writer) = self.writing.take() {
            let _ = writer.join();
        }
    }
}

fn write(store: &dyn ConfigStore, text: &str) {
    if let Err(e) = store.write(text) {
        eprintln!("lecoq: can't write config.toml: {e}");
    }
}
