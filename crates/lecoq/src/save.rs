//! Save (§5.1, §5.3, §7.2): Flatten the Document and write it, on the worker thread (§7.3).
//!
//! The UI hands the worker a copy of what is drawn ([`lecoq_core::Document::to_flatten`]), so editing
//! can go on meanwhile. The worker decodes the full-resolution pixels itself when editing hasn't
//! (they come back, so the Document keeps them), Flattens with the same Skia code as the screen,
//! encodes, and writes atomically ([`lecoq_codec::save`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lecoq_codec::{SaveFormat, Size};
use lecoq_core::{Annotation, AnnotationId, Document, PixelData, Pixels, Point, Rect, Shape};
use lecoq_render::{Composite, Renderer, Source, View};

use crate::app::job::{Job, Waker};
use crate::decode::decode_full;
use crate::strings;

/// What a save or copy Flattens (§7.2): the Document, plus where its full-resolution pixels come
/// from when the Document hasn't got them yet. Shared by [`SaveJob`] and [`CopyJob`], which both
/// start from the same input and differ only in where the result goes.
pub(crate) struct FlattenInput {
    /// The Document to Flatten: the size, the Annotations and (when decoded) the full-resolution
    /// pixels.
    pub doc: Document,
    /// The file the Image came from: its pixels (when not yet decoded) and metadata.
    pub source: Option<PathBuf>,
    /// The display-size decode when it is the full resolution: used when the source can't be
    /// decoded any more (deleted or changed on disk, #21). Already the one pixel type (#64).
    pub fallback: Option<Arc<Pixels>>,
}

/// What to save, and where.
pub(crate) struct SaveJob {
    pub input: FlattenInput,
    pub target: PathBuf,
    pub format: SaveFormat,
    pub jpeg_quality: u8,
}

/// A save that went through.
pub(crate) struct Saved {
    /// The full-resolution pixels, when the worker had to decode them.
    pub decoded: Option<Arc<Pixels>>,
}

/// The `<reason>` of "Can't save <name>: <reason>" on failure.
pub(crate) type SaveResult = Result<Saved, String>;

/// Why Flattening the input failed (§7.2): the Document's full-resolution pixels couldn't be
/// obtained, or the file changed underneath. Each maps to a fixed `<reason>` for the "Can't
/// save/copy" message, so it converts straight into the `String` a [`SaveResult`] carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlattenError {
    /// A clipboard Image with no file to decode, and no fallback pixels (§3.5).
    NoPixelsToSave,
    /// The source file is gone and there are no fallback pixels (#21).
    FileChangedOrGone,
    /// The decoded (or fallback) pixels no longer match the Document's size (#21).
    FileChanged,
}

impl FlattenError {
    /// The `<reason>` this failure shows.
    fn reason(self) -> &'static str {
        match self {
            FlattenError::NoPixelsToSave => strings::NO_PIXELS_TO_SAVE,
            FlattenError::FileChangedOrGone => strings::FILE_CHANGED_OR_GONE,
            FlattenError::FileChanged => strings::FILE_CHANGED,
        }
    }
}

impl From<FlattenError> for String {
    fn from(error: FlattenError) -> Self {
        error.reason().to_owned()
    }
}

/// Start `job` on the worker thread (§7.3), waking `waker` when it finishes.
pub(crate) fn spawn_save(job: SaveJob, waker: Option<Waker>) -> Job<SaveResult> {
    Job::spawn("worker", waker, move || run(job))
}

fn run(job: SaveJob) -> SaveResult {
    let info = job
        .input
        .source
        .as_deref()
        .map(lecoq_codec::source_info)
        .unwrap_or_default();
    let (flattened, decoded) = flatten_document(job.input)?;
    let pixels = straight(&flattened);
    let size = Size::new(pixels.width, pixels.height);
    lecoq_codec::save(
        &job.target,
        &pixels,
        size,
        job.format,
        &info,
        job.jpeg_quality,
    )
    .map_err(|e| strings::save_reason(&e))?;
    Ok(Saved { decoded })
}

/// What to copy to the clipboard (§6.2): the same as a save, without a target file.
pub(crate) struct CopyJob {
    pub input: FlattenInput,
    pub clipboard: Arc<dyn lecoq_platform::Clipboard>,
}

