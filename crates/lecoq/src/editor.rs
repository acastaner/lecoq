//! The editor (§4): the open Document, the active tool and the tools' styles, the full-resolution
//! decode, and the Skia composite of the canvas.
//!
//! The app owns one [`Editor`]. Per Image state ([`Edited`]) is replaced on every open, which is
//! what clears the History when moving to another Image (§4.8). The active tool and each tool's
//! style outlive it.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

use egui::CursorIcon;
use lecoq_codec::Size;
use lecoq_core::{
    AnnotationId, CopiedAnnotation, Document, HISTORY_PIXEL_CAP, Handle, Inset, Pixels, Point,
    Property, PropertyKind, Rect, Restack, Shape, Style, Vector,
};
use lecoq_render::{Composite, Renderer, Source, View};

use crate::app::job::{self, Job};
use crate::decode::{self, FullDecodeResult};
use crate::tools::select::End;
use crate::tools::text::{TextSession, text_at};
use crate::tools::{Pointer, Tool, ToolContext, ToolId, select};
use crate::ui::canvas::{EditorPaint, Layout};

/// The panning affordance the cursor shows (#43, #48), which the app reads from the pan gesture: a
/// pan drag under way is [`PanCursor::Active`] (the closed hand), and Space held over a pannable
/// Image is [`PanCursor::Ready`] (the open hand). Panning is possible only when the Image is zoomed
/// beyond Fit (§3.1), so the grab cursor never promises a pan that can't happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanCursor {
    /// No panning affordance at the pointer.
    None,
    /// A pan could start now (Space held over a pannable Image): the open hand.
    Ready,
    /// A pan drag is under way: the closed hand.
    Active,
}

/// Editing needs the full-resolution decode, allowed up to 100 MP (§4.7).
pub const MAX_EDIT_PIXELS: u64 = 100_000_000;

/// How far Ctrl+D moves the copies, right and down, in Image pixels (§4.2).
pub const DUPLICATE_OFFSET: f32 = 10.0;

/// The display-size pixels of the shown Image (shared with its texture), which the composite draws
/// from unless zoomed past them.
#[derive(Clone, Copy)]
pub struct DisplayPixels<'a> {
    /// Unique per decode: the renderer's cache key.
    pub key: u64,
    /// The display-size pixels, premultiplied and upload-ready (#64, §7.2).
    pub pixels: &'a Pixels,
}

/// The editor's state for the open Image.
struct Edited {
    doc: Document,
    /// The file the Image came from; `None` for a clipboard image not saved yet (§3.5).
    path: Option<PathBuf>,
    view_only: bool,
    full_decode: Option<Job<FullDecodeResult>>,
    /// Which open this is: a save finishing after another Image was opened leaves it alone.
    generation: u64,
    /// Image operations chosen before the full-resolution pixels arrived: done when they do.
    queued: Vec<ImageOp>,
    /// The last composite, its texture, and what it was rendered for.
    composite: Option<CompositeTexture>,
}

struct CompositeTexture {
    key: (u64, u64, View),
    texture: egui::TextureHandle,
    dest: egui::Rect,
    /// The view-sized RGBA-premultiplied buffer the composite is rendered straight into, shared
    /// with `texture`: uploading clones this `Arc`, so no copy happens between render and upload
    /// (#55). `image.as_raw_mut()` is the byte buffer the renderer and the fast path write into,
    /// and it is kept between frames so a same-size view reuses the allocation.
    image: Arc<egui::ColorImage>,
    /// Each Annotation's drawn bounds (Image pixels) as of this composite, in z-order: the
    /// dirty-region fast path diffs them against the next frame's to find what changed (#46).
    bounds: Vec<(AnnotationId, Rect)>,
    /// An opaque neighbourhood effect was frozen during the current gesture, so one full re-render
    /// is forced when the gesture ends to correct it. `commit_gesture` does not bump the Document
    /// revision, so the flag (not the revision) is what triggers that correction (#46).
    froze: bool,
}

/// A property-bar interaction in progress: changes to `kind` merge into one step once the first
/// one has made a step.
#[derive(Clone, Copy, PartialEq)]
struct PropertySession {
    kind: PropertyKind,
    stepped: bool,
}

pub struct Editor {
    tool: ToolId,
    behaviour: Box<dyn Tool>,
    /// Each tool's current style (its defaults, §4.3), saved as `tools.<tool>` in the config.
    styles: HashMap<ToolId, Style>,
    edited: Option<Edited>,
    /// Created on the first Annotation (§1.3, §7.2): creating it initialises Skia.
    renderer: Option<Renderer>,
    /// Is a pointer gesture on the canvas in progress?
    pointer_down: bool,
    property_session: Option<PropertySession>,
    /// The selection box as painted last frame, in points.
    selection_box: Option<egui::Rect>,
    /// The text box being edited (§4.4), if any.
    text: Option<TextSession>,
    /// The installed font families, listed on first use of the text tool (§7.2).
    fonts: Fonts,
    /// The crop tool's region, in Image pixels (§4.1 C).
    crop: Option<Rect>,
    /// The cap on pixels each Document's History holds (§4.8).
    history_pixel_cap: usize,
    /// The rubber band as painted last frame, in points.
    rubber_band: Option<egui::Rect>,
    /// How many Images have been opened (each one's [`Edited::generation`]).
    opened: u64,
    /// The UI context whose repaint the full-resolution decode wakes when it lands; set by the app
    /// once the window exists. `None` in editor-only tests, which step frames and poll directly.
    waker: Option<egui::Context>,
}

/// What a save (#34) needs from the editor when it starts.
pub struct SaveSource {
    /// What is drawn, for the worker to Flatten.
    pub doc: Document,
    /// The file the Document came from (`None`: a clipboard image).
    pub path: Option<PathBuf>,
    /// Identifies the Document and its state, for [`Editor::saved`].
    pub stamp: SaveStamp,
}

/// Which Document, in which state, a save wrote.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SaveStamp {
    generation: u64,
    revision: u64,
}

/// The font families for the property bar's font control.
enum Fonts {
    NotListed,
    Listing(mpsc::Receiver<Vec<String>>),
    Listed(Vec<String>),
}

