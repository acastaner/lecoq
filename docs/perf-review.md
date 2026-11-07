# lecoq performance and bloat review (2026-11-07, v0.3.0 at b259eb8)

A read-through of every crate against the §1.2 budgets and the v0.3.0 bench record, in the
deep-module vocabulary (module, interface, seam, depth, locality). Nothing here was profiled;
costs are estimated from the code and the bench JSON, so each item names what to measure.

## Where the time actually is

`packaging/release-notes/v0.3.0.bench-cold.json`, per launch:

| mark | ms (typical) |
|---|---|
| `main` | 6–10 |
| `decoded` (display-size decode of the 12 MP JPEG, resize included) | 87–105 |
| `renderer_ready` (window + GL context + egui, from `eframe::run_native`) | 104–117 |
| `first_image_painted` | 107–121 |

So on the cold-start path the decode already finishes ~10 ms before the window is ready. A faster
JPEG decode (DCT-scaled decode through libjpeg-turbo, say) would move cold start by nothing today;
it would only shorten uncached Sibling switches and prefetch. The ~100 ms between `main` and
`renderer_ready` is winit/glutin/eframe plus `Platform::os()`, and nothing in lecoq's own code
sits on it except `OsConfigStore::new` (one `current_exe` + `canonicalize`) and `BaseDirs::new`.
Cold start is not where optimisation effort pays.

What does pay, in order:

1. the cached Sibling switch (`switch_ms`, the one number the spec budgets besides cold start);
2. every frame while panning, zooming or dragging with Annotations on screen;
3. Image operations on large Images;
4. per-frame garbage that runs at the display's refresh rate for the whole session;
5. redundant code: the same pixel conversions written several times, and the same background-job
   choreography written six times in `app/mod.rs`.

## Tier 1: measurable wins

### 1. The composite touches the canvas three times per full render

