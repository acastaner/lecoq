//! Native file dialogs (§8.2): `rfd`, using the XDG desktop portal on Linux.
//!
//! The app never blocks on a dialog: it asks for one and gets the answer through a callback, called
//! from whatever thread the dialog finished on. The app polls for it each frame.
//!
//! The words a dialog shows (its title, filter names) come from the app's string table (§8.6): this
//! crate has none of its own.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Receives what the user chose: `None` when the dialog was cancelled.
pub type DialogReply = Box<dyn FnOnce(Option<PathBuf>) + Send>;

/// What the open dialog says, in the app's words.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenDialogText {
    pub title: String,
    /// The name of the filter for the formats lecoq reads.
    pub images: String,
    /// The name of the filter that shows every file.
    pub all_files: String,
}

/// The native file dialogs.
pub trait FileDialogs: Send + Sync {
    /// Show the open dialog for an image file (Ctrl+O, §3.5). `reply` is called exactly once.
    fn pick_image(&self, text: &OpenDialogText, reply: DialogReply);

    /// Show the Save As dialog (Ctrl+Shift+S, §6.1) titled `title`, offering PNG, JPEG and WebP
    /// (§5.1), with `suggested` as the starting folder and file name. `reply` is called exactly once.
    fn save_image(&self, title: &str, suggested: &Path, reply: DialogReply);

    /// Show the folder picker (Copy to… / Move to…, §6.4, issue #53) titled `title`, starting in
    /// `start` (the last Target folder this session, else the Image's own folder). `reply` is
    /// called exactly once with the folder chosen, or `None` on Cancel.
    fn pick_folder(&self, title: &str, start: &Path, reply: DialogReply);
}

/// The formats Save As offers (§5.1): (filter name, extensions).
pub const SAVE_FILTERS: &[(&str, &[&str])] = &[
    ("PNG", &["png"]),
    ("JPEG", &["jpg", "jpeg"]),
    ("WebP", &["webp"]),
];

/// The extensions of the formats lecoq reads (§5.1).
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "webp", "gif", "bmp", "ico", "tif", "tiff", "avif",
];

/// `rfd`'s native dialogs.
#[derive(Default)]
pub struct OsFileDialogs;

impl FileDialogs for OsFileDialogs {
    fn pick_image(&self, text: &OpenDialogText, reply: DialogReply) {
        // Build the dialog on the calling (UI) thread, as rfd wants on macOS, and wait for it on a
        // thread of its own so the window keeps painting while it is up.
        let dialog = rfd::AsyncFileDialog::new()
            .set_title(&text.title)
            .add_filter(&text.images, IMAGE_EXTENSIONS)
            .add_filter(&text.all_files, &["*"])
            .pick_file();
        let spawned = std::thread::Builder::new()
            .name("open-dialog".into())
            .spawn(move || {
                let chosen = pollster::block_on(dialog);
                reply(chosen.map(|file| file.path().to_owned()));
            });
        if let Err(e) = spawned {
            eprintln!("lecoq: can't show the open dialog: {e}");
        }
    }

    fn save_image(&self, title: &str, suggested: &Path, reply: DialogReply) {
        let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
        if let Some(dir) = suggested.parent().filter(|d| !d.as_os_str().is_empty()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = suggested.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
        // The suggested name's format first: it is the filter the dialog starts on.
        let ext = suggested
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let (first, rest): (Vec<_>, Vec<_>) = SAVE_FILTERS
            .iter()
            .partition(|(_, exts)| exts.contains(&ext.as_str()));
        for (name, exts) in first.into_iter().chain(rest) {
            dialog = dialog.add_filter(name.to_string(), exts);
        }
        let dialog = dialog.save_file();
        let spawned = std::thread::Builder::new()
            .name("save-dialog".into())
            .spawn(move || {
                let chosen = pollster::block_on(dialog);
                reply(chosen.map(|file| file.path().to_owned()));
            });
        if let Err(e) = spawned {
            eprintln!("lecoq: can't show the save dialog: {e}");
        }
    }

    fn pick_folder(&self, title: &str, start: &Path, reply: DialogReply) {
        let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
        if !start.as_os_str().is_empty() {
            dialog = dialog.set_directory(start);
        }
        let dialog = dialog.pick_folder();
        let spawned = std::thread::Builder::new()
            .name("folder-dialog".into())
            .spawn(move || {
                let chosen = pollster::block_on(dialog);
                reply(chosen.map(|folder| folder.path().to_owned()));
            });
        if let Err(e) = spawned {
            eprintln!("lecoq: can't show the folder picker: {e}");
        }
    }
}

/// Scripted dialogs for tests: each dialog shown takes the next queued answer (cancel when none).
#[derive(Default)]
pub struct MemoryFileDialogs {
    state: Mutex<MemoryDialogsState>,
}

#[derive(Default)]
struct MemoryDialogsState {
    answers: VecDeque<Option<PathBuf>>,
    opened: usize,
    /// When set, dialogs stay up: their replies are kept, never called.
    hold: bool,
    held: Vec<DialogReply>,
    last_suggested: Option<PathBuf>,
    last_title: Option<String>,
    last_open_text: Option<OpenDialogText>,
    last_start: Option<PathBuf>,
}

impl MemoryFileDialogs {
    /// What the next dialog shown answers: a chosen path, or `None` for Cancel.
    pub fn answer_next(&self, answer: Option<PathBuf>) {
        self.state.lock().unwrap().answers.push_back(answer);
    }

    /// Keep every dialog from now on up, as if the user never answered it.
    pub fn hold_answers(&self) {
        self.state.lock().unwrap().hold = true;
    }

    /// How many dialogs have been shown.
    pub fn opened(&self) -> usize {
        self.state.lock().unwrap().opened
    }

    /// The path the last Save As dialog suggested.
    pub fn last_suggested(&self) -> Option<PathBuf> {
        self.state.lock().unwrap().last_suggested.clone()
    }

    /// The title of the last dialog shown.
    pub fn last_title(&self) -> Option<String> {
        self.state.lock().unwrap().last_title.clone()
    }

    /// What the last open dialog said.
    pub fn last_open_text(&self) -> Option<OpenDialogText> {
        self.state.lock().unwrap().last_open_text.clone()
    }

    /// The folder the last folder picker started in (its starting directory).
    pub fn last_start(&self) -> Option<PathBuf> {
        self.state.lock().unwrap().last_start.clone()
    }

    fn show(&self, title: &str, reply: DialogReply) {
        let mut state = self.state.lock().unwrap();
        state.opened += 1;
        state.last_title = Some(title.to_owned());
        if state.hold {
            state.held.push(reply);
            return;
        }
        let answer = state.answers.pop_front().flatten();
        drop(state);
        reply(answer);
    }
}

impl FileDialogs for MemoryFileDialogs {
    fn pick_image(&self, text: &OpenDialogText, reply: DialogReply) {
        self.state.lock().unwrap().last_open_text = Some(text.clone());
        self.show(&text.title, reply);
    }

    fn save_image(&self, title: &str, suggested: &Path, reply: DialogReply) {
        self.state.lock().unwrap().last_suggested = Some(suggested.to_owned());
        self.show(title, reply);
    }

    fn pick_folder(&self, title: &str, start: &Path, reply: DialogReply) {
        self.state.lock().unwrap().last_start = Some(start.to_owned());
        self.show(title, reply);
    }
}
