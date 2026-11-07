//! Skia rendering (spec §7.1, §7.2, ADR 0003): render a Document region at screen resolution,
//! Flatten at full resolution, the highlight/obfuscate/shadow effects, resampling.
//!
//! Skia is initialised lazily, on the first Annotation, never before the first paint (§1.3): the
//! app creates its [`Renderer`] only then. Nothing in this crate runs unless a `Renderer` exists.
//!
//! [`Renderer::composite`] is the one drawing routine: the screen composite is a region of the
//! Image at the window's resolution; Flatten (#34) is the same call with the whole Image at full
//! resolution, so the screen and the file match.

mod bounds;
mod effects;
mod resample;
mod scene;
mod shapes;
pub mod text;

pub use bounds::{drawn_bounds, is_opaque_effect};
pub use resample::resample;

use lecoq_core::{AnnotationId, Document, PixelData, Pixels, Rect};
use skia_safe as sk;

/// Pixels to draw the Image from: the display-size decode, or the full-resolution pixels (#64).
pub struct Source<'a> {
    /// Identifies these pixels: the renderer keeps them converted between calls with the same key.
    pub key: u64,
    /// The one pixel buffer ([`to_skia`] turns any of its layouts into a Skia image).
    pub pixels: &'a Pixels,
}

/// What to render: `region` of the Image (in Image pixels) scaled onto `width`×`height` pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub region: Rect,
    pub width: u32,
    pub height: u32,
}

/// A rendered region: 8-bit RGBA, premultiplied alpha, tightly packed.
#[derive(Clone)]
pub struct Composite {
    pub width: u32,
    pub height: u32,
    pub rgba_premultiplied: Vec<u8>,
}

impl std::fmt::Debug for Composite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Composite({}×{})", self.width, self.height)
    }
}

/// The Skia renderer. Creating one is what "initialising Skia" means for lecoq.
pub struct Renderer {
    /// The converted Skia sources, most-recently-used first, at most two. Two slots are kept so
    /// zooming back and forth across the display density — which alternates the source key between
    /// the display-size and the full-resolution pixels — does not reconvert the full-resolution
    /// buffer on each flip (#66). Each entry is keyed by [`Source::key`], exactly as the one slot
    /// was.
    sources: Vec<(u64, sk::Image)>,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    /// The Image plus every Annotation of `doc`, for `view`, in a freshly allocated buffer. This is
    /// [`composite_into`](Self::composite_into) with its own `view`-sized buffer, kept for Flatten
    /// and the Annotation PNG, which own their result.
    pub fn composite(&mut self, doc: &Document, source: &Source<'_>, view: View) -> Composite {
        let width = view.width.max(1);
        let height = view.height.max(1);
        let mut buffer = vec![0u8; width as usize * height as usize * 4];
        self.composite_into(doc, source, view, &mut buffer, true);
        Composite {
            width,
            height,
            rgba_premultiplied: buffer,
        }
    }

    /// Render the composite straight into `buffer`, a caller-provided RGBA-premultiplied buffer of
    /// exactly `view.width.max(1) * view.height.max(1) * 4` bytes (#55). This is the one drawing
    /// routine; [`composite`](Self::composite) wraps it around an owned buffer.
    ///
    /// When `fresh` is true the caller guarantees `buffer` is already fully transparent (a new
    /// zeroed allocation), so the redundant `clear` is skipped — the win that keeps a reused,
    /// page-faulted buffer from being walked a second time. When it is false the buffer is a reused
    /// one and is cleared first, so no pixel of the previous frame can survive.
    pub fn composite_into(
        &mut self,
        doc: &Document,
        source: &Source<'_>,
        view: View,
        buffer: &mut [u8],
        fresh: bool,
    ) {
        let image = self.source_image(source);
        render_into(view, buffer, fresh, |canvas| {
            scene::Scene::new(image, doc).draw(canvas)
        });
    }

    /// Only the Annotations `ids` of `doc`, on transparency, for `view` (the PNG of copied
    /// Annotations, §6.2). They are drawn as in `doc` (a Step counter keeps its number there),
    /// each over an empty Scene: no Image and no other Annotation.
    pub fn annotations_only(
        &mut self,
        doc: &Document,
        ids: &[AnnotationId],
        view: View,
    ) -> Composite {
        let below = scene::Scene::empty(doc);
        render(view, |canvas| {
            for annotation in doc.annotations().iter().filter(|a| ids.contains(&a.id)) {
                shapes::draw(canvas, annotation, &below);
            }
        })
    }

    /// Flatten (§7.2): the whole Image plus every Annotation at full resolution, with the same
    /// drawing code as the screen. `source` is the full-resolution pixels.
    pub fn flatten(&mut self, doc: &Document, source: &Source<'_>) -> Composite {
        let view = View {
            region: doc.bounds(),
            width: doc.width(),
            height: doc.height(),
        };
        self.composite(doc, source, view)
    }

