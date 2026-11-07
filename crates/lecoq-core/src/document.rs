//! The Document: the open Image, its Annotations, the Selection and the History.
//!
//! Every change goes through a step, so it lands in the History:
//! - a gesture (a drag that draws, moves or resizes): [`Document::begin_gesture`], then any number
//!   of edits while the pointer moves, then [`Document::commit_gesture`] (or
//!   [`Document::cancel_gesture`]);
//! - a one-shot edit (delete, paste, duplicate, …): [`Document::step`] wraps a closure in a
//!   gesture;
//! - a property change: [`Document::set_property`];
//! - an Image operation (§4.7): [`Document::crop`], or [`Document::image_operation`] with new
//!   pixels and an [`ImageTransform`] (see `image_op`). Its step also stores the Image.
//!
//! Edit methods ([`Document::add`], [`Document::annotation_mut`], [`Document::remove`], …) may only
//! be called inside a gesture; they panic otherwise, so no edit can bypass the History.

use std::sync::Arc;

use crate::annotation::{Annotation, AnnotationId, AnnotationShape, Shape};
use crate::clipboard::CopiedAnnotation;
use crate::geometry::{Affine, Point, Rect, Vector};
use crate::history::{HISTORY_PIXEL_CAP, History, ImageState, Snapshot, Step, StepKind};
use crate::image_op::{ImageTransform, PixelRect};
use crate::style::{Property, Style};

/// A move in the z-order (§4.2), for the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restack {
    /// Bring to front: above every other Annotation.
    Front,
    /// Send to back: below every other Annotation.
    Back,
    /// One level up (Ctrl+]).
    Up,
    /// One level down (Ctrl+[).
    Down,
}

/// Image pixels: 8-bit sRGB, tightly packed, row-major. The one pixel buffer the codec decodes
/// into, the render crate draws from, and the app wraps for egui (#64). Full-resolution pixels are
/// straight ([`PixelData::Rgb`] or [`PixelData::Rgba`]); display-size pixels are premultiplied
/// ([`PixelData::RgbaPremultiplied`]), ready for egui and the screen composite.
#[derive(Clone)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub data: PixelData,
}

/// The channel layout of [`Pixels`].
#[derive(Clone)]
pub enum PixelData {
    /// Opaque images.
    Rgb(Vec<u8>),
    /// Images with an alpha channel, not premultiplied (straight): what the encoders take.
    Rgba(Vec<u8>),
    /// RGBA with premultiplied alpha, the layout egui and the Skia screen composite keep (§7.2).
    RgbaPremultiplied(Vec<u8>),
}

impl PixelData {
    /// `data` wrapped in the same layout variant as `self`: how [`Pixels::crop`] and the rotate/flip
    /// remap keep an operation's output in the input's layout.
    pub(crate) fn same_layout(&self, data: Vec<u8>) -> PixelData {
        match self {
            PixelData::Rgb(_) => PixelData::Rgb(data),
            PixelData::Rgba(_) => PixelData::Rgba(data),
            PixelData::RgbaPremultiplied(_) => PixelData::RgbaPremultiplied(data),
        }
    }
}

impl Pixels {
    /// The same pixels with premultiplied alpha: the display-size, upload-ready layout (#64, §7.2).
    /// RGB gains an opaque alpha; straight RGBA is premultiplied exactly the way egui premultiplies
    /// a `ColorImage` (`from_rgba_unmultiplied`), so the display texture and the Skia source agree
    /// to the byte. Already-premultiplied pixels are returned unchanged.
    pub fn premultiplied(&self) -> Pixels {
        let data = match &self.data {
            PixelData::Rgb(rgb) => PixelData::RgbaPremultiplied(
                rgb.chunks_exact(3)
                    .flat_map(|px| [px[0], px[1], px[2], 255])
                    .collect(),
            ),
            PixelData::Rgba(rgba) => PixelData::RgbaPremultiplied(
                rgba.chunks_exact(4)
                    .flat_map(|px| {
                        let a = px[3];
                        match a {
                            0 => [0, 0, 0, 0],
                            255 => [px[0], px[1], px[2], 255],
                            a => [
                                mul_frac_round(px[0], a),
                                mul_frac_round(px[1], a),
                                mul_frac_round(px[2], a),
                                a,
                            ],
                        }
                    })
                    .collect(),
            ),
            PixelData::RgbaPremultiplied(_) => return self.clone(),
        };
        Pixels {
            width: self.width,
            height: self.height,
            data,
        }
    }