`lecoq-render/src/lib.rs:116-139` (`render`) allocates `vec![0u8; w*h*4]`, then `canvas.clear`
zeroes it again (the `vec!` is calloc, so the clear is what actually faults every page in), and
`editor.rs:1151-1174` copies the result a third time into an `egui::ColorImage` before
`texture.set`. At 2560×1440 that is three passes over 14.7 MB on every full-path frame: every pan
or zoom frame with Annotations, every undo, every property change. The fast path (#46) only avoids
this during a gesture.

Fix at the `Renderer::composite` seam: render straight into the texture's pixels. The editor owns
an `egui::ColorImage` sized to the view, hands `as_raw_mut()` to the renderer (`wrap_pixels` takes
any `&mut [u8]`), the renderer skips `clear` when told the buffer is fresh, and `texture.set` takes
the `Arc<ColorImage>` with no copy. Keep the buffer between frames (`Arc::make_mut` is free once
the texture manager has consumed last frame's delta). `Composite` stays an RGBA-premultiplied
byte buffer, so the render crate stays egui-free; `blit`/`restore_region` work unchanged on
`as_raw_mut()`.

Measure: frame time while panning a 2560×1440 canvas with one shadowed Rectangle.

### 2. The display-size cache holds decode-format pixels, so every switch converts on the UI thread

`Prefetcher` caches `Arc<DisplayImage>` (RGB or straight RGBA, codec format). On a cached switch
`app/mod.rs` `upload` → `color_image` converts RGB→`Color32` on the UI thread (a 2 MP display
image: 6 MB read, 8 MB written, ~1–3 ms of the 5–12 ms `switch_ms`), then `load_texture` uploads.
The result is kept as `ShownImage::display_pixels`, so the shown Image's pixels live twice (RGB in
the cache, RGBA in the app) and the second copy is not counted against the 256 MB cap.

Fix at the cache's interface: its contract becomes "upload-ready pixels", produced on the pool
thread. `DisplayImage` carries RGBA8 with alpha 255 for opaque images plus an `opaque` flag; for
opaque images straight == premultiplied, so the app wraps the bytes as a `ColorImage` with
`bytemuck::cast_vec` (Color32 is `repr(C) [u8; 4]`, Pod; `bytemuck` is already in the tree via
`ecolor`). The switch path then does an `Arc` clone and a GPU upload, nothing else, and
`display_pixels` *is* the cache entry. Cost: opaque entries grow by a third, so the cap holds
fewer Siblings. Translucent (RGBA) images keep one premultiply on the pool thread.

Measure: `tools/bench switch` p90 before and after.

### 3. Two independent full-resolution decodes of the same file

Zooming past the display size starts `request_pixels` (`app/mod.rs:1489-1535`): a
`PendingDecode` at `max_texture_side`, uploaded as `ShownImage::full`. Picking a tool starts
`Editor::start_full_decode` (`editor.rs:531-541`): a `PendingFullDecode` into
`Document::full_resolution`. Zoom in, then draw, and the file is decoded twice at full resolution,
with two transient full-size buffers. For a 100 MP Image that is two multi-second decodes and two
400 MB buffers.

Fix: one full-resolution source behind one seam. `ShownImage` (or a small `FullResolution` module
both own a handle to) runs at most one decode; the zoom texture is built from the editor's
`Arc<Pixels>` when that exists, and the editor adopts the zoom decode's pixels when that landed
first (the `keep_decoded` path already does exactly this hand-over for the save worker, so the
pattern exists). `decode_full` and `PendingDecode::spawn` currently produce different types
(`Pixels` vs `DisplayImage`); with finding 10 they produce the same one.

Measure: time from Rectangle tool press to first composite, on a zoomed 50 MP JPEG.

### 4. Pan and zoom with Annotations re-render the visible region every frame

`CompositeTexture::key` is `(revision, source.key, View)`, so any pan or zoom changes the key and
runs the full Skia pass, shadows included, for the whole visible region. Shadowed shapes are the
dominant cost (`with_shadow` is a `save_layer` plus a two-pass blur per shape), and nothing is
reused between consecutive pan frames.

Deeper option: while the composite is drawn from the display-size source (Fit, and any zoom up to
the display density), render once per `revision` over the *whole* Image at the display texture's
resolution, and let the canvas draw the visible sub-rect with UV coordinates, exactly as it draws
the plain Image texture. Re-render only when `revision` changes or the zoom crosses the display
density (where the full-resolution source takes over, as now). Pan and zoom with Annotations then
cost what pan and zoom without them cost.

Trade-off: the screen stops being a pixel-exact preview of Flatten at sub-display zooms (it is
resampled by the GPU from the display-density composite, as the plain Image already is), and the
composite goldens need the whole-Image view. It is the one item here that changes a spec'd
behaviour (§7.2 "the screen and the file match"), so it needs a decision rather than a patch.

### 5. Rotate and flip go through a per-pixel closure

`lecoq-core/src/image_op.rs:197-220` (`Pixels::remap`) runs a closure, two multiplies, a bounds
check and a 3- or 4-byte `extend_from_slice` per pixel: ~1e8 iterations for a 100 MP Image, so
rotate and flip pause for hundreds of milliseconds where a row-wise version would not. Flip
horizontal is `chunks_exact(n).rev()` per row, flip vertical is rows copied in reverse
(`copy_from_slice`, a memcpy each), rotate is a blocked transpose. The same `remap` is used by
`Inset::image_transform`.

Measure: Rotate on a 100 MP RGB Image.

### 6. Animation frames parse the ICC profile and build an lcms transform per frame

`lecoq-codec/src/animation.rs:50-58` clones the profile bytes per frame and calls
`Decoded::into_display`, which runs `colour::to_srgb`: `Profile::new_icc`, `Profile::new_srgb`,
the `is_srgb` tag and curve comparison, and `Transform::new_flags`, for every frame. A 200-frame
GIF with an embedded profile parses it 200 times on the animation thread while the first frames
are already playing.

Fix: `Frames` prepares the colour converter once (an `Option<lcms2::Transform>`, or `None` when
the profile is sRGB or absent) and `into_display` takes a prepared converter instead of raw bytes.
While there, `is_srgb` builds the reference sRGB profile on every call; a `thread_local` holds
it.

### 7. The whole `Config` is cloned and compared every frame

`track_window` (`app/mod.rs:852-879`) runs every frame and calls `update_config`, which clones
`Config` (`tools: BTreeMap<String, toml::Table>`: ~12 tables of ~14 keyed values, every value an
owned `String` or `toml::Value`) and compares it, to detect a window-size change. That is a few
hundred allocations and frees per frame for the whole session.

Fix: compare the three window fields first and call `update_config` only when they differ.
Related: `Command::SetProperty` (`app/mod.rs:362-370`) serialises every tool's style to a toml
table (`tool_styles_config`) on every slider tick; serialise the one tool that changed.

### 8. The status bar locates the shown file by a linear path scan every frame

`status_bar()` → `Navigation::position` → `SiblingList::locate` (`scan.rs:46-55`), a
`position(|s| s.path == path)` over the whole list, every frame. In the 10,000-file folder §1.2
budgets that is 10,000 `PathBuf` compares (long shared prefixes, so near-full memcmps) per frame,
then a `format!`. The index is also recomputed by `would_move`, `go` and `keep_window`.

Fix: `SiblingList` remembers `(path, index)` of the last lookup and returns it while the list is
unchanged (every mutation goes through `remove`/`apply`, so invalidation is local), or keeps a
`HashMap<PathBuf, usize>` beside the `Vec`.

### 9. Hand-rolled base64 decodes with a linear alphabet search per character

`lecoq-core/src/clipboard.rs:186-202`: `ALPHABET.iter().position(..)` per input byte, so
pasting an Inset of 1000×1000 RGBA (5.3 M base64 chars) does ~170 M byte compares before the
pixels exist. A 256-entry reverse table makes it a lookup per byte; the `base64` crate is the
other route.

## Tier 2: per-frame garbage and needless clones (each small, all cheap)

- **Tooltips rebuilt every frame.** `tool_strip::show` formats 11 tooltip strings and
  `menu_bar::quick_actions` 4 more, every frame (`with_shortcut` + `shortcut_text`). They depend
  only on the OS; compute once.
- **Status bar.** `position` and `Zoom::label` allocate per frame; `canvas_message` rebuilds the
  empty-window hint per frame. Cache per state change or return `Cow`.
- **`property_target`** clones the `Style` and `to_vec()`s the property list every frame.
- **`Style::font` is a `String`.** `Style` is cloned per pointer event (`with_tool`), per frame
  (`property_target`), per tool switch, and once per Annotation in every History snapshot. An
  `Arc<str>` (or an interned family id) makes `Style` clone allocation-free.
- **Six redundant mutex locks per frame.** Every poll site calls `pending.wake(ctx)` each frame
  (`poll_decode`, `poll_upgrade`, `poll_incoming`, `poll_save`, `poll_copy`, `Editor::poll`)
  because some spawn sites (`open`, `Launch::decode_first_image`, `start_full_decode`) have no
  `ctx` at spawn time. Give `Pending::spawn_job` the waker (an `Option<egui::Context>`, or the
  `Box<dyn Fn()>` the siblings crate already uses) and delete the per-frame calls.
- **Text editing.** `TextSession::caret` lays the paragraph out twice per frame (`at(cursor)` and
  `at(cursor + preedit)`), `layout()` clones the text and the `Style` on each call, and
  `input()` clones the text twice to detect a change. `text_input` also clones the whole egui
  event list per frame. A `thread_local` cache of the last `Paragraph` keyed on
  `(text, font, size, wrap)` in `lecoq_render::text` removes the layouts; the clones go with
  borrowing.
- **History snapshots own two copies per step.** `Step { before, after }` each hold a cloned
  `Vec<Annotation>`, and `after` of step *n* equals `before` of step *n+1*; `undo`/`redo` clone
  the snapshot and then `restore` clones it again. Sharing the annotation vector as
  `Arc<[Annotation]>` between consecutive steps halves History memory (freehand strokes and text
  layouts are the heavy ones; Inset pixels are already `Arc`) and makes undo a pointer copy plus
  one `to_vec`.
- **`families()`** sorts with `sort_by_key(|n| n.to_lowercase())`, one allocation per comparison;
  `sort_by_cached_key`.
- **`scan()`** clones every name into a second `Vec<String>` to call `sorted_indices`; take
  `&[&str]` (or a name accessor) and drop the clone and the `Option` dance.
- **`Renderer` keeps one converted source.** Zooming back and forth across the display density
  alternates the source key between display and full pixels, and `to_skia_image` reconverts the
  full-resolution buffer (RGB→RGBA copy + `Data::new_copy`, 2× 400 MB at 100 MP) on each flip.
  Two slots, one per source kind.

## Tier 3: redundant and bloated code (perf-neutral, locality wins)

### 10. Five pixel-buffer types and nine conversion sites

| type | crate |
|---|---|
| `Pixels::{Rgb, Rgba}(Vec<u8>)` | lecoq-codec |
| `Pixels { width, height, data: PixelData::{Rgb, Rgba} }` | lecoq-core |
| `SourcePixels::{Rgb, Rgba, RgbaPremultiplied}(&[u8])` | lecoq-render |
| `DisplayImage` | lecoq-codec |
| `egui::ColorImage` | lecoq |

Conversions between them are written by hand in `decode.rs` (`decode_full`), `paste.rs`
(`image`), `save.rs` (`flatten`, `straight`, `from_color_image`), `editor.rs`
(`update_composite`), `app/mod.rs` (`color_image`), `resample.rs` (`to_image`, and RGBA→RGB),
`lib.rs` (`to_skia_image`) and `shapes/inset.rs` (`convert`). The RGB→RGBA loop exists three
times in the render crate alone; RGBA→RGB twice.

Deletion test: delete the codec `Pixels` and `SourcePixels`, and the complexity does not reappear
at the callers; it disappears, because `lecoq_core::Pixels` already carries width, height and
channels. `lecoq-core` has no dependencies, so `lecoq-codec` can depend on it and decode straight
into it; `DisplayImage` wraps it; the render crate gets one `to_skia(&Pixels) -> sk::Image` used
by `Renderer`, `resample` and the Inset cache; and the three app-side conversion functions go.
This is also what lets finding 3 share one decode and finding 2 hand the cache's pixels straight
to egui.

### 11. The app's background-job choreography is written six times

`LecoqApp` has `pending`, `upgrade`, `incoming`, `save.saving`, `copy.in_progress` and the
editor's `full_decode`: each an `Option<Pending<T>>` with its own `poll_*` that does
`wake` / `take` / `expect("checked above")` / clear, and its own term in `is_busy`. Three dialogs
(`open_dialog`, `folder_dialog`, `save.dialog`) each hold an `mpsc::Receiver<Option<PathBuf>>`
and do the same `try_recv` match; `dialog_answer` exists for two of them and `poll_folder_dialog`
re-implements it inline.

A `Job<T>` module (spawn with waker, `poll(&mut self) -> Option<T>`, `is_running()`) and a
`DialogSlot` module (open, `answer() -> Option<Option<PathBuf>>`) are two deep modules behind
small interfaces; `is_busy` becomes a fold over them and roughly 150 lines leave `app/mod.rs`,
which its own header says should be thin and is 2,000 lines.

### 12. Smaller redundancies

- `Pending::wait` and the `Condvar` exist for one caller (startup's `FIRST_FRAME_WAIT`); after
  finding 11 the first image can be the one `Job` that supports a timed wait, and the others drop
  the `Condvar`.
- `Editor::poll` polls the font listing at 30 ms intervals (`request_repaint_after`) instead of
  the thread requesting a repaint when done, as every other worker does.
- `ImageTransform::Rotate90 { width, height }` and `Flip { width, height }` carry the Image size
  that `Document::image_operation` already knows.
- `Decoded::icc` is `Option<Vec<u8>>` and cloned into every animation frame (folds into finding 6).

## What not to do

- Don't swap the JPEG decoder for cold start; see "Where the time actually is". Revisit only if
  uncached switches or prefetch throughput become the complaint, and bench before and after.
- Don't thin the History cap or the cache cap to hide finding 2's extra third per entry; measure
  how many Siblings fit first.

## Suggested order

1. Findings 1, 2 and 7 (the per-frame and per-switch costs), then bench `switch` and a pan
   frame.
2. Finding 10, because 3 and 2 get simpler once there is one pixel type.
3. Findings 3, 5, 6, 8, 9 as independent small changes.
4. Finding 11 and the Tier 2 list as a cleanup pass.
5. Finding 4 as a design decision of its own (an ADR), since it touches §7.2.
