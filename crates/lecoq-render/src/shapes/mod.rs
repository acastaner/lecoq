//! Drawing each Annotation type, one module per type. The canvas is in Image pixels.
//!
//! Adding a type: a module with `draw(canvas, shape, style)` and, unless it is an effect that draws
//! within its own box, a `content_bounds(shape, style)`; then one arm in [`draw`] and one in
//! [`content_bounds`] here. The two dispatches sit side by side so a new shape's drawing and its
//! drawn bounds are registered together. Effect types (highlight, obfuscate) that act on what's
//! beneath them also take `below` (see `effects`).

mod arrow;
mod ellipse;
mod freehand;
mod highlight;
mod inset;
mod line;
mod obfuscate;
mod rectangle;
mod step_counter;
mod text;

use lecoq_core::{Annotation, AnnotationShape, Shape, Style};
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

/// The rectangle of Image pixels a shape's `draw` grows its geometry to, before any shadow: the
/// bound each shape's own `draw` confines its `with_shadow` layer to, and the one [`crate::bounds`]
/// grows for the dirty region. It sits beside [`draw`] so the two dispatches stay in step; within
/// each shape module the one `content_bounds` is still the single source, so a shape's drawing and
/// its dirty region cannot drift.
pub(crate) fn content_bounds(shape: &AnnotationShape, style: &Style) -> sk::Rect {
    match shape {
        AnnotationShape::Rectangle(s) => rectangle::content_bounds(s, style),
        AnnotationShape::Ellipse(s) => ellipse::content_bounds(s, style),
        AnnotationShape::Line(s) => line::content_bounds(s, style),
        AnnotationShape::Arrow(s) => arrow::content_bounds(s, style),
        // The smoothed path stays within the points' convex hull, so the points' bounds are a
        // superset of the drawn curve's: the draw site bounds its layer to the tighter path, so this
        // deliberately differs and stays a separate (conservative) derivation.
        AnnotationShape::Freehand(s) => crate::rect(s.bounds().expand(style.thickness)),
        AnnotationShape::StepCounter(s) => step_counter::content_bounds(s, style),
        AnnotationShape::Text(s) => text::content_bounds(s, style),
        AnnotationShape::Inset(s) => inset::content_bounds(s, style),
        // Effects draw within their own box (clipped to the Image when composited).
        AnnotationShape::Highlight(s) => crate::rect(s.rect),
        AnnotationShape::Obfuscate(s) => crate::rect(s.rect),
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
