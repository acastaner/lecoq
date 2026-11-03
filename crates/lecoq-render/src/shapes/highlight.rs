//! Highlight (§4.2): live on everything below it, in four modes.

use lecoq_core::{Highlight, HighlightMode, Style};
use skia_safe as sk;

use crate::effects::{Region, filter_below, paint_over, render_below};
use crate::scene::Scene;

/// Area mode: how much of what's outside the region shows through.
const AREA_DIM_ALPHA: u8 = 128;

/// Rec. 709 luma, applied to the sRGB values.
#[rustfmt::skip]
const GRAYSCALE: [f32; 20] = [
    0.2126, 0.7152, 0.0722, 0.0, 0.0,
    0.2126, 0.7152, 0.0722, 0.0, 0.0,
    0.2126, 0.7152, 0.0722, 0.0, 0.0,
    0.0,    0.0,    0.0,    1.0, 0.0,
];

pub fn draw(canvas: &sk::Canvas, shape: &Highlight, style: &Style, below: &Scene<'_>) {
    let rect = crate::rect(shape.rect);
    match style.highlight_mode {
        HighlightMode::Text => {
            let mut paint = sk::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(crate::color(style.highlight));
            paint.set_blend_mode(sk::BlendMode::Multiply);
            paint_over(canvas, below, Region::Inside(rect), &paint);
        }
        HighlightMode::Area => {
            let mut paint = sk::Paint::default();
            paint.set_color(sk::Color::from_argb(AREA_DIM_ALPHA, 0, 0, 0));
            // Only darken what's there: transparent parts of the Image stay transparent.
            paint.set_blend_mode(sk::BlendMode::SrcATop);
            paint_over(canvas, below, Region::Outside(rect), &paint);
        }
        HighlightMode::Grayscale => {
            let filter = sk::color_filters::matrix_row_major(&GRAYSCALE, None);
            filter_below(canvas, below, Region::Outside(rect), filter);
        }
        HighlightMode::Magnify => {
            // The middle 1/factor of the region, enlarged to fill it.
            let factor = style.magnification.max(1.0);
            let center = rect.center();
            let half = (rect.width() / factor / 2.0, rect.height() / factor / 2.0);
            let source = sk::Rect::from_ltrb(
                center.x - half.0,
                center.y - half.1,
                center.x + half.0,
                center.y + half.1,
            );
            if let Some(r) = render_below(canvas, below, source, rect) {
                canvas.draw_image_rect(&r.image, None, r.dest, &sk::Paint::default());
            }
        }
    }
}
