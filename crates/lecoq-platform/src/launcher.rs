//! Opening another lecoq window (§6.1 Ctrl+Shift+V, *File › New from clipboard*). Every window is
//! its own process (§8.4), so this starts lecoq again.

use std::sync::Mutex;

/// The command-line flag a new window starts with to open the clipboard image (§3.5).
pub const NEW_FROM_CLIPBOARD_ARG: &str = "--new-from-clipboard";

/// Starts other lecoq windows.
pub trait Launcher: Send + Sync {
    /// Open a new window showing the clipboard image. The error is a reason fit for the status bar.
    fn new_window_from_clipboard(&self) -> Result<(), String>;
}

/// Runs this executable again.
#[derive(Default)]
pub struct OsLauncher;

impl Launcher for OsLauncher {
    fn new_window_from_clipboard(&self) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        std::process::Command::new(exe)
            .arg(NEW_FROM_CLIPBOARD_ARG)
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string())
    }
}

/// A launcher for tests: it counts the windows asked for and starts nothing.
#[derive(Default)]
pub struct MemoryLauncher {
    from_clipboard: Mutex<usize>,
}

impl MemoryLauncher {
    /// How many "new window from the clipboard" were asked for.
    pub fn windows_from_clipboard(&self) -> usize {
        *self.from_clipboard.lock().unwrap()
    }
}

impl Launcher for MemoryLauncher {
    fn new_window_from_clipboard(&self) -> Result<(), String> {
        *self.from_clipboard.lock().unwrap() += 1;
        Ok(())
    }
}