    /// The same pixels with straight (non-premultiplied) alpha, what the encoders take (§5.3):
    /// premultiplied pixels are un-premultiplied, becoming [`PixelData::Rgb`] when every pixel is
    /// opaque and straight [`PixelData::Rgba`] otherwise. Straight pixels are returned unchanged.
    pub fn straight(&self) -> Pixels {
        let premultiplied = match &self.data {
            PixelData::RgbaPremultiplied(data) => data,
            PixelData::Rgb(_) | PixelData::Rgba(_) => return self.clone(),
        };
        let data = if premultiplied.chunks_exact(4).all(|px| px[3] == 255) {
            PixelData::Rgb(
                premultiplied
                    .chunks_exact(4)
                    .flat_map(|px| [px[0], px[1], px[2]])
                    .collect(),
            )
        } else {
            let mut rgba = premultiplied.clone();
            for px in rgba.chunks_exact_mut(4) {
                let a = u32::from(px[3]);
                if a != 0 && a != 255 {
                    for c in &mut px[..3] {
                        *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                    }
                }
            }
            PixelData::Rgba(rgba)
        };
        Pixels {
            width: self.width,
            height: self.height,
            data,
        }
    }
}

/// `val * (frac / 255)`, rounded, with no division: egui's own premultiply (`ecolor`'s
/// `mul_frac_round`), replicated here so premultiplied Image pixels match egui's to the byte.
fn mul_frac_round(val: u8, frac: u8) -> u8 {
    let p = (val as u16) * (frac as u16) + 128;
    ((p + (p >> 8)) >> 8) as u8
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pixels({}×{})", self.width, self.height)
    }
}

#[derive(Debug)]
pub struct Document {
    width: u32,
    height: u32,
    /// The full-resolution pixels, once decoded (§4.7: decoded when editing starts).
    full_resolution: Option<Arc<Pixels>>,
    /// Which pixels the Image has: 0 for the decoded file, a new number per Image operation.
    image_generation: u64,
    next_image_generation: u64,
    /// The cap on pixels held by the History, in bytes (§4.8).
    history_pixel_cap: usize,
    /// Bottom to top: the last one is drawn on top.
    annotations: Vec<Annotation>,
    selection: Vec<AnnotationId>,
    history: History,
    next_id: u64,
    /// The state when the open gesture began.
    gesture: Option<Snapshot>,
    /// The current committed Annotations behind a shared pointer, kept equal to `annotations`
    /// whenever no gesture is open. A step's `before` reuses this `Arc`, and its `after` becomes
    /// the next step's `before`, so consecutive steps share one copy (§4.8, issue #66).
    snapshot_annotations: Arc<[Annotation]>,
    /// Bumped on every visible change, so renderers know when to redraw.
    revision: u64,
}

