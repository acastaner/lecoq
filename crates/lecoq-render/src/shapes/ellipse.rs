use lecoq_core::{Ellipse, Style};
use skia_safe as sk;

use super::{fill_paint, line_paint};
use crate::effects::with_shadow;

pub fn draw(canvas: &sk::Canvas, shape: &Ellipse, style: &Style) {
    let oval = crate::rect(shape.rect);
    with_shadow(
        canvas,
        style.shadow,
        content_bounds(shape, style),
        |canvas| {
            if let Some(fill) = fill_paint(style) {
                canvas.draw_oval(oval, &fill);
            }
            canvas.draw_oval(oval, &line_paint(style));
        },
    );
}

/// The drawn content bounds (the oval's box grown by the stroke): the single source both `draw` (for
/// the `with_shadow` layer) and [`crate::bounds`] (for the dirty region) consume, so they cannot drift.
pub(super) fn content_bounds(shape: &Ellipse, style: &Style) -> sk::Rect {
    super::outset(crate::rect(shape.rect), style.thickness)
}