    fn source_image(&mut self, source: &Source<'_>) -> &sk::Image {
        if let Some(pos) = self.sources.iter().position(|(key, _)| *key == source.key) {
            // Keep the hit as most-recently-used, so the other slot survives the next flip.
            self.sources[..=pos].rotate_right(1);
        } else {
            // Miss: convert, put it in front, and drop the least-recently-used beyond the two slots.
            self.sources.insert(0, (source.key, to_skia(source.pixels)));
            self.sources.truncate(2);
        }
        &self.sources[0].1
    }
}

/// Run `draw` on a transparent `view`-sized canvas (a freshly allocated buffer) set up in Image
/// pixels, and return it owned. For callers that own their result (Flatten, the Annotation PNG).
fn render(view: View, draw: impl FnOnce(&sk::Canvas)) -> Composite {
    let width = view.width.max(1);
    let height = view.height.max(1);
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    render_into(view, &mut pixels, true, draw);
    Composite {
        width,
        height,
        rgba_premultiplied: pixels,
    }
}

/// Run `draw` on `buffer` as a `view`-sized canvas set up in Image pixels. `buffer` must be exactly
/// `view.width.max(1) * view.height.max(1) * 4` bytes. When `fresh` is false the buffer is cleared
/// first; when true it is taken to be already transparent and the clear is skipped (#55).
fn render_into(view: View, buffer: &mut [u8], fresh: bool, draw: impl FnOnce(&sk::Canvas)) {
    let width = view.width.max(1);
    let height = view.height.max(1);
    let info = sk::ImageInfo::new(
        (width as i32, height as i32),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    let mut surface = sk::surfaces::wrap_pixels(&info, buffer, None, None)
        .expect("a raster surface for the composite");
    let canvas = surface.canvas();
    if !fresh {
        canvas.clear(sk::Color::TRANSPARENT);
    }
    // From here on, draw in Image pixels.
    canvas.scale((
        width as f32 / view.region.width().max(f32::EPSILON),
        height as f32 / view.region.height().max(f32::EPSILON),
    ));
    canvas.translate((-view.region.min.x, -view.region.min.y));
    draw(canvas);
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

/// The one conversion from a lecoq [`Pixels`] to a Skia image (#64): the [`Renderer`]'s source
/// cache, [`resample`] and the Inset cache all go through it, so the RGB→RGBA expansion and the
/// alpha handling live in one place. RGB expands to opaque RGBA; straight RGBA stays straight
/// (`Unpremul`); premultiplied RGBA is taken as-is (`Premul`).
/// See also `lecoq_core::Pixels::premultiplied`, the other RGB→opaque-RGBA expansion; keep in sync.
pub fn to_skia(pixels: &Pixels) -> sk::Image {
    let (data, alpha) = match &pixels.data {
        PixelData::Rgb(rgb) => {
            let mut rgba = Vec::with_capacity(pixels.width as usize * pixels.height as usize * 4);
            for px in rgb.chunks_exact(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            (sk::Data::new_copy(&rgba), sk::AlphaType::Opaque)
        }
        PixelData::Rgba(rgba) => (sk::Data::new_copy(rgba), sk::AlphaType::Unpremul),
        PixelData::RgbaPremultiplied(rgba) => (sk::Data::new_copy(rgba), sk::AlphaType::Premul),
    };
    let info = sk::ImageInfo::new(
        (pixels.width as i32, pixels.height as i32),
        sk::ColorType::RGBA8888,
        alpha,
        None,
    );
    sk::images::raster_from_data(&info, data, pixels.width as usize * 4)
        .expect("a Skia image from the decoded pixels")
}

pub(crate) fn color(c: lecoq_core::Rgba) -> sk::Color {
    sk::Color::from_argb(c.a, c.r, c.g, c.b)
}

pub(crate) fn point(p: lecoq_core::Point) -> sk::Point {
    sk::Point::new(p.x, p.y)
}

pub(crate) fn rect(r: Rect) -> sk::Rect {
    sk::Rect::from_ltrb(r.min.x, r.min.y, r.max.x, r.max.y)
}

pub(crate) fn core_rect(r: sk::Rect) -> Rect {
    Rect::from_corners(
        lecoq_core::Point::new(r.left, r.top),
        lecoq_core::Point::new(r.right, r.bottom),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lecoq_core::{Document, Point, Rectangle, Style};

    /// A 48×32 RGB source with a gradient, so a stale-pixel bug would show up as wrong colours.
    fn source() -> Pixels {
        let (w, h) = (48u32, 32u32);
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                data.push((x * 5) as u8);
                data.push((y * 7) as u8);
                data.push(96);
            }
        }
        Pixels {
            width: w,
            height: h,
            data: PixelData::Rgb(data),
        }
    }

    fn whole_view() -> View {
        View {
            region: Rect::from_corners(Point::new(0.0, 0.0), Point::new(48.0, 32.0)),
            width: 48,
            height: 32,
        }
    }

    /// A shadowed Rectangle (the ticket's measurement shape) inside the view.
    fn doc_with_shadowed_rectangle() -> Document {
        let mut doc = Document::new(48, 32);
        doc.step(|doc| {
            doc.add(
                Rectangle::new(Rect::from_corners(
                    Point::new(10.0, 8.0),
                    Point::new(36.0, 26.0),
                )),
                Style {
                    shadow: true,
                    ..Style::default()
                },
            )
        });
        doc
    }

    #[test]
    fn composite_into_a_fresh_buffer_equals_the_owned_composite() {
        let px = source();
        let doc = doc_with_shadowed_rectangle();
        let view = whole_view();

        let owned = Renderer::new().composite(
            &doc,
            &Source {
                key: 1,
                pixels: &px,
            },
            view,
        );

        let mut buffer = vec![0u8; (view.width * view.height * 4) as usize];
        Renderer::new().composite_into(
            &doc,
            &Source {
                key: 1,
                pixels: &px,
            },
            view,
            &mut buffer,
            true,
        );

        assert_eq!(
            owned.rgba_premultiplied, buffer,
            "composite_into a fresh buffer must equal composite()"
        );
    }

    #[test]
    fn composite_into_a_reused_dirty_buffer_leaves_no_stale_pixels() {
        let px = source();
        let view = whole_view();
        let mut renderer = Renderer::new();

        // Dirty the buffer with a different Document: a filled, shadowed shape over the whole view.
        let mut other = Document::new(48, 32);
        other.step(|doc| {
            doc.add(
                Rectangle::new(view.region),
                Style {
                    shadow: true,
                    fill: Some(lecoq_core::Rgba::WHITE),
                    ..Style::default()
                },
            )
        });
        let mut buffer = vec![0u8; (view.width * view.height * 4) as usize];
        renderer.composite_into(
            &other,
            &Source {
                key: 1,
                pixels: &px,
            },
            view,
            &mut buffer,
            true,
        );

        // Reuse it for the real Document with fresh = false (the per-frame reuse path).
        let doc = doc_with_shadowed_rectangle();
        renderer.composite_into(
            &doc,
            &Source {
                key: 1,
                pixels: &px,
            },
            view,
            &mut buffer,
            false,
        );

        // Must match a from-scratch render: no pixel of the previous frame may survive.
        let fresh = renderer.composite(
            &doc,
            &Source {
                key: 1,
                pixels: &px,
            },
            view,
        );
        assert_eq!(
            fresh.rgba_premultiplied, buffer,
            "a reused buffer rendered with fresh = false must leave no stale pixels"
        );
    }

    /// A second source with a different gradient, so returning the wrong cached slot would show as
    /// wrong pixels.
    fn other_source() -> Pixels {
        let (w, h) = (48u32, 32u32);
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                data.push(200u8.wrapping_sub((x * 5) as u8));
                data.push(16);
                data.push((y * 3) as u8);
            }
        }
        Pixels {
            width: w,
            height: h,
            data: PixelData::Rgb(data),
        }
    }

    #[test]
    fn alternating_two_source_keys_keeps_both_converted_and_renders_each_correctly() {
        let doc = doc_with_shadowed_rectangle();
        let view = whole_view();
        let display = source();
        let full = other_source();
        // The app's keys: display-size decodes are even, full-resolution ones are odd.
        let display_src = Source {
            key: 4,
            pixels: &display,
        };
        let full_src = Source {
            key: 3,
            pixels: &full,
        };

        // What each source must look like on its own.
        let want_display = Renderer::new().composite(&doc, &display_src, view);
        let want_full = Renderer::new().composite(&doc, &full_src, view);

        let mut renderer = Renderer::new();
        // Flip back and forth across the display density several times.
        for _ in 0..3 {
            let got_display = renderer.composite(&doc, &display_src, view);
            assert_eq!(
                got_display.rgba_premultiplied,
                want_display.rgba_premultiplied
            );
            let got_full = renderer.composite(&doc, &full_src, view);
            assert_eq!(got_full.rgba_premultiplied, want_full.rgba_premultiplied);
        }

        // Both sources stay converted: the flips never evict each other (so, in the app, the
        // full-resolution buffer is not reconverted on every flip — the point of the two slots).
        assert_eq!(renderer.sources.len(), 2);
        assert!(renderer.sources.iter().any(|(k, _)| *k == 4));
        assert!(renderer.sources.iter().any(|(k, _)| *k == 3));
    }
}