impl Document {
    /// A Document for an Image of `width`×`height` full-resolution pixels, with no Annotations.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            full_resolution: None,
            image_generation: 0,
            next_image_generation: 1,
            history_pixel_cap: HISTORY_PIXEL_CAP,
            annotations: Vec::new(),
            selection: Vec::new(),
            history: History::new(),
            next_id: 1,
            gesture: None,
            snapshot_annotations: Arc::from([]),
            revision: 0,
        }
    }

    // ---- The Image ----

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The Image's bounds in Image pixels.
    pub fn bounds(&self) -> Rect {
        Rect::from_min_size(Point::default(), self.width as f32, self.height as f32)
    }

    pub fn full_resolution(&self) -> Option<&Arc<Pixels>> {
        self.full_resolution.as_ref()
    }

    /// The decoded file's full-resolution pixels have arrived.
    pub fn set_full_resolution(&mut self, pixels: Arc<Pixels>) {
        if self.image_generation == 0 {
            self.full_resolution = Some(pixels);
        }
    }

    /// Are the Image's pixels still the decoded file's (no Image operation in effect)?
    pub fn is_original_image(&self) -> bool {
        self.image_generation == 0
    }

    /// Identifies the Image's current pixels: changes with every Image operation, undo and redo of
    /// one (a renderer cache key). 0 is the decoded file.
    pub fn image_generation(&self) -> u64 {
        self.image_generation
    }

    /// Change the cap on pixels the History holds (default [`HISTORY_PIXEL_CAP`]).
    pub fn set_history_pixel_cap(&mut self, bytes: usize) {
        self.history_pixel_cap = bytes;
    }

    // ---- Image operations (§4.7, ADR 0001; see `image_op`) ----

    /// Crop the Image to `region` (rounded to whole pixels, clamped to the Image) as one step.
    /// Needs the full-resolution pixels; returns false without them or for an empty region.
    pub fn crop(&mut self, region: Rect) -> bool {
        let Some(full) = &self.full_resolution else {
            return false;
        };
        let Some(r) = PixelRect::within(region, self.width, self.height) else {
            return false;
        };
        let pixels = full.crop(r);
        let origin = r.to_rect().min;
        self.image_operation(pixels, ImageTransform::Crop { origin });
        true
    }

    /// Turn the Image a quarter, clockwise or not, as one step. Needs the full-resolution pixels;
    /// returns false without them.
    pub fn rotate(&mut self, clockwise: bool) -> bool {
        let Some(full) = &self.full_resolution else {
            return false;
        };
        let pixels = full.rotate90(clockwise);
        let (width, height) = (self.width as f32, self.height as f32);
        self.image_operation(
            pixels,
            ImageTransform::Rotate90 {
                clockwise,
                width,
                height,
            },
        );
        true
    }

    /// Mirror the Image (left-right when `horizontal`, else top-bottom) as one step. Needs the
    /// full-resolution pixels; returns false without them.
    pub fn flip(&mut self, horizontal: bool) -> bool {
        let Some(full) = &self.full_resolution else {
            return false;
        };
        let pixels = full.flip(horizontal);
        let (width, height) = (self.width as f32, self.height as f32);
        self.image_operation(
            pixels,
            ImageTransform::Flip {
                horizontal,
                width,
                height,
            },
        );
        true
    }

    /// Resize the Image to `width`×`height` as one step; `resample` computes the new pixels (Skia,
    /// in `lecoq-render`). Annotations scale with it, line thickness included. Needs the
    /// full-resolution pixels; returns false without them, for an empty size, or for the current
    /// size (nothing to do).
    pub fn resize(
        &mut self,
        width: u32,
        height: u32,
        resample: impl FnOnce(&Pixels, u32, u32) -> Pixels,
    ) -> bool {
        let Some(full) = &self.full_resolution else {
            return false;
        };
        if width == 0 || height == 0 || (width, height) == (self.width, self.height) {
            return false;
        }
        let pixels = resample(full, width, height);
        assert_eq!((pixels.width, pixels.height), (width, height));
        let sx = width as f32 / self.width as f32;
        let sy = height as f32 / self.height as f32;
        self.image_operation(pixels, ImageTransform::Resize { sx, sy });
        true
    }

    /// Replace the Image with `pixels` (the result of an Image operation), move every Annotation
    /// through `transform`, and remove those left entirely outside the new Image. One History step
    /// that stores the pixels and Annotations from before; the History's pixel cap is enforced.
    pub fn image_operation(&mut self, pixels: Pixels, transform: ImageTransform) {
        assert!(self.gesture.is_none(), "image operation inside a gesture");
        let before = Snapshot {
            image: self.image_state(),
            ..self.baseline_snapshot()
        };
        self.width = pixels.width;
        self.height = pixels.height;
        self.full_resolution = Some(Arc::new(pixels));
        self.image_generation = self.next_image_generation;
        self.next_image_generation += 1;
        let bounds = self.bounds();
        for a in &mut self.annotations {
            a.image_transform(&transform);
        }
        self.annotations
            .retain(|a| a.extent().intersect(&bounds).is_some());
        let annotations = &self.annotations;
        self.selection
            .retain(|id| annotations.iter().any(|a| a.id == *id));
        self.revision += 1;
        let after = Snapshot {
            image: self.image_state(),
            ..self.snapshot_after()
        };
        self.history.push(Step {
            kind: StepKind::ImageOperation,
            before,
            after,
        });
        self.history
            .enforce_pixel_cap(self.history_pixel_cap, self.full_resolution.as_ref());
    }

    /// A copy of what is drawn (the size, the pixels and the Annotations), with no Selection and
    /// no History: what a worker thread Flattens (§7.2) while editing goes on.
    pub fn to_flatten(&self) -> Document {
        Document {
            full_resolution: self.full_resolution.clone(),
            image_generation: self.image_generation,
            annotations: self.annotations.clone(),
            revision: self.revision,
            ..Document::new(self.width, self.height)
        }
    }

    // ---- Reading ----

    /// Bottom to top.
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    pub fn annotation(&self, id: AnnotationId) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.id == id)
    }

    /// The topmost Annotation under `p`.
    pub fn hit_test(&self, p: Point, tolerance: f32) -> Option<AnnotationId> {
        self.annotations
            .iter()
            .rev()
            .find(|a| a.hit(p, tolerance))
            .map(|a| a.id)
    }

    /// A Step counter's number (§4.5): its rank, from 1, among the Step counters in creation
    /// order. Ids grow with creation and are never reused, so this is derived, not stored: deleting
    /// one renumbers the rest, and undo restores the old numbering. `None` for other Annotations.
    pub fn step_number(&self, id: AnnotationId) -> Option<u32> {
        let is_counter = |a: &Annotation| matches!(a.shape, AnnotationShape::StepCounter(_));
        self.annotation(id).filter(|a| is_counter(a))?;
        let earlier = self
            .annotations
            .iter()
            .filter(|a| is_counter(a) && a.id <= id)
            .count();
        Some(earlier as u32)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    // ---- Selection (not a History step on its own) ----

    pub fn selection(&self) -> &[AnnotationId] {
        &self.selection
    }

    pub fn is_selected(&self, id: AnnotationId) -> bool {
        self.selection.contains(&id)
    }

    pub fn select(&mut self, ids: impl IntoIterator<Item = AnnotationId>) {
        self.selection = ids
            .into_iter()
            .filter(|id| self.annotation(*id).is_some())
            .collect();
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// The selected Annotations, bottom to top.
    pub fn selected(&self) -> impl Iterator<Item = &Annotation> {
        self.annotations
            .iter()
            .filter(|a| self.selection.contains(&a.id))
    }

    /// The bounds of the whole selection: what the dashed box and its handles surround.
    pub fn selection_bounds(&self) -> Option<Rect> {
        self.selected()
            .map(|a| a.bounds())
            .reduce(|a, b| a.union(&b))
    }

    // ---- Steps ----

    pub fn in_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    /// Start a gesture: the edits until [`Self::commit_gesture`] become one History step.
    pub fn begin_gesture(&mut self) {
        assert!(self.gesture.is_none(), "a gesture is already open");
        self.gesture = Some(self.baseline_snapshot());
    }

    /// End the gesture. Records a step if anything changed; returns whether it did.
    pub fn commit_gesture(&mut self) -> bool {
        let before = self.gesture.take().expect("no open gesture");
        if *before.annotations == *self.annotations {
            return false;
        }
        let after = self.snapshot_after();
        self.history.push(Step {
            kind: StepKind::Gesture,
            before,
            after,
        });
        true
    }

    /// Abandon the gesture and put everything back as it was.
    pub fn cancel_gesture(&mut self) {
        let before = self.gesture.take().expect("no open gesture");
        self.restore(&before);
    }

    /// Run `edit` as one step.
    pub fn step<R>(&mut self, edit: impl FnOnce(&mut Self) -> R) -> R {
        self.begin_gesture();
        let r = edit(self);
        self.commit_gesture();
        r
    }

    // ---- Edits (inside a gesture) ----

    /// Add an Annotation on top of the others; returns its id.
    pub fn add(&mut self, shape: impl Into<AnnotationShape>, style: Style) -> AnnotationId {
        self.assert_in_gesture();
        let id = AnnotationId(self.next_id);
        self.next_id += 1;
        let mut annotation = Annotation {
            id,
            shape: shape.into(),
            style,
        };
        annotation.relayout();
        self.annotations.push(annotation);
        self.revision += 1;
        id
    }

    pub fn annotation_mut(&mut self, id: AnnotationId) -> Option<&mut Annotation> {
        self.assert_in_gesture();
        self.revision += 1;
        self.annotations.iter_mut().find(|a| a.id == id)
    }

    /// Remove these Annotations (and drop them from the selection).
    pub fn remove(&mut self, ids: &[AnnotationId]) {
        self.assert_in_gesture();
        self.annotations.retain(|a| !ids.contains(&a.id));
        self.selection.retain(|id| !ids.contains(id));
        self.revision += 1;
    }

    // ---- Edits (each one step) ----

    /// Delete the selected Annotations as one step. Returns false when nothing was selected.
    pub fn delete_selection(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let ids = self.selection.clone();
        self.step(|doc| doc.remove(&ids));
        true
    }

    /// Duplicate the selected Annotations as one step (Ctrl+D, §4.2): the copies go on top, in the
    /// same order, moved by `offset`, and become the selection. Returns false when nothing was
    /// selected.
    pub fn duplicate_selection(&mut self, offset: Vector) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let t = Affine::translate(offset);
        let copies: Vec<(AnnotationShape, Style)> = self
            .selected()
            .map(|a| (a.shape.transformed(&t), a.style.clone()))
            .collect();
        self.step(|doc| {
            let ids: Vec<AnnotationId> = copies
                .into_iter()
                .map(|(shape, style)| doc.add(shape, style))
                .collect();
            doc.select(ids);
        });
        true
    }

    /// Paste copied Annotations as one step (§6.3): they go on top, in their order, and become the
    /// selection. While they would land exactly on Annotations already there (same type, same
    /// bounds: pasting back onto the Image they were copied from), they move by `offset` each
    /// time, so repeated pastes cascade. Returns false when there was nothing to paste.
    pub fn paste(&mut self, copied: Vec<CopiedAnnotation>, offset: Vector) -> bool {
        if copied.is_empty() || self.gesture.is_some() {
            return false;
        }
        let mut pasted: Vec<Annotation> = copied
            .into_iter()
            .map(|c| {
                let mut a = Annotation {
                    id: AnnotationId(0),
                    shape: c.shape,
                    style: c.style,
                };
                a.relayout();
                a
            })
            .collect();
        let t = Affine::translate(offset);
        // Bounded: a cascade can't run forever, however many copies are stacked.
        for _ in 0..1000 {
            if !pasted.iter().all(|p| self.has_one_like(p)) {
                break;
            }
            for p in &mut pasted {
                p.shape.transform(&t);
            }
        }
        self.step(|doc| {
            let ids: Vec<AnnotationId> = pasted
                .into_iter()
                .map(|a| doc.add(a.shape, a.style))
                .collect();
            doc.select(ids);
        });
        true
    }

    /// Is there an Annotation of the same type exactly where `like` is?
    fn has_one_like(&self, like: &Annotation) -> bool {
        self.annotations.iter().any(|a| {
            std::mem::discriminant(&a.shape) == std::mem::discriminant(&like.shape)
                && a.bounds() == like.bounds()
        })
    }

    /// Change the selection's place in the z-order as one step (§4.2). Nothing changes, and no
    /// step is recorded, when the selection is already there.
    pub fn restack(&mut self, restack: Restack) {
        if self.selection.is_empty() || self.gesture.is_some() {
            return;
        }
        self.step(|doc| {
            // Bottom to top: is each Annotation selected? Kept in step with the swaps below.
            let mut order: Vec<(bool, Annotation)> = std::mem::take(&mut doc.annotations)
                .into_iter()
                .map(|a| (doc.selection.contains(&a.id), a))
                .collect();
            let n = order.len();
            match restack {
                // A stable sort keeps the relative order within each group.
                Restack::Front => order.sort_by_key(|(selected, _)| *selected),
                Restack::Back => order.sort_by_key(|(selected, _)| !*selected),
                // Each selected Annotation swaps with the unselected one just above (below) it.
                Restack::Up => {
                    for i in (0..n.saturating_sub(1)).rev() {
                        if order[i].0 && !order[i + 1].0 {
                            order.swap(i, i + 1);
                        }
                    }
                }
                Restack::Down => {
                    for i in 1..n {
                        if order[i].0 && !order[i - 1].0 {
                            order.swap(i, i - 1);
                        }
                    }
                }
            }
            doc.annotations = order.into_iter().map(|(_, a)| a).collect();
            doc.revision += 1;
        });
    }

    /// Restyle the selected Annotations as one step. With `merge`, the change is folded into the
    /// previous step when that was a change of the same property on the same Annotations (one
    /// widget interaction, e.g. dragging in a colour picker, is one step). Returns false when
    /// nothing was selected.
    pub fn set_property(&mut self, property: Property, merge: bool) -> bool {
        assert!(self.gesture.is_none(), "set_property inside a gesture");
        if self.selection.is_empty() {
            return false;
        }
        let kind = StepKind::Property {
            kind: property.kind(),
            targets: self.selection.clone(),
        };
        let before = self.baseline_snapshot();
        let selection = self.selection.clone();
        for a in &mut self.annotations {
            if selection.contains(&a.id) {
                a.style.set(property.clone());
                a.relayout();
            }
        }
        self.revision += 1;
        if *before.annotations == *self.annotations {
            return true;
        }
        let after = self.snapshot_after();
        if merge && let Some(last) = self.history.last_mut().filter(|s| s.kind == kind) {
            last.after = after;
            return true;
        }
        self.history.push(Step {
            kind,
            before,
            after,
        });
        true
    }

    // ---- History ----

    pub fn undo(&mut self) -> bool {
        if self.gesture.is_some() {
            return false;
        }
        let Some(before) = self.history.undo().map(|s| s.before.clone()) else {
            return false;
        };
        self.restore(&before);
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.gesture.is_some() {
            return false;
        }
        let Some(after) = self.history.redo().map(|s| s.after.clone()) else {
            return false;
        };
        self.restore(&after);
        true
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// How many steps can be undone.
    pub fn undo_len(&self) -> usize {
        self.history.undo_len()
    }

    /// How many steps can be redone.
    pub fn redo_len(&self) -> usize {
        self.history.redo_len()
    }

    /// Unsaved changes: the state differs from the one last saved (or opened).
    pub fn is_modified(&self) -> bool {
        self.history.is_modified()
    }

    /// Record that the current state was saved (§4.8: saving doesn't clear the History).
    pub fn mark_saved(&mut self) {
        self.history.mark_saved();
    }

    /// A step's `before`: reuses the shared baseline `Arc` for the Annotations (no copy). Valid
    /// only at a committed boundary, where the baseline equals the working `annotations` — every
    /// caller takes it before editing.
    fn baseline_snapshot(&self) -> Snapshot {
        Snapshot {
            annotations: Arc::clone(&self.snapshot_annotations),
            selection: self.selection.clone(),
            image: None,
        }
    }

    /// A step's `after`: a fresh shared copy of the current Annotations, which also becomes the new
    /// baseline so the next step's `before` shares this very `Arc`.
    fn snapshot_after(&mut self) -> Snapshot {
        let annotations: Arc<[Annotation]> = Arc::from(self.annotations.as_slice());
        self.snapshot_annotations = Arc::clone(&annotations);
        Snapshot {
            annotations,
            selection: self.selection.clone(),
            image: None,
        }
    }

    /// The Image as the current Document holds it, for an Image-operation step's snapshots.
    fn image_state(&self) -> Option<ImageState> {
        let pixels = self
            .full_resolution
            .clone()
            .expect("Image operations need the full-resolution pixels");
        Some(ImageState {
            width: self.width,
            height: self.height,
            pixels,
            generation: self.image_generation,
        })
    }

    fn restore(&mut self, s: &Snapshot) {
        self.annotations = s.annotations.to_vec();
        self.snapshot_annotations = Arc::clone(&s.annotations);
        self.selection = s.selection.clone();
        if let Some(image) = &s.image {
            self.width = image.width;
            self.height = image.height;
            self.full_resolution = Some(image.pixels.clone());
            self.image_generation = image.generation;
        }
        self.revision += 1;
    }

    fn assert_in_gesture(&self) {
        assert!(
            self.gesture.is_some(),
            "Document edits must be inside a step"
        );
    }
}

#[cfg(test)]
mod pixel_layout_tests {
    use super::{PixelData, Pixels};

    fn rgb(data: Vec<u8>) -> Pixels {
        Pixels {
            width: (data.len() / 3) as u32,
            height: 1,
            data: PixelData::Rgb(data),
        }
    }

    fn rgba(data: Vec<u8>) -> Pixels {
        Pixels {
            width: (data.len() / 4) as u32,
            height: 1,
            data: PixelData::Rgba(data),
        }
    }

    fn bytes(p: &Pixels) -> &[u8] {
        match &p.data {
            PixelData::Rgb(d) | PixelData::Rgba(d) | PixelData::RgbaPremultiplied(d) => d,
        }
    }

    #[test]
    fn premultiply_rgb_adds_opaque_alpha() {
        let p = rgb(vec![200, 100, 50]).premultiplied();
        assert!(matches!(p.data, PixelData::RgbaPremultiplied(_)));
        assert_eq!(bytes(&p), &[200, 100, 50, 255]);
    }

    #[test]
    fn premultiply_straight_rgba_matches_egui_at_each_alpha() {
        // One pixel (200, 100, 50) at alpha 0, 1, 254, 255; expected values hand-computed from
        // egui's `from_rgba_unmultiplied` (opaque and transparent fast paths, else mul_frac_round).
        let p = rgba(vec![
            200, 100, 50, 0, // transparent -> cleared
            200, 100, 50, 1, // barely visible
            200, 100, 50, 254, // nearly opaque
            200, 100, 50, 255, // opaque
        ])
        .premultiplied();
        assert_eq!(
            bytes(&p),
            &[
                0, 0, 0, 0, //
                1, 0, 0, 1, //
                199, 100, 50, 254, //
                200, 100, 50, 255,
            ]
        );
    }

    #[test]
    fn straight_of_fully_opaque_premultiplied_drops_alpha() {
        let premult = Pixels {
            width: 2,
            height: 1,
            data: PixelData::RgbaPremultiplied(vec![10, 20, 30, 255, 40, 50, 60, 255]),
        };
        let s = premult.straight();
        assert!(matches!(s.data, PixelData::Rgb(_)));
        assert_eq!(bytes(&s), &[10, 20, 30, 40, 50, 60]);
    }

    #[test]
    fn straight_of_partly_transparent_premultiplied_un_premultiplies() {
        let premult = Pixels {
            width: 1,
            height: 1,
            data: PixelData::RgbaPremultiplied(vec![199, 100, 50, 254]),
        };
        let s = premult.straight();
        assert!(matches!(s.data, PixelData::Rgba(_)));
        // (c * 255 + a/2) / a, the encoders' un-premultiply.
        assert_eq!(bytes(&s), &[200, 100, 50, 254]);
    }

    #[test]
    fn round_trip_is_exact_near_opaque_and_documented_at_the_edges() {
        let original = rgba(vec![
            200, 100, 50, 0, //
            200, 100, 50, 1, //
            200, 100, 50, 254, //
            200, 100, 50, 255,
        ]);
        let back = original.premultiplied().straight();
        // Not fully opaque, so the un-premultiply keeps straight RGBA.
        assert_eq!(
            bytes(&back),
            &[
                0, 0, 0, 0, // alpha 0 loses the colour entirely
                255, 0, 0, 1, // alpha 1 can't recover the colour (1 step of precision)
                200, 100, 50, 254, // alpha 254 round-trips exactly
                200, 100, 50, 255, // alpha 255 round-trips exactly
            ]
        );
    }

    #[test]
    fn premultiplied_already_premultiplied_is_unchanged() {
        let premult = Pixels {
            width: 1,
            height: 1,
            data: PixelData::RgbaPremultiplied(vec![1, 2, 3, 4]),
        };
        assert_eq!(bytes(&premult.premultiplied()), &[1, 2, 3, 4]);
    }
}

#[cfg(test)]
mod history_sharing_tests {
    use std::sync::Arc;

    use super::{Document, PixelData, Pixels};
    use crate::annotation::Rectangle;
    use crate::geometry::{Point, Rect};
    use crate::image_op::ImageTransform;
    use crate::style::Style;

    fn add_rect(doc: &mut Document, x: f32) {
        doc.step(|d| {
            d.add(
                Rectangle::new(Rect::from_min_size(Point::new(x, x), 10.0, 10.0)),
                Style::default(),
            );
        });
    }

    fn image(w: u32, h: u32) -> Pixels {
        Pixels {
            width: w,
            height: h,
            data: PixelData::Rgb(vec![0u8; (w * h * 3) as usize]),
        }
    }

    #[test]
    fn undo_restores_the_exact_annotations_after_several_steps() {
        let mut doc = Document::new(100, 100);
        add_rect(&mut doc, 1.0);
        let after_one = doc.annotations().to_vec();
        add_rect(&mut doc, 2.0);
        add_rect(&mut doc, 3.0);
        let after_three = doc.annotations().to_vec();

        assert!(doc.undo());
        assert!(doc.undo());
        assert_eq!(doc.annotations(), after_one.as_slice());

        assert!(doc.redo());
        assert!(doc.redo());
        assert_eq!(doc.annotations(), after_three.as_slice());
    }

    #[test]
    fn consecutive_steps_share_one_annotation_copy() {
        let mut doc = Document::new(100, 100);
        add_rect(&mut doc, 1.0);
        add_rect(&mut doc, 2.0);
        add_rect(&mut doc, 3.0);

        let steps = doc.history.steps_for_test();
        assert_eq!(steps.len(), 3);
        for pair in steps.windows(2) {
            assert!(
                Arc::ptr_eq(&pair[0].after.annotations, &pair[1].before.annotations),
                "the after of one step must be the same Arc as the before of the next",
            );
        }
    }

    #[test]
    fn pixel_cap_drops_the_oldest_image_operation_steps() {
        // Each operation stores one 100×100 RGB buffer (30 000 bytes). A 50 000-byte cap keeps the
        // pixels of one step, so the oldest steps are dropped as new ones arrive.
        let mut doc = Document::new(100, 100);
        doc.set_full_resolution(Arc::new(image(100, 100)));
        doc.set_history_pixel_cap(50_000);

        for _ in 0..4 {
            doc.image_operation(
                image(100, 100),
                ImageTransform::Flip {
                    horizontal: true,
                    width: 100.0,
                    height: 100.0,
                },
            );
        }

        // The oldest Image-operation steps were dropped to stay under the cap; undo still works on
        // what remains and never panics.
        assert!(doc.undo_len() < 4, "the cap must drop the oldest steps");
        while doc.can_undo() {
            assert!(doc.undo());
        }
    }
}
