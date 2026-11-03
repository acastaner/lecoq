//! OS integration behind adapter traits (spec §7.1, §8, §9).
//!
//! The app only ever talks to the OS through a [`Platform`]: [`Platform::os`] in the real binary,
//! [`Platform::in_memory`] in the headless App tests. Each adapter is a trait with a real
//! implementation and an in-memory fake next to it.

pub mod clipboard;
pub mod config;
pub mod dialogs;
pub mod instance;
pub mod launcher;
pub mod trash;

use std::sync::Arc;

pub use clipboard::{Clipboard, ClipboardItem, MemoryClipboard, OsClipboard};
pub use config::{Config, ConfigStore, MemoryConfigStore, OsConfigStore};
pub use dialogs::{DialogReply, FileDialogs, MemoryFileDialogs, OpenDialogText, OsFileDialogs};
pub use instance::{Arrival, HandOff, LocalSocketInstance, MemoryInstance, SingleInstance};
pub use launcher::{Launcher, MemoryLauncher, OsLauncher};
pub use trash::{MemoryTrash, OsTrash, Trash};

/// Every OS adapter the app uses. Cheap to clone.
#[derive(Clone)]
pub struct Platform {
    /// Where `config.toml` lives (§8.3).
    pub config: Arc<dyn ConfigStore>,
    /// Native file dialogs (§8.2).
    pub dialogs: Arc<dyn FileDialogs>,
    /// The system trash (§8.2).
    pub trash: Arc<dyn Trash>,
    /// The single-instance socket (§8.4).
    pub instance: Arc<dyn SingleInstance>,
    /// The system clipboard (§8.1).
    pub clipboard: Arc<dyn Clipboard>,
    /// Starts other lecoq windows (§6.1 Ctrl+Shift+V).
    pub launcher: Arc<dyn Launcher>,
}

impl Platform {
    /// The real adapters.
    pub fn os() -> Self {
        Self {
            config: Arc::new(OsConfigStore::new()),
            dialogs: Arc::new(OsFileDialogs),
            trash: Arc::new(OsTrash),
            instance: Arc::new(LocalSocketInstance::new()),
            clipboard: Arc::new(OsClipboard::default()),
            launcher: Arc::new(OsLauncher),
        }
    }

    /// In-memory fakes for tests: no file system, clipboard, trash or dialogs are touched.
    pub fn in_memory() -> Self {
        Self {
            config: Arc::new(MemoryConfigStore::default()),
            dialogs: Arc::new(MemoryFileDialogs::default()),
            trash: Arc::new(MemoryTrash::default()),
            instance: Arc::new(MemoryInstance::default()),
            clipboard: Arc::new(MemoryClipboard::default()),
            launcher: Arc::new(MemoryLauncher::default()),
        }
    }

    /// The same adapters, with this config store.
    pub fn with_config(mut self, store: Arc<dyn ConfigStore>) -> Self {
        self.config = store;
        self
    }

    /// The same adapters, with this trash.
    pub fn with_trash(mut self, trash: Arc<dyn Trash>) -> Self {
        self.trash = trash;
        self
    }

    /// The same adapters, with this single-instance socket.
    pub fn with_instance(mut self, instance: Arc<dyn SingleInstance>) -> Self {
        self.instance = instance;
        self
    }

    /// The same adapters, with this clipboard.
    pub fn with_clipboard(mut self, clipboard: Arc<dyn Clipboard>) -> Self {
        self.clipboard = clipboard;
        self
    }

    /// The same adapters, with this launcher.
    pub fn with_launcher(mut self, launcher: Arc<dyn Launcher>) -> Self {
        self.launcher = launcher;
        self
    }
}
