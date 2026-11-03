//! Effects shared by Annotation types: the drop shadow, and the machinery of effect Annotations
//! (highlight, obfuscate) that act on everything below them in z-order (§4.2).
//!
//! An effect Annotation's `draw` gets `below: &Scene` (the Image and the Annotations under it) and
//! the canvas, which at that point holds exactly `below` drawn. Two ways to act on it:
//! - pixel-wise effects (dim, grayscale, a multiplied colour) work on the canvas as it is:
//!   [`paint_over`], [`filter_below`];
//! - effects that move or mix pixels (magnify, pixelate, blur) re-render `below` offscreen over the
//!   region they read ([`render_below`]), so pixels outside the view (zoomed in) are still right
//!   and the screen matches Flatten.
//!
//! Both recompute on every composite, so the effect follows any change beneath it.

use skia_safe as sk;

use crate::scene::Scene;

/// The drop shadow of the "Shadow" property, in Image pixels (so Flatten matches the screen).
const SHADOW_OFFSET: (f32, f32) = (3.0, 3.0);
const SHADOW_SIGMA: f32 = 2.0;
const SHADOW_ALPHA: u8 = 110;

/// How far a shadow reaches past the shape it is under: its offset plus the blur's radius (~3σ).
pub(crate) const SHADOW_SPREAD: f32 = SHADOW_OFFSET.0 + 3.0 * SHADOW_SIGMA + 1.0;

/// Draw `draw` with a drop shadow under it when `shadow` is on. `content` is the shape's drawn
/// bounds (stroke included) in Image pixels: the shadow's offscreen layer is bounded to it plus
/// [`SHADOW_SPREAD`], so Skia sizes and blurs a shape-sized layer. Without bounds `save_layer`
/// takes the whole clip, i.e. the entire visible canvas — O(canvas) blur per shape every frame.
pub fn with_shadow(
    canvas: &sk::Canvas,
    shadow: bool,
    content: sk::Rect,
    draw: impl FnOnce(&sk::Canvas),
) {
    if !shadow {
        draw(canvas);
        return;
    }
    let filter = sk::image_filters::drop_shadow(
        SHADOW_OFFSET,
        (SHADOW_SIGMA, SHADOW_SIGMA),
        sk::Color::from_argb(SHADOW_ALPHA, 0, 0, 0),
        None,
        None,
        None,
    );
    let mut paint = sk::Paint::default();
    paint.set_image_filter(filter);
    let bounds = sk::Rect::from_ltrb(
        content.left - SHADOW_SPREAD,
        content.top - SHADOW_SPREAD,
        content.right + SHADOW_SPREAD,
        content.bottom + SHADOW_SPREAD,
    );
    canvas.save_layer(
        &sk::canvas::SaveLayerRec::default()
            .bounds(&bounds)
            .paint(&paint),
    );
    draw(canvas);
    canvas.restore();
}

/// Where a pixel-wise effect applies.
#[derive(Clone, Copy, Debug)]
pub enum Region {
    /// Inside this rectangle (Image pixels).
    Inside(sk::Rect),
    /// Everywhere on the Image except this rectangle.
    Outside(sk::Rect),
}

/// Clip the canvas to `region`, within the Image's bounds `image_rect`.
fn clip_to(canvas: &sk::Canvas, image_rect: sk::Rect, region: Region) {
    canvas.clip_rect(image_rect, sk::ClipOp::Intersect, true);
    match region {
        Region::Inside(r) => canvas.clip_rect(r, sk::ClipOp::Intersect, true),
        Region::Outside(r) => canvas.clip_rect(r, sk::ClipOp::Difference, true),
    };
}

/// Fill `region` with `paint` over what's already drawn (its blend mode decides how it combines
/// with what's below: multiply for a text highlight, src-atop to dim).
pub fn paint_over(canvas: &sk::Canvas, below: &Scene<'_>, region: Region, paint: &sk::Paint) {
    canvas.save();
    clip_to(canvas, below.image_rect, region);
    canvas.draw_rect(below.image_rect, paint);
    canvas.restore();
}

