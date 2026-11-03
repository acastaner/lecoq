//! Obfuscate (§4.2): live on everything below it, pixelated or blurred.
//!
//! Both are computed in Image pixels, independent of the view, so the screen shows exactly what
//! Flatten writes: pixelate's blocks sit on a grid anchored at the region's top-left corner, and
//! blur reads a margin around the region (clamped at the Image's edge).

use lecoq_core::{Obfuscate, ObfuscateMode, Style};
use skia_safe as sk;

use crate::effects::{device_size, render_region};
use crate::scene::Scene;

/// Pixelate: how many samples per block side are averaged (at most).
const SAMPLES_PER_BLOCK: f32 = 4.0;

pub fn draw(canvas: &sk::Canvas, shape: &Obfuscate, style: &Style, below: &Scene<'_>) {
    let rect = crate::rect(shape.rect).sorted();
    // Only the part over the Image is obfuscated (and read).
    let mut area = rect;
    if !area.intersect(below.image_rect) {
        return;
    }
    let Some(clip) = canvas.local_clip_bounds() else {
        return;
    };
    let mut visible = area;
    if !visible.intersect(clip) {
        return;
    }
    let strength = style.strength.max(1.0);
    match style.obfuscate_mode {
        ObfuscateMode::Pixelate => pixelate(canvas, below, rect, area, visible, strength),
        ObfuscateMode::Blur => blur(canvas, below, area, visible, strength),
    }
}

/// Blocks of `size` Image pixels on a grid anchored at `rect`'s top-left corner, each the average
/// of what's below it within `area` (the region over the Image). Only the blocks `visible` touches
/// are computed.
fn pixelate(
    canvas: &sk::Canvas,
    below: &Scene<'_>,
    rect: sk::Rect,
    area: sk::Rect,
    visible: sk::Rect,
    size: f32,
) {
    let first = (
        ((visible.left - rect.left) / size).floor(),
        ((visible.top - rect.top) / size).floor(),
    );
    let last = (
        ((visible.right - rect.left) / size).ceil(),
        ((visible.bottom - rect.top) / size).ceil(),
    );
    let cols = (last.0 - first.0).max(1.0) as i32;
    let rows = (last.1 - first.1).max(1.0) as i32;
    let blocks = sk::Rect::from_xywh(
        rect.left + first.0 * size,
        rect.top + first.1 * size,
        cols as f32 * size,
        rows as f32 * size,
    );

    // Render the blocks at `m` samples per block side, then average each block's samples.
    let m = size.ceil().clamp(1.0, SAMPLES_PER_BLOCK) as i32;
    let Some(samples) = render_region(below, blocks, cols * m, rows * m) else {
        return;
    };
    let info = sk::ImageInfo::new(
        (cols * m, rows * m),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    let row_bytes = (cols * m) as usize * 4;
    let mut pixels = vec![0u8; row_bytes * (rows * m) as usize];
    if !samples.read_pixels(
        &info,
        &mut pixels,
        row_bytes,
        (0, 0),
        sk::image::CachingHint::Disallow,
    ) {
        return;
    }

    let step = size / m as f32;
    let inside = |i: i32, j: i32| {
        let x = blocks.left + (i as f32 + 0.5) * step;
        let y = blocks.top + (j as f32 + 0.5) * step;
        (area.left..area.right).contains(&x) && (area.top..area.bottom).contains(&y)
    };
    let mut averaged = vec![0u8; cols as usize * rows as usize * 4];
    for by in 0..rows {
        for bx in 0..cols {
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            // A sliver of a block with no sample centre inside `area` takes all of its samples.
            for pass in [true, false] {
                for j in by * m..(by + 1) * m {
                    for i in bx * m..(bx + 1) * m {
                        if pass && !inside(i, j) {
                            continue;
                        }
                        let p = j as usize * row_bytes + i as usize * 4;
                        for c in 0..4 {
                            sum[c] += pixels[p + c] as u32;
                        }
                        count += 1;
                    }
                }
                if count > 0 {
                    break;
                }
            }
            let o = (by * cols + bx) as usize * 4;
            for c in 0..4 {
                averaged[o + c] = ((sum[c] + count / 2) / count) as u8;
            }
        }
    }
    let block_info = sk::ImageInfo::new(
        (cols, rows),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    let Some(image) = sk::images::raster_from_data(
        &block_info,
        sk::Data::new_copy(&averaged),
        cols as usize * 4,
    ) else {
        return;
    };
    canvas.save();
    canvas.clip_rect(area, sk::ClipOp::Intersect, true);
    canvas.draw_image_rect_with_sampling_options(
        &image,
        None,
        blocks,
        sk::SamplingOptions::new(sk::FilterMode::Nearest, sk::MipmapMode::None),
        &sk::Paint::default(),
    );
    canvas.restore();
}

/// A Gaussian blur of what's below, with `radius` (Image pixels) converted to a sigma as Skia
/// does. Reads a 3-sigma margin around what's shown, clamped at the Image's edge.
fn blur(canvas: &sk::Canvas, below: &Scene<'_>, area: sk::Rect, visible: sk::Rect, radius: f32) {
    let sigma = 0.57735 * radius + 0.5;
    let mut source = visible.with_outset((3.0 * sigma, 3.0 * sigma));
    if !source.intersect(below.image_rect) {
        return;
    }
    let (width, height) = device_size(canvas, source);
    let Some(image) = render_region(below, source, width, height) else {
        return;
    };
    let scale = (
        width as f32 / source.width(),
        height as f32 / source.height(),
    );
    let Some(filter) = sk::image_filters::blur(
        (sigma * scale.0, sigma * scale.1),
        sk::TileMode::Clamp,
        None,
        None,
    ) else {
        return;
    };
    // Blurred into a surface of the same size: Clamp repeats the edge pixels of `image`, so
    // the Image's edge doesn't fade in from transparent.
    let Some(mut surface) = sk::surfaces::raster_n32_premul((width, height)) else {
        return;
    };
    let mut paint = sk::Paint::default();
    paint.set_image_filter(filter);
    surface.canvas().clear(sk::Color::TRANSPARENT);
    surface.canvas().draw_image(&image, (0, 0), Some(&paint));
    let blurred = surface.image_snapshot();
    canvas.save();
    canvas.clip_rect(area, sk::ClipOp::Intersect, true);
    canvas.draw_image_rect_with_sampling_options(
        &blurred,
        None,
        source,
        sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None),
        &sk::Paint::default(),
    );
    canvas.restore();
}
