use lecoq_core::{Freehand, Style};
use skia_safe as sk;

use super::line_paint;
use crate::effects::with_shadow;

pub fn draw(canvas: &sk::Canvas, shape: &Freehand, style: &Style) {
    let points = &shape.points;
    let mut paint = line_paint(style);
    paint.set_stroke_cap(sk::PaintCap::Round);
    paint.set_stroke_join(sk::PaintJoin::Round);
    let mut path = sk::PathBuilder::new();
    path.move_to(crate::point(points[0]));
    // Quadratic curves through the midpoints, with the points as control points: a smooth stroke
    // that still starts and ends exactly where the pointer did. One point is a dot (a zero-length
    // segment with round caps).
    for i in 1..points.len().saturating_sub(1) {
        let (c, n) = (points[i], points[i + 1]);
        path.quad_to(crate::point(c), ((c.x + n.x) / 2.0, (c.y + n.y) / 2.0));
    }
    path.line_to(crate::point(points[points.len() - 1]));
    let path = path.detach();
    let bounds = super::outset(*path.bounds(), style.thickness);
    with_shadow(canvas, style.shadow, bounds, |canvas| {
        canvas.draw_path(&path, &paint);
    });
}
