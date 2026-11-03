use lecoq_core::{Arrow, Point, Style};
use skia_safe as sk;

use super::line_paint;
use crate::effects::with_shadow;

pub fn draw(canvas: &sk::Canvas, shape: &Arrow, style: &Style) {
    let heads = style.arrowheads;
    let length = shape.from.distance(shape.to);
    // Heads no longer than half the arrow each, so short arrows still show both.
    let head = Arrow::head_length(style.thickness).min(length / 2.0);
    with_shadow(
        canvas,
        style.shadow,
        content_bounds(shape, style),
        |canvas| {
            // The line stops inside each head, so its square end doesn't poke out beside the tip.
            let inset = head * 0.6;
            let mut from = shape.from;
            let mut to = shape.to;
            if heads.at_start() {
                from = towards(shape.from, shape.to, inset);
            }
            if heads.at_end() {
                to = towards(shape.to, shape.from, inset);
            }
            let mut paint = line_paint(style);
            paint.set_stroke_cap(sk::PaintCap::Butt);
            canvas.draw_line(crate::point(from), crate::point(to), &paint);

            let mut fill = paint.clone();
            fill.set_style(sk::PaintStyle::Fill);
            if heads.at_start() {
                canvas.draw_path(&head_path(shape.from, shape.to, head), &fill);
            }
            if heads.at_end() {
                canvas.draw_path(&head_path(shape.to, shape.from, head), &fill);
            }
        },
    );
}

/// The drawn content bounds: the endpoints' box grown by the arrow's reach — a head-length sideways
/// where the heads sit, or the stroke half-width when that is larger. The single source both `draw`
/// (for the `with_shadow` layer) and [`crate::bounds`] (for the dirty region) consume, so they
/// cannot drift.
pub(crate) fn content_bounds(shape: &Arrow, style: &Style) -> sk::Rect {
    let bounds = sk::Rect::from_ltrb(
        shape.from.x.min(shape.to.x),
        shape.from.y.min(shape.to.y),
        shape.from.x.max(shape.to.x),
        shape.from.y.max(shape.to.y),
    );
    let reach = Arrow::head_length(style.thickness).max(style.thickness);
    super::outset(bounds, reach)
}

/// `p` moved `by` towards `q`.
fn towards(p: Point, q: Point, by: f32) -> Point {
    let d = q - p;
    let len = d.length();
    if len == 0.0 {
        return p;
    }
    Point::new(p.x + d.x / len * by, p.y + d.y / len * by)
}

/// A triangular head with its tip on `tip`, pointing away from `tail`: `length` long and as wide.
fn head_path(tip: Point, tail: Point, length: f32) -> sk::Path {
    let d = tip - tail;
    let len = d.length().max(f32::EPSILON);
    let (ux, uy) = (d.x / len, d.y / len);
    let base = Point::new(tip.x - ux * length, tip.y - uy * length);
    let half = length / 2.0;
    let mut path = sk::PathBuilder::new();
    path.move_to(crate::point(tip));
    path.line_to((base.x - uy * half, base.y + ux * half));
    path.line_to((base.x + uy * half, base.y - ux * half));
    path.close();
    path.detach()
}
