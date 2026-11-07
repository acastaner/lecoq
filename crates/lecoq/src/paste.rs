//! Images coming in to be pasted (§6.3) or Ctrl+dropped (§4.6): read and decoded off the UI thread
//! (another application serves the clipboard's data, and a large image takes a while to decode).
//!
//! A paste prefers lecoq Annotations (§6.3): their MIME type comes before [`IMAGE_FORMATS`] in
//! the formats [`PendingIncoming::paste`] asks the clipboard for.

use std::path::PathBuf;
use std::sync::Arc;

use lecoq_codec::{DisplayImage, Size};
use lecoq_core::{CopiedAnnotation, Pixels};
use lecoq_platform::Clipboard;
use lecoq_platform::clipboard::IMAGE_FORMATS;

use crate::app::job::{Job, Waker};

/// What came in.
pub(crate) enum Incoming {
    /// An image at full resolution (oriented, in sRGB), and its display-size pixels when asked for
    /// (it becomes the Image).
    Image {
        full: Pixels,
        display: Option<DisplayImage>,
    },
    /// lecoq Annotations (§6.3), bottom to top.
    Annotations(Vec<CopiedAnnotation>),
    /// The clipboard holds nothing lecoq can paste.
    Nothing,
}

/// Where an incoming image goes, decided when it was asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Destination {
    /// Into the open Image, as an Inset.
    Inset,
    /// The empty window: it becomes the Image (§3.5).
    Window,
}

/// An incoming image in flight. The error is the status-bar reason.
pub(crate) struct PendingIncoming {
    pub job: Job<Result<Incoming, String>>,
    pub destination: Destination,
    /// The file Ctrl+dropped; `None` for a paste.
    pub file: Option<PathBuf>,
}

impl PendingIncoming {
    /// Read the clipboard and decode the image on it, waking `waker` when it lands. `display_box`:
    /// the size display pixels are made for, when it becomes the Image.
    pub fn paste(
        clipboard: Arc<dyn Clipboard>,
        destination: Destination,
        display_box: Size,
        waker: Option<Waker>,
    ) -> Self {
        let job = Job::spawn("paste", waker, move || {
            // Annotations only go onto an Image; the empty window takes an image.
            let mut accept = Vec::with_capacity(IMAGE_FORMATS.len() + 1);
            if destination == Destination::Inset {
                accept.push(lecoq_core::clipboard::MIME);
            }
            accept.extend_from_slice(IMAGE_FORMATS);
            let Some(item) = clipboard.read(&accept)? else {
                return Ok(Incoming::Nothing);
            };
            if item.mime == lecoq_core::clipboard::MIME {
                return Ok(match lecoq_core::clipboard::from_json(&item.data) {
                    Ok(copied) if !copied.is_empty() => Incoming::Annotations(copied),
                    _ => Incoming::Nothing,
                });
            }
            match lecoq_codec::decode_bytes(&item.data) {
                Ok(decoded) => Ok(image(decoded, destination, display_box)),
                // An image format lecoq can't decode is still nothing to paste.
                Err(_) => Ok(Incoming::Nothing),
            }
        });
        Self {
            job,
            destination,
            file: None,
        }
    }

    /// Decode a Ctrl+dropped file, waking `waker` when it lands.
    pub fn file(
        path: PathBuf,
        destination: Destination,
        display_box: Size,
        waker: Option<Waker>,
    ) -> Self {
        let source = path.clone();
        let job = Job::spawn("decode", waker, move || {
            let decoded = lecoq_codec::decode(&source).map_err(|e| e.to_string())?;
            Ok(image(decoded, destination, display_box))
        });
        Self {
            job,
            destination,
            file: Some(path),
        }
    }

    /// Is the read-and-decode still outstanding?
    pub(crate) fn is_running(&self) -> bool {
        self.job.is_running()
    }
}

fn image(decoded: lecoq_codec::Decoded, destination: Destination, display_box: Size) -> Incoming {
    let size = decoded.size();
    // Decode once: the editor keeps the straight full-resolution pixels (§5.3), and a window paste
    // also gets an upload-ready display copy reduced from them (§3.5, #56).
    let full = decoded.into_full();
    let display = (destination == Destination::Window)
        .then(|| lecoq_codec::DisplayImage::reduced_upload_ready(&full, size, display_box));
    Incoming::Image { full, display }
}