/// How far from a text box a press still counts as inside it, in points.
const TEXT_SLOP: f32 = 2.0;

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        // Registering a function pointer touches no fonts: they load on the first text box.
        lecoq_core::annotation::text::set_measure(lecoq_render::text::measure);
        Self {
            tool: ToolId::Select,
            behaviour: ToolId::Select.create().expect("the select tool"),
            styles: HashMap::new(),
            edited: None,
            renderer: None,
            pointer_down: false,
            property_session: None,
            selection_box: None,
            text: None,
            fonts: Fonts::NotListed,
            opened: 0,
            crop: None,
            history_pixel_cap: HISTORY_PIXEL_CAP,
            rubber_band: None,
            waker: None,
        }
    }

    /// Give the editor the UI context the full-resolution decode wakes when it finishes. The app
    /// sets it once the window exists, so the decode needs no per-frame waker re-registration.
    pub fn set_waker(&mut self, ctx: egui::Context) {
        self.waker = Some(ctx);
    }

    /// Change the History's pixel cap for the Documents opened from now on (tests lower it).
    pub fn set_history_pixel_cap(&mut self, bytes: usize) {
        self.history_pixel_cap = bytes;
    }

    /// An editor whose tools start with the styles saved in `config.toml` (`tools.<tool>`, §8.3).
    /// A table that isn't a valid style leaves that tool on its defaults.
    pub fn with_tool_styles(saved: &BTreeMap<String, toml::Table>) -> Self {
        let mut editor = Self::new();
        for tool in ToolId::styled() {
            let Some(table) = saved.get(tool.config_key()) else {
                continue;
            };
            // Keys missing from the table keep the tool's own defaults (a Step counter's fill).
            let mut merged = toml::Table::try_from(tool.default_style()).unwrap_or_default();
            merged.extend(table.clone());
            match toml::Value::Table(merged).try_into::<Style>() {
                Ok(style) => {
                    editor.styles.insert(tool, style.sanitised());
                }
                Err(e) => eprintln!(
                    "lecoq: tools.{} in config.toml is not a valid style, using the defaults: {e}",
                    tool.config_key()
                ),
            }
        }
        editor
    }

    /// A new Image is on screen: start a fresh Document (and History). `path`: the file it came
    /// from; `None` for a clipboard image, whose pixels come with [`Self::open_pixels`].
    pub fn open(&mut self, path: Option<&Path>, size: Size) {
        let view_only = u64::from(size.width) * u64::from(size.height) > MAX_EDIT_PIXELS;
        // Drop any half-done gesture: it belonged to the previous Document.
        self.behaviour = self.tool.create().expect("an available tool");
        self.pointer_down = false;
        self.property_session = None;
        self.text = None;
        self.opened += 1;
        self.crop = None;
        let mut doc = Document::new(size.width, size.height);
        doc.set_history_pixel_cap(self.history_pixel_cap);
        self.edited = Some(Edited {
            doc,
            path: path.map(Path::to_owned),
            view_only,
            full_decode: None,
            generation: self.opened,
            queued: Vec::new(),
            composite: None,
        });
        if view_only {
            self.set_tool(ToolId::Select);
        }
    }

    /// A clipboard image is on screen (§3.5): a fresh Document that already has its pixels.
    pub fn open_pixels(&mut self, pixels: Arc<Pixels>) {
        self.open(None, Size::new(pixels.width, pixels.height));
        if let Some(e) = &mut self.edited {
            e.doc.set_full_resolution(pixels);
        }
    }

    /// No Image any more (a failed open).
    pub fn close(&mut self) {
        self.edited = None;
        self.behaviour = self.tool.create().expect("an available tool");
        self.pointer_down = false;
        self.text = None;
        self.crop = None;
    }

    /// Is a text box being edited (§4.4)?
    pub fn editing_text(&self) -> bool {
        self.text.is_some()
    }

    /// The installed font families, once listed.
    pub fn font_families(&self) -> Option<&[String]> {
        match &self.fonts {
            Fonts::Listed(names) => Some(names),
            _ => None,
        }
    }

    /// §7.2: the fonts are enumerated the first time the text tool is used, in the background.
    fn list_fonts(&mut self) {
        if matches!(self.fonts, Fonts::NotListed) {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(lecoq_render::text::families());
            });
            self.fonts = Fonts::Listing(rx);
        }
    }

    pub fn document(&self) -> Option<&Document> {
        self.edited.as_ref().map(|e| &e.doc)
    }

    pub fn tool(&self) -> ToolId {
        self.tool
    }

    pub fn is_view_only(&self) -> bool {
        self.edited.as_ref().is_some_and(|e| e.view_only)
    }

    /// Can the tools be used? Not without an Image, nor on a view-only one.
    pub fn can_edit(&self) -> bool {
        self.edited.as_ref().is_some_and(|e| !e.view_only)
    }

    /// The crop tool's region, in Image pixels.
    pub fn crop_region(&self) -> Option<Rect> {
        self.crop
    }

    /// The selection box (dashed, with 8 handles) as painted last frame, in points.
    pub fn selection_box(&self) -> Option<egui::Rect> {
        self.selection_box
    }

    /// The select tool's rubber band as painted last frame, in points.
    pub fn rubber_band(&self) -> Option<egui::Rect> {
        self.rubber_band
    }

    /// The last Skia composite of the canvas, as an owned [`Composite`] (§7.2). The pixels live in
    /// the texture's shared `ColorImage` now (#55); this reads them back for tests and inspection,
    /// so it is off the render path.
    pub fn composite(&self) -> Option<Composite> {
        let c = self.edited.as_ref()?.composite.as_ref()?;
        Some(Composite {
            width: c.image.size[0] as u32,
            height: c.image.size[1] as u32,
            rgba_premultiplied: c.image.as_raw().to_vec(),
        })
    }

    /// The style the property bar shows when nothing is selected: the active tool's.
    pub fn tool_style(&self, tool: ToolId) -> Style {
        self.styles
            .get(&tool)
            .cloned()
            .unwrap_or_else(|| tool.default_style())
    }

    /// Is the full-resolution decode still running?
    pub fn is_busy(&self) -> bool {
        self.edited
            .as_ref()
            .is_some_and(|e| e.full_decode.is_some())
            || matches!(self.fonts, Fonts::Listing(_))
    }

    // ---- Saving (#34) ----

    /// The Document to save, as it is now. `None` without an Image, or mid-gesture.
    pub fn save_source(&self) -> Option<SaveSource> {
        let e = self.edited.as_ref().filter(|e| !e.doc.in_gesture())?;
        Some(SaveSource {
            doc: e.doc.to_flatten(),
            path: e.path.clone(),
            stamp: SaveStamp {
                generation: e.generation,
                revision: e.doc.revision(),
            },
        })
    }

    /// A save of `stamp` was written to `target`. The Document now belongs to `target`; it is
    /// marked saved (§4.8: the History stays) unless it changed while the save ran. `decoded`: the
    /// full-resolution pixels the save had to decode, kept so later saves and zooms don't decode
    /// the Flattened file.
    pub fn saved(&mut self, stamp: SaveStamp, target: &Path, decoded: Option<Arc<Pixels>>) {
        let Some(e) = self
            .edited
            .as_mut()
            .filter(|e| e.generation == stamp.generation)
        else {
            return;
        };
        e.path = Some(target.to_owned());
        if e.doc.revision() == stamp.revision {
            e.doc.mark_saved();
        }
        self.keep_decoded(stamp, decoded);
    }

    /// The full-resolution pixels a save or a copy (on the worker) had to decode for `stamp`'s
    /// Document: kept, so later saves and zooms don't decode again.
    pub fn keep_decoded(&mut self, stamp: SaveStamp, decoded: Option<Arc<Pixels>>) {
        let Some(e) = self
            .edited
            .as_mut()
            .filter(|e| e.generation == stamp.generation)
        else {
            return;
        };
        if let Some(pixels) = decoded
            && e.doc.full_resolution().is_none()
        {
            // A full decode still running could read the Flattened file: these pixels win, and a
            // crop waiting for that decode is done now.
            e.full_decode = None;
            e.doc.set_full_resolution(pixels);
            for op in std::mem::take(&mut e.queued) {
                op.apply(&mut e.doc);
            }
        }
    }

    /// Is the Document waiting for its full-resolution pixels before it is what the user sees (an
    /// Image operation queued until they arrive)? A save waits too.
    pub fn save_waits(&self) -> bool {
        self.edited.as_ref().is_some_and(|e| !e.queued.is_empty())
    }

    /// Has the Image been edited: an Annotation or Image operation done (even if undone since), or
    /// an Image operation waiting for the full resolution? Freezes an animation (§3.4).
    pub fn has_edits(&self) -> bool {
        self.edited
            .as_ref()
            .is_some_and(|e| e.doc.can_undo() || e.doc.can_redo() || !e.queued.is_empty())
    }

    // ---- Commands ----

    pub fn select_tool(&mut self, tool: ToolId) {
        if !tool.is_available() || (tool != ToolId::Select && !self.can_edit()) {
            return;
        }
        if tool == ToolId::Text {
            self.list_fonts();
        }
        if tool != ToolId::Select {
            self.start_full_decode();
            if let Some(e) = &mut self.edited
                && tool != self.tool
            {
                e.doc.clear_selection();
            }
        }
        self.set_tool(tool);
    }

    fn set_tool(&mut self, tool: ToolId) {
        self.finish_text();
        if let Some(e) = &mut self.edited {
            let style = self
                .styles
                .get(&self.tool)
                .cloned()
                .unwrap_or_else(|| self.tool.default_style());
            let mut cx = ToolContext {
                doc: &mut e.doc,
                style: &style,
                image_per_point: 1.0,
                crop: &mut self.crop,
            };
            self.behaviour.cancel(&mut cx);
        }
        // Leaving the crop tool drops its region (Esc cancels the crop, §4.1).
        self.crop = None;
        self.pointer_down = false;
        self.tool = tool;
        self.behaviour = tool.create().expect("an available tool");
    }

    /// A secondary (right) press on the canvas returns to the Select tool, cancelling any
    /// in-progress draw and dropping a pending crop (#47, ADR 0006). In Select it does nothing — no
    /// menu, and the current selection is left untouched. Going through `select_tool` (not raw
    /// `set_tool`) keeps it on the same path as the tool strip and Esc.
    fn on_secondary_press(&mut self) {
        if self.tool != ToolId::Select {
            self.select_tool(ToolId::Select);
        }
    }

    pub fn undo(&mut self) {
        if let Some(e) = self.edited_idle() {
            e.doc.undo();
        }
    }

    pub fn redo(&mut self) {
        if let Some(e) = self.edited_idle() {
            e.doc.redo();
        }
    }

    /// Enter or Apply with the crop tool: crop to its region (§4.7), then back to the select tool.
    pub fn apply_crop(&mut self) {
        let Some(region) = self.crop.filter(|_| self.tool == ToolId::Crop) else {
            return;
        };
        self.image_operation(ImageOp::Crop(region));
        self.set_tool(ToolId::Select);
    }

    /// An Image operation is about to be chosen (the Resize dialog is up): start the
    /// full-resolution decode now (§4.7).
    pub fn prepare_image_operation(&mut self) {
        if self.can_edit() {
            self.start_full_decode();
        }
    }

    /// Carry out an Image operation (§4.7) as one History step. Before the full-resolution pixels
    /// have arrived (this starts their decode), it waits for them. Ignored mid-gesture and on a
    /// view-only Image. Any other operation ends the crop tool (its region would no longer fit).
    pub fn image_operation(&mut self, op: ImageOp) {
        if !self.can_edit() || self.edited_idle().is_none() {
            return;
        }
        if self.tool == ToolId::Crop && !matches!(op, ImageOp::Crop(_)) {
            self.set_tool(ToolId::Select);
        }
        self.start_full_decode();
        let e = self.edited.as_mut().expect("checked above");
        if !e.queued.is_empty() || e.doc.full_resolution().is_none() {
            e.queued.push(op);
        } else {
            op.apply(&mut e.doc);
        }
    }

    /// Cancel (Esc) with the crop tool: drop the region and go back to the select tool.
    pub fn cancel_crop(&mut self) {
        if self.tool == ToolId::Crop && !self.pointer_down {
            self.set_tool(ToolId::Select);
        }
    }

    /// Delete the selected Annotations. Returns false when nothing was selected.
    pub fn delete_selection(&mut self) -> bool {
        self.edited_idle().is_some_and(|e| e.doc.delete_selection())
    }

    /// Paste or Ctrl+drop an image (§4.6, §6.3): insert it as an Inset centred on `centre`
    /// (Image pixels) as one step, switch to the select tool and select it. Returns false when the
    /// Image can't be edited (view-only, mid-gesture).
    pub fn insert_inset(&mut self, pixels: Arc<Pixels>, centre: Option<Point>) -> bool {
        if !self.can_edit() || self.edited_idle().is_none() {
            return false;
        }
        self.set_tool(ToolId::Select);
        let style = self.tool_style(ToolId::Inset);
        let e = self.edited.as_mut().expect("checked above");
        let image = e.doc.bounds();
        let inset = Inset::placed(pixels, centre.unwrap_or(image.center()), image);
        e.doc.step(|doc| {
            let id = doc.add(inset, style);
            doc.select([id]);
        });
        true
    }

    /// Paste lecoq Annotations (§6.3) as one step: added and selected, moved by
    /// [`crate::app::PASTE_OFFSET`] while they would land exactly on Annotations already there.
    /// Switches to the select tool, as pasting an Inset does. Returns false when the Image can't
    /// be edited (view-only, mid-gesture).
    pub fn paste_annotations(&mut self, copied: Vec<CopiedAnnotation>) -> bool {
        if !self.can_edit() || self.edited_idle().is_none() {
            return false;
        }
        self.set_tool(ToolId::Select);
        let e = self.edited.as_mut().expect("checked above");
        let offset = crate::app::PASTE_OFFSET;
        e.doc.paste(copied, Vector::new(offset, offset))
    }

    /// Ctrl+D: duplicate the selection, offset by [`DUPLICATE_OFFSET`] Image pixels.
    pub fn duplicate_selection(&mut self) {
        if let Some(e) = self.edited_idle() {
            e.doc
                .duplicate_selection(Vector::new(DUPLICATE_OFFSET, DUPLICATE_OFFSET));
        }
    }

    pub fn restack(&mut self, restack: Restack) {
        if let Some(e) = self.edited_idle() {
            e.doc.restack(restack);
        }
    }

    /// Restyle the selection, and make the change the default of the tools that create the
    /// selected Annotations (§4.3). With nothing selected, it is the active tool's default.
    pub fn set_property(&mut self, property: Property, merge: bool) -> Vec<ToolId> {
        // While editing text, the box being edited is restyled within the session's step, and the
        // change becomes the text tool's default (§4.3).
        if let (Some(session), Some(e)) = (&self.text, &mut self.edited) {
            session.restyle(&mut e.doc, property.clone());
            self.styles.entry(ToolId::Text).or_default().set(property);
            return vec![ToolId::Text];
        }
        let mut tools = vec![self.tool];
        if let Some(e) = self.edited.as_mut().filter(|e| !e.doc.in_gesture())
            && e.doc.set_property(property.clone(), merge)
        {
            tools = e
                .doc
                .selected()
                .map(|a| ToolId::for_shape(&a.shape))
                .collect();
        }
        let kind = property.kind();
        let mut changed = Vec::new();
        for tool in tools {
            if tool.properties().contains(&kind) {
                self.styles
                    .entry(tool)
                    .or_insert_with(|| tool.default_style())
                    .set(property.clone());
                if !changed.contains(&tool) {
                    changed.push(tool);
                }
            }
        }
        changed
    }

    /// One tool's style as its `tools.<tool>` table for `config.toml` (§8.3): the property command
    /// persists only the tools a change applied to, rather than every tool's style on every tick
    /// (#61). `None` for a tool with no style set this run (or read from the config), or one that
    /// doesn't serialise.
    pub fn tool_style_config(&self, tool: ToolId) -> Option<(String, toml::Table)> {
        let table = toml::Table::try_from(self.styles.get(&tool)?).ok()?;
        Some((tool.config_key().to_owned(), table))
    }

    /// Set a tool's default for its next Annotations, whatever is selected (settings seed them).
    pub fn set_tool_default(&mut self, tool: ToolId, property: Property) {
        self.styles
            .entry(tool)
            .or_insert_with(|| tool.default_style())
            .set(property);
    }

    /// Is a tool gesture (a drag) under way?
    pub fn in_gesture(&self) -> bool {
        self.edited.as_ref().is_some_and(|e| e.doc.in_gesture())
    }

    /// The edited Image, when no pointer gesture is in progress.
    fn edited_idle(&mut self) -> Option<&mut Edited> {
        self.edited.as_mut().filter(|e| !e.doc.in_gesture())
    }

    /// The zoom upgrade needs the full-resolution pixels (#57): run the one shared decode if it
    /// isn't running or landed already. The same decode as the first tool use, so whichever asks
    /// first runs it and the other adopts its pixels through [`Document::full_resolution`].
    pub fn request_full_decode(&mut self) {
        self.start_full_decode();
    }

    /// §4.7: the first tool use starts the full-resolution decode, in the background.
    fn start_full_decode(&mut self) {
        let waker = self.waker.as_ref().map(job::waker);
        let Some(e) = &mut self.edited else {
            return;
        };
        if e.view_only || e.full_decode.is_some() || e.doc.full_resolution().is_some() {
            return;
        }
        if let Some(path) = &e.path {
            e.full_decode = Some(decode::spawn_full(path.clone(), waker));
        }
    }

    /// Collect background results.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Fonts::Listing(rx) = &self.fonts {
            match rx.try_recv() {
                Ok(names) => self.fonts = Fonts::Listed(names),
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30))
                }
                Err(mpsc::TryRecvError::Disconnected) => self.fonts = Fonts::Listed(Vec::new()),
            }
        }
        let Some(e) = &mut self.edited else {
            return;
        };
        let Some(pending) = &mut e.full_decode else {
            return;
        };
        let Some(result) = pending.poll() else {
            return;
        };
        e.full_decode = None;
        // A failed full decode leaves editing on the display pixels; Flatten (#34) reports it.
        if let Ok(pixels) = result {
            e.doc.set_full_resolution(Arc::new(pixels));
        }
        for op in std::mem::take(&mut e.queued) {
            op.apply(&mut e.doc);
        }
    }

    // ---- Property bar ----

    /// What the property bar edits: the selection's style and properties, or the active tool's.
    ///
    /// The style and the property list are borrowed, not cloned, every frame (#66): a single
    /// selection and the active tool hand back their own `Style` and `&'static` property list; only
    /// a multi-selection, whose property list is the intersection of its members', owns a `Vec`.
    pub fn property_target(&self) -> (Cow<'_, Style>, Cow<'static, [PropertyKind]>, bool) {
        if let Some(doc) = self.document()
            && let Some(first) = doc.selected().next()
        {
            let first_props = first.shape.properties();
            // Only a multi-selection needs an owned, intersected list; one Annotation borrows.
            let kinds = match doc.selected().nth(1) {
                None => Cow::Borrowed(first_props),
                Some(_) => {
                    let mut kinds = first_props.to_vec();
                    for other in doc.selected().skip(1) {
                        let theirs = other.shape.properties();
                        kinds.retain(|k| theirs.contains(k));
                    }
                    Cow::Owned(kinds)
                }
            };
            return (Cow::Borrowed(&first.style), kinds, true);
        }
        match self.styles.get(&self.tool) {
            Some(style) => (
                Cow::Borrowed(style),
                Cow::Borrowed(self.tool.properties()),
                false,
            ),
            None => (
                Cow::Owned(self.tool.default_style()),
                Cow::Borrowed(self.tool.properties()),
                false,
            ),
        }
    }

    /// Turn a property-bar change into a command, merging the changes of one widget interaction.
    pub fn property_change(&mut self, property: Property) -> crate::Command {
        let kind = property.kind();
        let merge = self.property_session
            == Some(PropertySession {
                kind,
                stepped: true,
            });
        self.property_session = Some(PropertySession {
            kind,
            stepped: true,
        });
        crate::Command::SetProperty { property, merge }
    }

    /// After the property bar: `active` is the property whose widget is still being interacted
    /// with (a colour picker open, a value being dragged), if any.
    pub fn property_interaction(&mut self, active: Option<PropertyKind>) {
        self.property_session = match (active, self.property_session) {
            (None, _) => None,
            (Some(kind), Some(s)) if s.kind == kind => Some(s),
            (Some(kind), _) => Some(PropertySession {
                kind,
                stepped: false,
            }),
        };
    }

    // ---- Canvas ----

    /// Handle the pointer on the canvas and say what to paint (§7.2). `pan` is the panning
    /// affordance (#43) the app reads from the pan gesture, for the grab cursor.
    pub fn canvas(
        &mut self,
        ctx: &egui::Context,
        layout: &Layout,
        response: &egui::Response,
        display: Option<DisplayPixels<'_>>,
        pointer_is_ours: bool,
        pan: PanCursor,
    ) -> EditorPaint {
        let revision = self.document().map(|d| d.revision());
        if self.can_edit() && (pointer_is_ours || self.pointer_down) {
            self.pointer(ctx, layout, response);
        }
        // The cursor (#48) reflects what a press would do now, from the state the pointer left.
        if let Some(icon) = self.cursor(layout, response, pan) {
            ctx.set_cursor_icon(icon);
        }
        let overlay = self.text_input(ctx, layout, response);
        if self.document().map(|d| d.revision()) != revision {
            ctx.request_repaint();
        }
        let composite = display.and_then(|d| self.update_composite(ctx, layout, d));
        // A lone line/arrow is reshaped by two endpoint handles, not the dashed box (#50); any
        // other selection keeps its bounding box.
        let endpoints = self
            .document()
            .and_then(select::line_endpoints)
            .map(|(from, to)| [layout.screen_point(from), layout.screen_point(to)]);
        let selection = if endpoints.is_some() {
            None
        } else {
            self.document()
                .and_then(|d| d.selection_bounds())
                .map(|r| layout.rect_to_screen(r))
        };
        self.selection_box = selection;
        let rubber_band = self
            .behaviour
            .rubber_band()
            .map(|r| layout.rect_to_screen(r));
        self.rubber_band = rubber_band;
        let (hovered_handle, hovered_end) = match self.hovered_grab(layout, response) {
            Some(select::Grab::Handle(h)) => (Some(h), None),
            Some(select::Grab::Endpoint(e)) => (None, Some(e)),
            None => (None, None),
        };
        EditorPaint {
            composite,
            selection,
            endpoints,
            overlay,
            crop: self.crop.map(|r| layout.rect_to_screen(r)),
            rubber_band,
            hovered_handle,
            hovered_end,
        }
    }

    /// The selection or crop handle under the pointer, for the hover highlight (#49). It delegates
    /// to [`Self::handle_under`], the one hit-test the cursor (#48) also uses, so the highlight and
    /// the cursor always agree on which handle the pointer is over. Only the tools whose press grabs
    /// a handle highlight one (Select for the selection, Crop for its region), matching what the
    /// cursor shows; purely presentational, so it changes no hit radius and nothing a press does.
    fn hovered_grab(&self, layout: &Layout, response: &egui::Response) -> Option<select::Grab> {
        if !self.can_edit() {
            return None;
        }
        let hover = response.hover_pos()?;
        if !layout.image.is_some_and(|image| image.contains(hover)) {
            return None;
        }
        if !matches!(self.tool, ToolId::Select | ToolId::Crop) {
            return None;
        }
        self.grab_under(layout.image_point(hover), layout.image_per_point())
    }

    /// What is under `p` (Image pixels): the crop region's handle when the Crop tool is active,
    /// else the Select tool's grab — a bounding-box handle, or an endpoint of a lone line/arrow
    /// (#50) — from [`select::grab_at`]. The single "what is under the pointer" query, shared by
    /// the cursor's decision (#48) and the hover highlight (#49) so both derive it the same way.
    /// `None` when nothing grabbable lies under the pointer.
    fn grab_under(&self, p: Point, image_per_point: f32) -> Option<select::Grab> {
        if self.tool == ToolId::Crop {
            return self
                .crop
                .and_then(|region| select::handle_at(region, p, image_per_point))
                .map(select::Grab::Handle);
        }
        select::grab_at(self.document()?, p, image_per_point)
    }

    /// The cursor to show this frame (#48, §4): a single decider driven by the same hit-testing the
    /// tools use — the handle-at, the Annotation hit-test, the active tool, the text-edit state and
    /// the panning affordance. The glyph answers "what would a press here do right now," so handles
    /// win over an Annotation body (as the Select tool's press does) and a drawing tool draws over
    /// everything. `None` leaves the chrome's own cursors (the tool buttons' hand) untouched.
    fn cursor(
        &self,
        layout: &Layout,
        response: &egui::Response,
        pan: PanCursor,
    ) -> Option<CursorIcon> {
        // A pan drag owns the pointer wherever it goes: the closed hand (story 8).
        if pan == PanCursor::Active {
            return Some(CursorIcon::Grabbing);
        }
        let hover = response.hover_pos()?;
        if !layout.image.is_some_and(|image| image.contains(hover)) {
            // Over the canvas but off the Image: a press does nothing special (story 11).
            return Some(CursorIcon::Default);
        }
        // Space over a pannable Image: the open hand that advertises panning (story 7, 9).
        if pan == PanCursor::Ready {
            return Some(CursorIcon::Grab);
        }
        // Without an editable Image the tools do nothing, so the plain arrow stands.
        let doc = match self.document() {
            Some(doc) if self.can_edit() => doc,
            _ => return Some(CursorIcon::Default),
        };
        let image_per_point = layout.image_per_point();
        let p = layout.image_point(hover);
        // An active text caret: the I-beam over its box (§4.4, story 6).
        if let Some(session) = &self.text
            && session.contains(doc, p, TEXT_SLOP * image_per_point)
        {
            return Some(CursorIcon::Text);
        }
        Some(match self.tool {
            ToolId::Select => self.select_cursor(doc, p, image_per_point),
            ToolId::Crop => self.crop_cursor(p, image_per_point),
            // Text places or edits a box: the I-beam, not a drawing crosshair (story 6).
            ToolId::Text => CursorIcon::Text,
            // Every shape-drawing tool draws over the canvas, Annotations included (story 5).
            ToolId::Rectangle
            | ToolId::Ellipse
            | ToolId::Line
            | ToolId::Arrow
            | ToolId::Freehand
            | ToolId::StepCounter
            | ToolId::Highlight
            | ToolId::Obfuscate => CursorIcon::Crosshair,
            // The Inset isn't in the tool strip (§4.6), so it is never the active tool.
            ToolId::Inset => CursorIcon::Default,
        })
    }

    /// The crop tool's cursor at `p` (story 10): its region's handles resize it (the same glyphs as
    /// the selection's), a press inside the region moves it, and a press elsewhere draws a new one.
    fn crop_cursor(&self, p: Point, image_per_point: f32) -> CursorIcon {
        if let Some(region) = self.crop
            && let Some(handle) = select::handle_at(region, p, image_per_point)
        {
            return resize_cursor(handle);
        }
        if let Some(region) = self.crop
            && region.contains(p)
        {
            return CursorIcon::Move;
        }
        CursorIcon::Crosshair
    }

    /// The Select tool's cursor at `p` (story 1–4, 11): a handle's resize glyph (handles win, as the
    /// press does), Move over the body of a selected Annotation, the pointing hand over a selectable
    /// one, and the plain arrow over empty space.
    fn select_cursor(&self, doc: &Document, p: Point, image_per_point: f32) -> CursorIcon {
        match select::grab_at(doc, p, image_per_point) {
            Some(select::Grab::Handle(handle)) => return resize_cursor(handle),
            Some(select::Grab::Endpoint(end)) => {
                let (from, to) = select::line_endpoints(doc)
                    .expect("an endpoint grab implies a lone line/arrow");
                return endpoint_cursor(from, to, end);
            }
            None => {}
        }
        match doc.hit_test(p, select::HIT_SLOP * image_per_point) {
            Some(id) if doc.is_selected(id) => CursorIcon::Move,
            Some(_) => CursorIcon::PointingHand,
            None => CursorIcon::Default,
        }
    }

    fn pointer(&mut self, ctx: &egui::Context, layout: &Layout, response: &egui::Response) {
        // A secondary (right) press on the canvas returns to the Select tool (#47, ADR 0006), acted
        // on the moment the button goes down so it feels instant. It wins over the tools' own press
        // handling below, so it is checked first; a following right-drag then carries no meaning.
        if response.contains_pointer()
            && ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary))
        {
            self.on_secondary_press();
            return;
        }
        let modifiers = ctx.input(|i| i.modifiers);
        let at = |pos: egui::Pos2| Pointer {
            pos: layout.image_point(pos),
            modifiers,
        };
        let press_origin = ctx.input(|i| i.pointer.press_origin());
        let current = response.interact_pointer_pos();

        // Text boxes (§4.4) take presses before the tools do.
        let press = if response.drag_started_by(egui::PointerButton::Primary) {
            press_origin
        } else if response.clicked() {
            current.or(press_origin)
        } else {
            None
        };
        if let Some(pos) = press
            && self.text_press(
                layout,
                at(pos).pos,
                response.clicked(),
                response.double_clicked(),
            )
        {
            return;
        }

        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(origin) = press_origin
        {
            self.pointer_down = true;
            self.with_tool(layout, |tool, cx| tool.press(cx, at(origin)));
        }
        if self.pointer_down
            && response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = current
        {
            self.with_tool(layout, |tool, cx| tool.drag(cx, at(pos)));
        }
        if self.pointer_down && response.drag_stopped_by(egui::PointerButton::Primary) {
            self.pointer_down = false;
            let pos = current
                .or_else(|| ctx.input(|i| i.pointer.latest_pos()))
                .unwrap_or_default();
            self.with_tool(layout, |tool, cx| tool.release(cx, at(pos)));
        }
        if response.clicked()
            && let Some(pos) = current.or(press_origin)
        {
            self.with_tool(layout, |tool, cx| {
                tool.press(cx, at(pos));
                tool.release(cx, at(pos));
            });
        }
    }

    /// A press on the canvas, as far as text is concerned (§4.4). Returns true when it was used.
    ///
    /// While editing, a press in the box moves the caret and a press elsewhere finishes (with the
    /// text tool that is all it does; other tools go on to handle it). The text tool's click edits
    /// the text Annotation under it or places a new box; a double-click on a text Annotation edits
    /// it (with the select tool too).
    fn text_press(&mut self, layout: &Layout, p: Point, clicked: bool, double: bool) -> bool {
        let slop = layout.image_per_point();
        let Some(e) = &mut self.edited else {
            return false;
        };
        if let Some(session) = &mut self.text {
            if session.contains(&e.doc, p, TEXT_SLOP * slop) {
                if clicked {
                    session.click(&e.doc, p);
                }
                return true;
            }
            self.finish_text();
            return self.tool == ToolId::Text;
        }
        let edits = (double && matches!(self.tool, ToolId::Select | ToolId::Text))
            || (clicked && self.tool == ToolId::Text);
        if !edits {
            return false;
        }
        self.list_fonts();
        self.start_full_decode();
        let style = self.tool_style(ToolId::Text);
        let Some(e) = &mut self.edited else {
            return false;
        };
        if let Some(id) = text_at(&e.doc, p, TEXT_SLOP * slop) {
            self.text = TextSession::edit(&mut e.doc, id);
            return true;
        }
        if self.tool == ToolId::Text && clicked {
            self.text = Some(TextSession::place(&mut e.doc, &style, p));
            return true;
        }
        false
    }

    /// Finish editing text, if a text box is being edited (before a save, #34).
    pub fn finish_text_editing(&mut self) {
        self.finish_text();
    }

    /// Finish the text session, if one is open: one History step (§4.8).
    fn finish_text(&mut self) {
        if let (Some(session), Some(e)) = (self.text.take(), &mut self.edited) {
            session.finish(&mut e.doc);
        }
    }

    /// While editing text: keep the keyboard on the canvas, apply this frame's typing and input
    /// method events, tell the platform where the caret is (for the input method's window), and
    /// return the caret to paint.
    fn text_input(
        &mut self,
        ctx: &egui::Context,
        layout: &Layout,
        response: &egui::Response,
    ) -> Vec<egui::Shape> {
        let id = response.id;
        let (Some(session), Some(e)) = (&mut self.text, &mut self.edited) else {
            if ctx.memory(|m| m.has_focus(id)) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
            return Vec::new();
        };
        ctx.memory_mut(|m| {
            // Take the keyboard back once nothing else wants it (after a property-bar click).
            if m.focused().is_none() {
                m.request_focus(id);
            }
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });
        if !ctx.memory(|m| m.has_focus(id)) {
            return Vec::new();
        }
        // Feed the events straight from the input lock, without cloning the whole list per frame.
        let (finish, had_events) =
            ctx.input(|i| (session.input(&mut e.doc, &i.events), !i.events.is_empty()));
        if finish {
            self.finish_text();
            ctx.memory_mut(|m| m.surrender_focus(id));
            ctx.request_repaint();
            return Vec::new();
        }
        if had_events {
            ctx.request_repaint();
        }
        let Some(annotation) = e.doc.annotation(session.id()) else {
            return Vec::new();
        };
        let colour = annotation.style.line;
        let colour = egui::Color32::from_rgb(colour.r, colour.g, colour.b);
        let bounds = layout.rect_to_screen(annotation.bounds());
        let Some(paint) = session.caret(&e.doc) else {
            return Vec::new();
        };
        let caret = layout.rect_to_screen(paint.caret);
        ctx.output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                purpose: egui::IMEPurpose::Normal,
                rect: bounds,
                cursor_rect: caret,
                should_interrupt_composition: false,
            });
        });
        let mut shapes = vec![egui::Shape::line_segment(
            [caret.center_top(), caret.center_bottom()],
            egui::Stroke::new(1.5, colour),
        )];
        if let Some(under) = paint.preedit {
            let under = layout.rect_to_screen(under);
            shapes.push(egui::Shape::line_segment(
                [under.left_bottom(), under.right_bottom()],
                egui::Stroke::new(1.0, colour),
            ));
        }
        shapes
    }

    fn with_tool(&mut self, layout: &Layout, f: impl FnOnce(&mut dyn Tool, &mut ToolContext<'_>)) {
        if self.tool != ToolId::Select {
            self.start_full_decode();
        }
        let Some(e) = &mut self.edited else {
            return;
        };
        let style = self
            .styles
            .get(&self.tool)
            .cloned()
            .unwrap_or_else(|| self.tool.default_style());
        let mut cx = ToolContext {
            doc: &mut e.doc,
            style: &style,
            image_per_point: layout.image_per_point(),
            crop: &mut self.crop,
        };
        f(self.behaviour.as_mut(), &mut cx);
    }

    /// Re-render the composite if the Document or the view changed. `None` while there are no
    /// Annotations and the Image is the decoded file: the plain Image texture is drawn then (§7.2).
    /// After an Image operation the composite always draws, from the Document's own pixels.
    fn update_composite(
        &mut self,
        ctx: &egui::Context,
        layout: &Layout,
        display: DisplayPixels<'_>,
    ) -> Option<(egui::TextureId, egui::Rect)> {
        let e = self.edited.as_mut()?;
        if e.doc.annotations().is_empty() && e.doc.is_original_image() {
            e.composite = None;
            return None;
        }
        let (dest, region, px) = layout.visible()?;
        let view = View {
            region,
            width: px.width,
            height: px.height,
        };
        // Zoomed in past the display-size decode, draw from the full-resolution pixels (§7.2).
        let display_density = display.pixels.width as f32 / e.doc.width() as f32;
        let wanted_density = px.width as f32 / region.width().max(1.0);
        let source = match e.doc.full_resolution() {
            Some(full) if wanted_density > display_density || !e.doc.is_original_image() => {
                Source {
                    key: source_key(display.key, e.doc.image_generation()),
                    pixels: full.as_ref(),
                }
            }
            _ => Source {
                key: display.key * 2,
                pixels: display.pixels,
            },
        };
        let revision = e.doc.revision();
        let key = (revision, source.key, view);
        let in_gesture = e.doc.in_gesture();

        // Already up to date? A gesture that ended with a frozen effect region still needs one full
        // re-render to correct it (the revision didn't change on commit), so don't short-circuit then.
        if let Some(c) = &e.composite {
            let correct_frozen = c.froze && !in_gesture;
            if c.key == key && !correct_frozen {
                return Some((c.texture.id(), c.dest));
            }
        }

        let renderer = self.renderer.get_or_insert_with(Renderer::new);
        let new_bounds: Vec<(AnnotationId, Rect)> = e
            .doc
            .annotations()
            .iter()
            .map(|a| (a.id, lecoq_render::drawn_bounds(a)))
            .collect();

        // The fast path (#46): during a gesture, when only the Document changed (same source and
        // view) and a previous composite is there to diff against, re-render and re-upload only the
        // region that changed. Outside a gesture (undo/redo, property bar, paste, zoom/pan, Image
        // ops) the full-region path below runs unchanged.
        let fast = in_gesture
            && e.composite
                .as_ref()
                .is_some_and(|c| c.key.1 == source.key && c.key.2 == view && c.key.0 != revision);
        if fast {
            let old = &e
                .composite
                .as_ref()
                .expect("fast path has a composite")
                .bounds;
            let dirty =
                dirty_region(old, &new_bounds).and_then(|d| device_rect(d, view).map(|s| (d, s)));
            let Some((dirty_image, sub)) = dirty else {
                // Nothing visible changed; keep the pixels, just mark the cache current.
                let c = e.composite.as_mut().expect("fast path has a composite");
                c.key = key;
                c.dest = dest;
                c.bounds = new_bounds;
                return Some((c.texture.id(), c.dest));
            };
            let sub_view = subview(view, sub);
            let mut sub_px = renderer.composite(&e.doc, &source, sub_view);
            // Opaque neighbourhood effects that only partly overlap the change can't be recomputed
            // from a slice, so freeze them: restore their pixels from the previous composite for
            // this frame and mark the composite so the gesture end corrects them. An effect fully
            // inside the dirty rectangle (e.g. the effect itself being moved) is recomputed live.
            let frozen: Vec<Rect> = e
                .doc
                .annotations()
                .iter()
                .filter(|a| lecoq_render::is_opaque_effect(a))
                .map(lecoq_render::drawn_bounds)
                .filter(|eb| overlaps_partially(eb, &dirty_image))
                .collect();
            let c = e.composite.as_mut().expect("fast path has a composite");
            // The freeze-restore and the write-back both work on the shared buffer's bytes
            // (#55). `make_mut` is free here: the texture manager consumed last frame's delta, so
            // the `Arc` is unique.
            let image = Arc::make_mut(&mut c.image);
            let full_width = image.size[0];
            for eb in &frozen {
                restore_region(&mut sub_px, image.as_raw(), full_width, *eb, view, sub);
            }
            // Write the sub-rect back into the cached pixels so the cache stays consistent, and
            // upload only that sub-rect to the texture.
            blit(image.as_raw_mut(), full_width, &sub_px, sub);
            let sub_image = egui::ColorImage::from_rgba_premultiplied(
                [sub_px.width as usize, sub_px.height as usize],
                &sub_px.rgba_premultiplied,
            );
            c.texture
                .set_partial([sub.x0, sub.y0], sub_image, egui::TextureOptions::LINEAR);
            c.key = key;
            c.dest = dest;
            c.bounds = new_bounds;
            c.froze |= !frozen.is_empty();
            return Some((c.texture.id(), c.dest));
        }

        // Full path: render the whole visible region straight into the texture's buffer (#55) and
        // reset the gesture's dirty-tracking state. The buffer is kept between frames; a view of the
        // same size reuses the allocation, a changed size reallocates. A fresh allocation is
        // transparent, so the renderer skips the clear; a reused one is cleared first.
        let size = [view.width.max(1) as usize, view.height.max(1) as usize];
        match &mut e.composite {
            Some(c) => {
                let reallocated = c.image.size != size;
                if reallocated {
                    c.image = Arc::new(egui::ColorImage::filled(size, egui::Color32::TRANSPARENT));
                }
                let image = Arc::make_mut(&mut c.image);
                renderer.composite_into(&e.doc, &source, view, image.as_raw_mut(), reallocated);
                c.texture
                    .set(Arc::clone(&c.image), egui::TextureOptions::LINEAR);
                c.key = key;
                c.dest = dest;
                c.bounds = new_bounds;
                c.froze = false;
            }
            None => {
                let mut image = egui::ColorImage::filled(size, egui::Color32::TRANSPARENT);
                renderer.composite_into(&e.doc, &source, view, image.as_raw_mut(), true);
                let image = Arc::new(image);
                let texture = ctx.load_texture(
                    "composite",
                    Arc::clone(&image),
                    egui::TextureOptions::LINEAR,
                );
                e.composite = Some(CompositeTexture {
                    key,
                    texture,
                    dest,
                    image,
                    bounds: new_bounds,
                    froze: false,
                });
            }
        }
        let c = e.composite.as_ref()?;
        Some((c.texture.id(), c.dest))
    }
}

