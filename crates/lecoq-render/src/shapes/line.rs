use lecoq_core::{Line, Style};
use skia_safe as sk;

use super::line_paint;
use crate::effects::with_shadow;

pub fn draw(canvas: &sk::Canvas, shape: &Line, style: &Style) {
    with_shadow(
        canvas,
        style.shadow,
        content_bounds(shape, style),
        |canvas| {
            let mut paint = line_paint(style);
            paint.set_stroke_cap(sk::PaintCap::Round);
            canvas.draw_line(crate::point(shape.from), crate::point(shape.to), &paint);
        },
    );
}

/// The drawn content bounds (the endpoints' box grown by the stroke): the single source both `draw`
/// (for the `with_shadow` layer) and [`crate::bounds`] (for the dirty region) consume, so they
/// cannot drift.
pub(crate) fn content_bounds(shape: &Line, style: &Style) -> sk::Rect {
    let bounds = sk::Rect::from_ltrb(
        shape.from.x.min(shape.to.x),
        shape.from.y.min(shape.to.y),
        shape.from.x.max(shape.to.x),
        shape.from.y.max(shape.to.y),
    );
    super::outset(bounds, style.thickness)
}
