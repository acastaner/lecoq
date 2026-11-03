//! Inset (§4.6): its pixels stretched over its box, with the shadow and an optional border.

use std::cell::RefCell;
use std::sync::{Arc, Weak};

use lecoq_core::{Inset, PixelData, Pixels, Style};
use skia_safe as sk;

use crate::effects::with_shadow;

/// How many Insets' pixels stay converted for Skia, per thread: redrawing while one is dragged
/// mustn't convert its pixels on every frame.
const CACHE_LEN: usize = 8;

thread_local! {
    /// Most recently used last. Keyed by the pixels' `Arc`, which an Inset shares with its copies
    /// in the History.
    static CACHE: RefCell<Vec<(Weak<Pixels>, sk::Image)>> = const { RefCell::new(Vec::new()) };
}

pub fn draw(canvas: &sk::Canvas, shape: &Inset, style: &Style) {
    let image = skia_image(&shape.pixels);
    let rect = crate::rect(shape.rect);
    with_shadow(
        canvas,
        style.shadow,
        content_bounds(shape, style),
        |canvas| {
            canvas.draw_image_rect_with_sampling_options(
                &image,
                None,
                rect,
                sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::Linear),
                &sk::Paint::default(),
            );
            if let Some(border) = style.border {
                let mut paint = sk::Paint::default();
                paint.set_anti_alias(true);
                paint.set_style(sk::PaintStyle::Stroke);
                paint.set_stroke_width(style.thickness);
                paint.set_stroke_join(sk::PaintJoin::Miter);
                paint.set_color(crate::color(border));
                canvas.draw_rect(rect, &paint);
            }
        },
    );
}

/// The drawn content bounds (the box grown by the border stroke): the single source both `draw` (for
/// the `with_shadow` layer) and [`crate::bounds`] (for the dirty region) consume, so they cannot drift.
pub(crate) fn content_bounds(shape: &Inset, style: &Style) -> sk::Rect {
    super::outset(crate::rect(shape.rect), style.thickness)
}

fn skia_image(pixels: &Arc<Pixels>) -> sk::Image {
    CACHE.with_borrow_mut(|cache| {
        cache.retain(|(weak, _)| weak.strong_count() > 0);
        if let Some(i) = cache
            .iter()
            .position(|(weak, _)| std::ptr::eq(weak.as_ptr(), Arc::as_ptr(pixels)))
        {
            let entry = cache.remove(i);
            let image = entry.1.clone();
            cache.push(entry);
            return image;
        }
        let image = convert(pixels);
        if cache.len() == CACHE_LEN {
            cache.remove(0);
        }
        cache.push((Arc::downgrade(pixels), image.clone()));
        image
    })
}

fn convert(pixels: &Pixels) -> sk::Image {
    let (data, alpha) = match &pixels.data {
        PixelData::Rgb(rgb) => {
            let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
            for px in rgb.chunks_exact(3) {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            (sk::Data::new_copy(&rgba), sk::AlphaType::Opaque)
        }
        PixelData::Rgba(rgba) => (sk::Data::new_copy(rgba), sk::AlphaType::Unpremul),
    };
    let info = sk::ImageInfo::new(
        (pixels.width as i32, pixels.height as i32),
        sk::ColorType::RGBA8888,
        alpha,
        None,
    );
    sk::images::raster_from_data(&info, data, pixels.width as usize * 4)
        .expect("a Skia image from the Inset's pixels")
}