/// A rectangle of composite (device) pixels: `[x0, x1) × [y0, y1)`.
#[derive(Clone, Copy)]
struct SubRect {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

/// The region that changed between two sets of Annotation drawn bounds (Image pixels): the union of
/// the bounds of every Annotation added, removed, or whose drawn bounds moved. `None` when nothing
/// changed (#46).
fn dirty_region(old: &[(AnnotationId, Rect)], new: &[(AnnotationId, Rect)]) -> Option<Rect> {
    let mut dirty: Option<Rect> = None;
    let mut mark = |r: Rect| dirty = Some(dirty.map_or(r, |d| d.union(&r)));
    for (id, nb) in new {
        match old.iter().find(|(oid, _)| oid == id) {
            Some((_, ob)) if ob == nb => {}
            Some((_, ob)) => {
                mark(*ob);
                mark(*nb);
            }
            None => mark(*nb),
        }
    }
    for (id, ob) in old {
        if !new.iter().any(|(nid, _)| nid == id) {
            mark(*ob);
        }
    }
    dirty
}

/// `effect` overlaps `dirty` but isn't wholly inside it: a sliced recompute of the effect would be
/// wrong, so it must be frozen for the frame. (Wholly inside → recompute live; disjoint → untouched.)
fn overlaps_partially(effect: &Rect, dirty: &Rect) -> bool {
    let contained = dirty.min.x <= effect.min.x
        && dirty.min.y <= effect.min.y
        && dirty.max.x >= effect.max.x
        && dirty.max.y >= effect.max.y;
    effect.intersect(dirty).is_some() && !contained
}

/// The composite (device) pixels an Image-pixel rectangle maps to, aligned to the pixel grid and
/// clamped to the view. `None` when it falls outside the visible region.
fn device_rect(r: Rect, view: View) -> Option<SubRect> {
    let (sx, sy) = view_scale(view);
    let w = view.width as f32;
    let h = view.height as f32;
    let x0 = (((r.min.x - view.region.min.x) * sx).floor()).clamp(0.0, w);
    let x1 = (((r.max.x - view.region.min.x) * sx).ceil()).clamp(0.0, w);
    let y0 = (((r.min.y - view.region.min.y) * sy).floor()).clamp(0.0, h);
    let y1 = (((r.max.y - view.region.min.y) * sy).ceil()).clamp(0.0, h);
    (x1 > x0 && y1 > y0).then_some(SubRect {
        x0: x0 as usize,
        y0: y0 as usize,
        x1: x1 as usize,
        y1: y1 as usize,
    })
}

/// The View for a pixel-grid-aligned sub-rectangle of `view`, so the sub-render's pixels land
/// exactly where the full render's would (#46).
fn subview(view: View, sub: SubRect) -> View {
    let (sx, sy) = view_scale(view);
    let region = Rect::from_min_size(
        Point::new(
            view.region.min.x + sub.x0 as f32 / sx,
            view.region.min.y + sub.y0 as f32 / sy,
        ),
        (sub.x1 - sub.x0) as f32 / sx,
        (sub.y1 - sub.y0) as f32 / sy,
    );
    View {
        region,
        width: (sub.x1 - sub.x0) as u32,
        height: (sub.y1 - sub.y0) as u32,
    }
}

/// Composite (device) pixels per Image pixel, as [`lecoq_render::Renderer::composite`] scales.
fn view_scale(view: View) -> (f32, f32) {
    (
        view.width as f32 / view.region.width().max(f32::EPSILON),
        view.height as f32 / view.region.height().max(f32::EPSILON),
    )
}

/// Copy `src` (sized to `at`) into `dst` at `at`'s top-left corner. `dst` is the full composite's
/// RGBA bytes, `dst_width` pixels wide; both are RGBA, row-major (#55: `dst` is the shared buffer).
fn blit(dst: &mut [u8], dst_width: usize, src: &Composite, at: SubRect) {
    let sw = src.width as usize;
    let len = sw * 4;
    for j in 0..(at.y1 - at.y0) {
        let drow = ((at.y0 + j) * dst_width + at.x0) * 4;
        let srow = j * len;
        dst[drow..drow + len].copy_from_slice(&src.rgba_premultiplied[srow..srow + len]);
    }
}

/// Restore the pixels of `region` (Image pixels) in the freshly rendered `sub` from the `previous`
/// full composite's RGBA bytes (`previous_width` pixels wide) — the freeze that keeps an opaque
/// effect intact for a frame (#46). `sub` covers the device rectangle `at`; only the part of
/// `region` inside it is restored.
fn restore_region(
    sub: &mut Composite,
    previous: &[u8],
    previous_width: usize,
    region: Rect,
    view: View,
    at: SubRect,
) {
    let Some(d) = device_rect(region, view) else {
        return;
    };
    let x0 = d.x0.max(at.x0);
    let y0 = d.y0.max(at.y0);
    let x1 = d.x1.min(at.x1);
    let y1 = d.y1.min(at.y1);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let sw = sub.width as usize;
    let len = (x1 - x0) * 4;
    for y in y0..y1 {
        let prow = (y * previous_width + x0) * 4;
        let srow = ((y - at.y0) * sw + (x0 - at.x0)) * 4;
        sub.rgba_premultiplied[srow..srow + len].copy_from_slice(&previous[prow..prow + len]);
    }
}

/// An Image operation (§4.7), as the editor queues it until the full-resolution pixels arrive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageOp {
    /// Crop to this region (Image pixels).
    Crop(Rect),
    Rotate {
        clockwise: bool,
    },
    Flip {
        horizontal: bool,
    },
    /// Resize to this many pixels (Mitchell cubic up, mipmap + cubic down).
    Resize {
        width: u32,
        height: u32,
    },
}

