use lecoq_core::{Rectangle, Style};
use skia_safe as sk;

use super::{fill_paint, line_paint};
use crate::effects::with_shadow;

pub fn draw(canvas: &sk::Canvas, shape: &Rectangle, style: &Style) {
    let rect = crate::rect(shape.rect);
    with_shadow(
        canvas,
        style.shadow,
        content_bounds(shape, style),
        |canvas| {
            if let Some(fill) = fill_paint(style) {
                canvas.draw_rect(rect, &fill);
            }
            let mut line = line_paint(style);
            line.set_stroke_join(sk::PaintJoin::Miter);
            canvas.draw_rect(rect, &line);
        },
    );
}

/// The drawn content bounds (the rect grown by the stroke): the single source both `draw` (for the
/// `with_shadow` layer) and [`crate::bounds`] (for the dirty region) consume, so they cannot drift.
pub(super) fn content_bounds(shape: &Rectangle, style: &Style) -> sk::Rect {
    super::outset(crate::rect(shape.rect), style.thickness)
}
