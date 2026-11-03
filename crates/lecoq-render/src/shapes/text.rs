use lecoq_core::annotation::text::TextSpec;
use lecoq_core::{Shape, Style, Text};
use skia_safe as sk;

use super::fill_paint;
use crate::effects::with_shadow;

/// The box's fill (if any), then the text in the text colour.
pub fn draw(canvas: &sk::Canvas, shape: &Text, style: &Style) {
    let paragraph = crate::text::paragraph(
        &TextSpec {
            text: shape.text(),
            font: &style.font,
            size: style.font_size,
            wrap: shape.width(),
        },
        crate::color(style.line),
    );
    let origin = shape.text_origin();
    with_shadow(
        canvas,
        style.shadow,
        content_bounds(shape, style),
        |canvas| {
            if let Some(fill) = fill_paint(style) {
                canvas.draw_rect(crate::rect(shape.bounds()), &fill);
            }
            paragraph.paint(canvas, (origin.x, origin.y));
        },
    );
}

/// The drawn content bounds (the text box grown by the font size, which covers the shadow/ascenders
/// Skia may paint past the box): the single source both `draw` (for the `with_shadow` layer) and
/// [`crate::bounds`] (for the dirty region) consume, so they cannot drift.
pub(crate) fn content_bounds(shape: &Text, style: &Style) -> sk::Rect {
    super::outset(crate::rect(shape.bounds()), style.font_size)
}