impl ImageOp {
    fn apply(self, doc: &mut Document) -> bool {
        match self {
            ImageOp::Crop(region) => doc.crop(region),
            ImageOp::Rotate { clockwise } => doc.rotate(clockwise),
            ImageOp::Flip { horizontal } => doc.flip(horizontal),
            ImageOp::Resize { width, height } => doc.resize(width, height, lecoq_render::resample),
        }
    }
}

/// The one mapping from a handle to its resize glyph, shared by the selection and crop handles
/// (#48): a diagonal for each corner, horizontal for the side handles and vertical for the ends.
fn resize_cursor(handle: Handle) -> CursorIcon {
    match handle {
        Handle::TopLeft | Handle::BottomRight => CursorIcon::ResizeNwSe,
        Handle::TopRight | Handle::BottomLeft => CursorIcon::ResizeNeSw,
        Handle::Left | Handle::Right => CursorIcon::ResizeHorizontal,
        Handle::Top | Handle::Bottom => CursorIcon::ResizeVertical,
    }
}

/// The resize glyph for a lone line/arrow's endpoint handle (#50): one of the four the box handles
/// use, chosen from the segment's direction at `end`, so dragging the end reads as a resize
/// pointing the same way. (Image y grows down, so a down-right segment is the "⤡" NW–SE diagonal.)
fn endpoint_cursor(from: Point, to: Point, end: End) -> CursorIcon {
    let (tip, other) = match end {
        End::From => (from, to),
        End::To => (to, from),
    };
    let v = tip - other;
    // Fold onto a half-circle (0..180): a direction and its opposite resize the same way.
    let degrees = v.y.atan2(v.x).to_degrees().rem_euclid(180.0);
    if !(22.5..157.5).contains(&degrees) {
        CursorIcon::ResizeHorizontal
    } else if degrees < 67.5 {
        CursorIcon::ResizeNwSe
    } else if degrees < 112.5 {
        CursorIcon::ResizeVertical
    } else {
        CursorIcon::ResizeNeSw
    }
}

/// The renderer's cache key for the full-resolution pixels: unique per decode and per Image
/// operation result (display-size decodes use even keys).
fn source_key(display_key: u64, generation: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (display_key, generation).hash(&mut h);
    h.finish() | 1
}
