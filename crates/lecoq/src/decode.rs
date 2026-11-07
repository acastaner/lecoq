//! Decodes running on their own thread, polled by the UI: the display-size decode of an Image
//! ([`spawn_display`]) and the full-resolution decode when editing starts ([`spawn_full`]). Both
//! run as a [`Job`].

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use lecoq_codec::{DecodeError, DisplayImage, Size};
use lecoq_core::Pixels;

use crate::app::job::{Job, Waker};

pub type DecodeResult = Result<DisplayImage, DecodeError>;
pub type FullDecodeResult = Result<Pixels, DecodeError>;

/// Start decoding `path` for display (§5.2), waking `waker` when it lands. `display_box` is called
/// only after the full decode, so the startup path can start decoding before it knows the window
/// size (§1.3).
pub(crate) fn spawn_display(
    path: PathBuf,
    waker: Option<Waker>,
    display_box: impl FnOnce() -> Size + Send + 'static,
) -> Job<DecodeResult> {
    Job::spawn("decode", waker, move || {
        let result = lecoq_codec::decode(&path).map(|d| d.into_display(display_box()));
        static FIRST: AtomicBool = AtomicBool::new(false);
        crate::bench::mark_once("decoded", &FIRST);
        result
    })
}

/// Start decoding `path` at full resolution (§4.7), waking `waker` when it lands.
pub(crate) fn spawn_full(path: PathBuf, waker: Option<Waker>) -> Job<FullDecodeResult> {
    Job::spawn("full-decode", waker, move || decode_full(&path))
}

/// Decode `path` at full resolution: oriented and colour-converted like the display decode, but
/// not reduced.
pub(crate) fn decode_full(path: &Path) -> FullDecodeResult {
    // Full-resolution pixels stay straight (not reduced, not premultiplied): the editor and the
    // save encoders work from them (§5.3, §7.2). The display path premultiplies; this one doesn't.
    lecoq_codec::decode(path).map(lecoq_codec::Decoded::into_full)
}