/// Flatten, encode as PNG and put on the clipboard, on the worker thread (§7.3), waking `waker`.
pub(crate) fn spawn_copy(job: CopyJob, waker: Option<Waker>) -> Job<SaveResult> {
    Job::spawn("worker", waker, move || copy(job))
}

fn copy(job: CopyJob) -> SaveResult {
    let (flattened, decoded) = flatten_document(job.input)?;
    let pixels = straight(&flattened);
    let size = Size::new(pixels.width, pixels.height);
    let png = lecoq_codec::encode_png_plain(&pixels, size).map_err(|e| strings::save_reason(&e))?;
    job.clipboard
        .write(vec![lecoq_platform::ClipboardItem::new(
            lecoq_platform::clipboard::IMAGE_PNG,
            png,
        )])?;
    Ok(Saved { decoded })
}

/// What copying selected Annotations puts on the clipboard (§6.2).
pub(crate) struct AnnotationsCopyJob {
    pub doc: Document,
    /// The copied Annotations (the selection).
    pub ids: Vec<AnnotationId>,
    pub clipboard: Arc<dyn lecoq_platform::Clipboard>,
}

/// Serialise the Annotations and render their PNG, on the worker thread (§7.3), waking `waker`.
pub(crate) fn spawn_copy_annotations(
    job: AnnotationsCopyJob,
    waker: Option<Waker>,
) -> Job<SaveResult> {
    Job::spawn("worker", waker, move || copy_annotations(job))
}

/// How far drawing reaches beyond an Annotation's bounds (line, arrowheads, shadow), at most;
/// the PNG is trimmed to what was drawn afterwards.
fn drawn_margin(style: &lecoq_core::Style) -> f32 {
    lecoq_core::Arrow::head_length(style.thickness) + style.thickness + 12.0
}

/// Publish the Annotations `job.ids` as clipboard JSON and, together, a PNG of them on
/// transparency (§6.2). The PNG leaves out types that draw from the pixels under them
/// ([`Shape::needs_pixels_below`]: highlight, obfuscate); it is omitted when nothing is left.
fn copy_annotations(job: AnnotationsCopyJob) -> SaveResult {
    let copied: Vec<&Annotation> = job
        .doc
        .annotations()
        .iter()
        .filter(|a| job.ids.contains(&a.id))
        .collect();
    if copied.is_empty() {
        return Err(strings::NOTHING_SELECTED.into());
    }
    let json = lecoq_core::clipboard::to_json(copied.iter().copied());
    let in_png: Vec<&Annotation> = copied
        .into_iter()
        .filter(|a| !a.shape.needs_pixels_below())
        .collect();
    let mut items = Vec::new();
    if let Some(png) = annotations_png(&job.doc, &in_png)? {
        // The PNG first: on Windows, setting an image clears the clipboard (see OsClipboard).
        items.push(lecoq_platform::ClipboardItem::new(
            lecoq_platform::clipboard::IMAGE_PNG,
            png,
        ));
    }
    items.push(lecoq_platform::ClipboardItem::new(
        lecoq_core::clipboard::MIME,
        json,
    ));
    job.clipboard.write(items)?;
    Ok(Saved { decoded: None })
}

/// `annotations` of `doc` drawn at 100% on transparency, trimmed to what was drawn.
fn annotations_png(doc: &Document, annotations: &[&Annotation]) -> Result<Option<Vec<u8>>, String> {
    let Some(region) = annotations
        .iter()
        .map(|a| a.bounds().expand(drawn_margin(&a.style)))
        .reduce(|a, b| a.union(&b))
    else {
        return Ok(None);
    };
    let min = Point::new(region.min.x.floor(), region.min.y.floor());
    let max = Point::new(region.max.x.ceil(), region.max.y.ceil());
    let region = Rect::from_corners(min, max);
    let view = View {
        region,
        width: region.width() as u32,
        height: region.height() as u32,
    };
    let ids: Vec<AnnotationId> = annotations.iter().map(|a| a.id).collect();
    let composite = Renderer::new().annotations_only(doc, &ids, view);
    let Some(trimmed) = trim_transparent(&composite) else {
        return Ok(None);
    };
    let pixels = straight(&trimmed);
    let size = Size::new(pixels.width, pixels.height);
    lecoq_codec::encode_png_plain(&pixels, size)
        .map(Some)
        .map_err(|e| strings::save_reason(&e))
}

