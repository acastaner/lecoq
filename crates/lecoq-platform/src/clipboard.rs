//! The system clipboard (§6.2, §6.3, §8.1): `clipboard-rs` on Windows, macOS and X11, and
//! `wl-clipboard-rs` (through `clipboard-rs`'s Wayland backend) on Wayland compositors with
//! data-control, falling back to XWayland's clipboard without it.
//!
//! The adapter speaks MIME types, so one copy can publish several formats of the same thing
//! (§6.2: lecoq Annotations as JSON plus a PNG of them) and a paste can say which formats it
//! takes, in order of preference. The OS implementation maps MIME types to each platform's own
//! format names. Calls may block (another application serves the data), so the app makes them
//! off the UI thread.

use std::sync::Mutex;

/// A PNG image: what copying the Image publishes (§6.2), and the first image format a paste takes.
pub const IMAGE_PNG: &str = "image/png";

/// The image formats a paste accepts (§6.3), most preferred first.
pub const IMAGE_FORMATS: &[&str] = &[
    IMAGE_PNG,
    "image/webp",
    "image/jpeg",
    "image/gif",
    "image/bmp",
    "image/tiff",
];

/// One format of what is on the clipboard: a MIME type and its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardItem {
    pub mime: String,
    pub data: Vec<u8>,
}

impl ClipboardItem {
    pub fn new(mime: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            mime: mime.into(),
            data,
        }
    }
}

/// The system clipboard.
pub trait Clipboard: Send + Sync {
    /// Replace the clipboard's contents with `items`: several formats of the same thing, published
    /// together. The error is a reason fit for the status bar.
    fn write(&self, items: Vec<ClipboardItem>) -> Result<(), String>;

    /// The first format in `accept` (MIME types, most preferred first) that the clipboard holds.
    /// `Ok(None)` when it holds none of them, or nothing at all.
    fn read(&self, accept: &[&str]) -> Result<Option<ClipboardItem>, String>;
}

/// The real clipboard. The `clipboard-rs` context is created on first use (never before the first
/// paint, §1.3) and kept: on X11 it is what serves what lecoq copied to other applications.
#[derive(Default)]
pub struct OsClipboard {
    context: Mutex<Option<clipboard_rs::ClipboardContext>>,
}

impl OsClipboard {
    fn with_context<T>(
        &self,
        f: impl FnOnce(&clipboard_rs::ClipboardContext) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut context = self.context.lock().unwrap_or_else(|e| e.into_inner());
        if context.is_none() {
            *context = Some(clipboard_rs::ClipboardContext::new().map_err(|e| e.to_string())?);
        }
        f(context.as_ref().expect("just set"))
    }
}

/// The platform's name for a MIME type.
fn native_format(mime: &str) -> &str {
    match mime {
        IMAGE_PNG if cfg!(windows) => "PNG",
        IMAGE_PNG if cfg!(target_os = "macos") => "public.png",
        "image/tiff" if cfg!(target_os = "macos") => "public.tiff",
        other => other,
    }
}

impl Clipboard for OsClipboard {
    fn write(&self, items: Vec<ClipboardItem>) -> Result<(), String> {
        use clipboard_rs::common::RustImage;
        use clipboard_rs::{Clipboard as _, ClipboardContent, RustImageData};
        // An image goes first: on Windows, setting it empties the clipboard. As an image (not raw
        // bytes) it is also published in the platform's native image formats (DIB, TIFF).
        let (images, others): (Vec<_>, Vec<_>) =
            items.into_iter().partition(|i| i.mime == IMAGE_PNG);
        let mut contents = Vec::new();
        for image in images {
            let image = RustImageData::from_bytes(&image.data).map_err(|e| e.to_string())?;
            contents.push(ClipboardContent::Image(image));
        }
        for item in others {
            contents.push(ClipboardContent::Other(
                native_format(&item.mime).to_owned(),
                item.data,
            ));
        }
        self.with_context(|cx| cx.set(contents).map_err(|e| e.to_string()))
    }

    fn read(&self, accept: &[&str]) -> Result<Option<ClipboardItem>, String> {
        use clipboard_rs::common::RustImage;
        use clipboard_rs::{Clipboard as _, ContentFormat};
        self.with_context(|cx| {
            // Ask for what is there first: on X11 each missing format would cost a read timeout.
            let available = cx.available_formats().unwrap_or_default();
            for mime in accept {
                let native = native_format(mime);
                if available.iter().any(|f| f == native)
                    && let Ok(data) = cx.get_buffer(native)
                    && !data.is_empty()
                {
                    return Ok(Some(ClipboardItem::new(*mime, data)));
                }
            }
            // An image only in a native format (a Windows DIB, a macOS TIFF): as PNG.
            if accept.contains(&IMAGE_PNG)
                && cx.has(ContentFormat::Image)
                && let Ok(image) = cx.get_image()
                && let Ok(png) = image.to_png()
            {
                return Ok(Some(ClipboardItem::new(
                    IMAGE_PNG,
                    png.get_bytes().to_vec(),
                )));
            }
            Ok(None)
        })
    }
}

/// A clipboard for tests: it lives in memory, shared by everything holding the same `Arc`.
#[derive(Default)]
pub struct MemoryClipboard {
    state: Mutex<MemoryClipboardState>,
}

#[derive(Default)]
struct MemoryClipboardState {
    items: Vec<ClipboardItem>,
    writes: usize,
}

impl MemoryClipboard {
    /// What is on the clipboard now, in every format.
    pub fn items(&self) -> Vec<ClipboardItem> {
        self.state.lock().unwrap().items.clone()
    }

    /// The bytes of one format, if the clipboard holds it.
    pub fn get(&self, mime: &str) -> Option<Vec<u8>> {
        let state = self.state.lock().unwrap();
        state
            .items
            .iter()
            .find(|i| i.mime == mime)
            .map(|i| i.data.clone())
    }

    /// Put something on the clipboard, as another application would.
    pub fn put(&self, items: Vec<ClipboardItem>) {
        self.state.lock().unwrap().items = items;
    }

    /// How many times lecoq wrote to the clipboard.
    pub fn writes(&self) -> usize {
        self.state.lock().unwrap().writes
    }
}

impl Clipboard for MemoryClipboard {
    fn write(&self, items: Vec<ClipboardItem>) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        state.items = items;
        state.writes += 1;
        Ok(())
    }

    fn read(&self, accept: &[&str]) -> Result<Option<ClipboardItem>, String> {
        let state = self.state.lock().unwrap();
        Ok(accept
            .iter()
            .find_map(|mime| state.items.iter().find(|i| i.mime == *mime))
            .cloned())
    }
}
