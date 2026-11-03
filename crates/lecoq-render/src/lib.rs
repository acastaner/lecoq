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

use lecoq_core::{AnnotationId, Document, Rect};
use skia_safe as sk;

/// Pixels to draw the Image from: the display-size decode, or the full-resolution pixels.
pub struct Source<'a> {
    /// Identifies these pixels: the renderer keeps them converted between calls with the same key.
    pub key: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: SourcePixels<'a>,
}

/// 8-bit sRGB, tightly packed, row-major.
#[derive(Clone, Copy)]
pub enum SourcePixels<'a> {
    Rgb(&'a [u8]),
    /// Straight (not premultiplied) alpha.
    Rgba(&'a [u8]),
    /// Premultiplied alpha, as egui keeps its images.
    RgbaPremultiplied(&'a [u8]),
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
    /// The last source, converted to a Skia image.
    source: Option<(u64, sk::Image)>,
}

impl Renderer {
    pub fn new() -> Self {
        Self { source: None }
    }

    /// The Image plus every Annotation of `doc`, for `view`.
    pub fn composite(&mut self, doc: &Document, source: &Source<'_>, view: View) -> Composite {
        let image = self.source_image(source);
        render(view, |canvas| scene::Scene::new(image, doc).draw(canvas))
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
        if self
            .source
            .as_ref()
            .is_none_or(|(key, _)| *key != source.key)
        {
            self.source = Some((source.key, to_skia_image(source)));
        }
        &self.source.as_ref().expect("just set").1
    }
}

/// Run `draw` on a transparent `view`-sized canvas set up in Image pixels.
fn render(view: View, draw: impl FnOnce(&sk::Canvas)) -> Composite {
    let width = view.width.max(1);
    let height = view.height.max(1);
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    let info = sk::ImageInfo::new(
        (width as i32, height as i32),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    {
        let mut surface = sk::surfaces::wrap_pixels(&info, &mut pixels, None, None)
            .expect("a raster surface for the composite");
        let canvas = surface.canvas();
        canvas.clear(sk::Color::TRANSPARENT);
        // From here on, draw in Image pixels.
        canvas.scale((
            width as f32 / view.region.width().max(f32::EPSILON),
            height as f32 / view.region.height().max(f32::EPSILON),
        ));
        canvas.translate((-view.region.min.x, -view.region.min.y));
        draw(canvas);
    }
    Composite {
        width,
        height,
        rgba_premultiplied: pixels,
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

fn to_skia_image(source: &Source<'_>) -> sk::Image {
    let (data, alpha) = match source.pixels {
        SourcePixels::Rgb(rgb) => {
            let mut rgba = Vec::with_capacity(source.width as usize * source.height as usize * 4);
            for px in rgb.chunks_exact(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            (sk::Data::new_copy(&rgba), sk::AlphaType::Opaque)
        }
        SourcePixels::Rgba(rgba) => (sk::Data::new_copy(rgba), sk::AlphaType::Unpremul),
        SourcePixels::RgbaPremultiplied(rgba) => (sk::Data::new_copy(rgba), sk::AlphaType::Premul),
    };
    let info = sk::ImageInfo::new(
        (source.width as i32, source.height as i32),
        sk::ColorType::RGBA8888,
        alpha,
        None,
    );
    sk::images::raster_from_data(&info, data, source.width as usize * 4)
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