/// The composite cropped to its pixels that aren't fully transparent; `None` when all are.
fn trim_transparent(c: &Composite) -> Option<Composite> {
    let (w, h) = (c.width as usize, c.height as usize);
    let alpha = |x: usize, y: usize| c.rgba_premultiplied[(y * w + x) * 4 + 3];
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if alpha(x, y) != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x0 >= x1 {
        return None;
    }
    let mut rgba = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        rgba.extend_from_slice(&c.rgba_premultiplied[(y * w + x0) * 4..(y * w + x1) * 4]);
    }
    Some(Composite {
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
        rgba_premultiplied: rgba,
    })
}

/// Flatten the input at full resolution, decoding its pixels from the source file (or taking the
/// fallback) when the Document hasn't got them yet; those decoded pixels come back too.
fn flatten_document(input: FlattenInput) -> Result<(Composite, Option<Arc<Pixels>>), FlattenError> {
    let FlattenInput {
        mut doc,
        source,
        fallback,
    } = input;
    let (full, decoded) = match doc.full_resolution() {
        Some(full) => (full.clone(), None),
        None => {
            let decoded = source
                .as_deref()
                .ok_or(FlattenError::NoPixelsToSave)
                .and_then(|source| decode_full(source).map_err(|_| FlattenError::FileChangedOrGone))
                .ok()
                .filter(|p| (p.width, p.height) == (doc.width(), doc.height()));
            let full = match (decoded, fallback) {
                (Some(pixels), _) => Arc::new(pixels),
                // The fallback is premultiplied display pixels; straighten them back to the
                // full-resolution layout the Flatten and the encoders expect (#21, #64).
                (None, Some(fallback)) => Arc::new(fallback.straight()),
                (None, None) => return Err(FlattenError::FileChangedOrGone),
            };
            (full.clone(), Some(full))
        }
    };
    if (full.width, full.height) != (doc.width(), doc.height()) {
        return Err(FlattenError::FileChanged);
    }
    doc.set_full_resolution(full.clone());
    Ok((flatten(&doc, &full), decoded))
}

/// Flatten (§7.2): the same renderer as the canvas, over the whole Image at full resolution.
fn flatten(doc: &Document, full: &Pixels) -> Composite {
    let source = Source {
        key: 0,
        pixels: full,
    };
    Renderer::new().flatten(doc, &source)
}

/// The composite's premultiplied pixels as the encoders take them (#64): [`Pixels::straight`] gives
/// RGB when opaque and straight RGBA otherwise, the un-premultiply living next to the pixel type.
fn straight(composite: &Composite) -> Pixels {
    Pixels {
        width: composite.width,
        height: composite.height,
        data: PixelData::RgbaPremultiplied(composite.rgba_premultiplied.clone()),
    }
    .straight()
}

/// Where Ctrl+S writes without asking: the file itself, when lecoq writes its format (§5.1).
pub(crate) fn in_place_format(path: &Path) -> Option<SaveFormat> {
    SaveFormat::from_path(path)
}

/// What Save As suggests: the file itself, or the same name as PNG for a read-only format. A
/// clipboard Image (no file) suggests "Clipboard image.png".
pub(crate) fn suggestion(path: Option<&Path>) -> PathBuf {
    let Some(path) = path else {
        return PathBuf::from(format!("{}.png", strings::CLIPBOARD_IMAGE));
    };
    if in_place_format(path).is_some() {
        path.to_owned()
    } else {
        path.with_extension("png")
    }
}

/// The file Save As writes for the name chosen in the dialog, and its format: a name without a
/// writable extension gets `.png` added (the default format).
pub(crate) fn chosen_target(chosen: PathBuf) -> (PathBuf, SaveFormat) {
    match SaveFormat::from_path(&chosen) {
        Some(format) => (chosen, format),
        None => {
            let mut name = chosen.into_os_string();
            name.push(".png");
            (PathBuf::from(name), SaveFormat::Png)
        }
    }
}
