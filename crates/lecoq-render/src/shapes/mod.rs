//! Drawing each Annotation type, one module per type. The canvas is in Image pixels.
//!
//! Adding a type: a module with `draw(canvas, shape, style)` and one arm in [`draw`]. Effect types
//! (highlight, obfuscate) that act on what's beneath them also take `below` (see `effects`).

pub(crate) mod arrow;
pub(crate) mod ellipse;
mod freehand;
mod highlight;
pub(crate) mod inset;
pub(crate) mod line;
mod obfuscate;
pub(crate) mod rectangle;
pub(crate) mod step_counter;
pub(crate) mod text;

use lecoq_core::{Annotation, AnnotationShape};
use skia_safe as sk;

use crate::scene::Scene;

/// Draw `annotation`; `below` is everything under it in z-order (the Image and lower Annotations),
/// and `below.doc` the whole Document (Step counter numbers).
pub fn draw(canvas: &sk::Canvas, annotation: &Annotation, below: &Scene<'_>) {
    let style = &annotation.style;
    match &annotation.shape {
        AnnotationShape::Rectangle(s) => rectangle::draw(canvas, s, style),
        AnnotationShape::StepCounter(s) => {
            let number = below.doc.step_number(annotation.id).unwrap_or_default();
            step_counter::draw(canvas, s, style, number)
        }
        AnnotationShape::Text(s) => text::draw(canvas, s, style),
        AnnotationShape::Ellipse(s) => ellipse::draw(canvas, s, style),
        AnnotationShape::Line(s) => line::draw(canvas, s, style),
        AnnotationShape::Arrow(s) => arrow::draw(canvas, s, style),
        AnnotationShape::Freehand(s) => freehand::draw(canvas, s, style),
        AnnotationShape::Highlight(s) => highlight::draw(canvas, s, style, below),
        AnnotationShape::Obfuscate(s) => obfuscate::draw(canvas, s, style, below),
        AnnotationShape::Inset(s) => inset::draw(canvas, s, style),
    }
}

/// A shape's geometric bounds grown by `by` (its stroke's half-width, usually `style.thickness`,
/// or an arrow's head reach) to give the drawn bounds `effects::with_shadow` bounds its layer to.
pub(crate) fn outset(r: sk::Rect, by: f32) -> sk::Rect {
    sk::Rect::from_ltrb(r.left - by, r.top - by, r.right + by, r.bottom + by)
}

/// An anti-aliased stroke paint for the Style's line.
pub(crate) fn line_paint(style: &lecoq_core::Style) -> sk::Paint {
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Stroke);
    paint.set_stroke_width(style.thickness);
    paint.set_color(crate::color(style.line));
    paint
}

/// An anti-aliased fill paint, if the Style has a fill.
pub(crate) fn fill_paint(style: &lecoq_core::Style) -> Option<sk::Paint> {
    let fill = style.fill?;
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Fill);
    paint.set_color(crate::color(fill));
    Some(paint)
}