/// Replace what's drawn in `region` with it passed through `filter` (pixel-wise: grayscale, …).
pub fn filter_below(
    canvas: &sk::Canvas,
    below: &Scene<'_>,
    region: Region,
    filter: sk::ColorFilter,
) {
    let Some(backdrop) = sk::image_filters::color_filter(filter, None, None) else {
        return;
    };
    canvas.save();
    clip_to(canvas, below.image_rect, region);
    // The layer starts as the filtered backdrop and replaces it on restore.
    let mut paint = sk::Paint::default();
    paint.set_blend_mode(sk::BlendMode::Src);
    canvas.save_layer(
        &sk::canvas::SaveLayerRec::default()
            .paint(&paint)
            .backdrop(&backdrop),
    );
    canvas.restore();
    canvas.restore();
}

/// `below` re-rendered offscreen at the canvas's resolution (see [`render_below`]).
pub struct Rendered {
    pub image: sk::Image,
    /// Where on the canvas (Image pixels) `image` goes: the visible part of the requested `dest`.
    pub dest: sk::Rect,
}

/// Render the `source` region of `below` (Image pixels) as it would look stretched over `dest`
/// (Image pixels) on `canvas`, at the canvas's resolution. Only the part of `dest` inside the
/// canvas's clip is rendered. `None` when none of it is visible.
///
/// Magnify passes a `source` smaller than `dest`; obfuscate can pass `source == dest` (plus a
/// margin for blur) and then filter the image before drawing it.
pub fn render_below(
    canvas: &sk::Canvas,
    below: &Scene<'_>,
    source: sk::Rect,
    dest: sk::Rect,
) -> Option<Rendered> {
    if dest.is_empty() || source.is_empty() {
        return None;
    }
    let mut visible = dest;
    if !visible.intersect(canvas.local_clip_bounds()?) {
        return None;
    }
    // The part of `source` that maps onto the visible part of `dest`.
    let sx = source.width() / dest.width();
    let sy = source.height() / dest.height();
    let from = sk::Rect::from_ltrb(
        source.left + (visible.left - dest.left) * sx,
        source.top + (visible.top - dest.top) * sy,
        source.left + (visible.right - dest.left) * sx,
        source.top + (visible.bottom - dest.top) * sy,
    );
    let (width, height) = device_size(canvas, visible);
    Some(Rendered {
        image: render_region(below, from, width, height)?,
        dest: visible,
    })
}

/// The size in device pixels (rounded up, at least 1) of `rect` (Image pixels) on `canvas`.
pub fn device_size(canvas: &sk::Canvas, rect: sk::Rect) -> (i32, i32) {
    let device = canvas.local_to_device_as_3x3().map_rect(rect).0;
    (
        device.width().ceil().max(1.0) as i32,
        device.height().ceil().max(1.0) as i32,
    )
}

/// The `region` of `below` (Image pixels) rendered offscreen into a `width`×`height` image,
/// whatever is visible on the canvas (for effects that read around what they show: blur's margin,
/// pixelate's whole blocks). Transparent where `region` leaves the Image.
pub fn render_region(
    below: &Scene<'_>,
    region: sk::Rect,
    width: i32,
    height: i32,
) -> Option<sk::Image> {
    if region.is_empty() || width <= 0 || height <= 0 {
        return None;
    }
    let mut surface = sk::surfaces::raster_n32_premul((width, height))?;
    let offscreen = surface.canvas();
    offscreen.clear(sk::Color::TRANSPARENT);
    offscreen.scale((
        width as f32 / region.width(),
        height as f32 / region.height(),
    ));
    offscreen.translate((-region.left, -region.top));
    below.draw(offscreen);
    Some(surface.image_snapshot())
}
