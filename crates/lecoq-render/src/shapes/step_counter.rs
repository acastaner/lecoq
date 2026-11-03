//! A Step counter: a filled disc with its number centred in it.
//!
//! The digits come from a font bundled with egui (Ubuntu Light, emboldened), loaded into Skia from
//! memory: no system font enumeration (§7.2: fonts are enumerated only for the text tool), and the
//! screen, Flatten and the golden tests look the same on every machine.

use lecoq_core::{Rgba, StepCounter, Style};
use skia_safe as sk;

use crate::effects::with_shadow;

/// The digits' height relative to the disc's diameter.
const DIGIT_SCALE: f32 = 0.6;
/// The widest the number may be, relative to the diameter (long numbers shrink to fit).
const MAX_WIDTH: f32 = 0.78;

pub fn draw(canvas: &sk::Canvas, shape: &StepCounter, style: &Style, number: u32) {
    let disc = shape.disc(style);
    let centre = (shape.centre.x, shape.centre.y);
    let radius = disc.width() / 2.0;
    with_shadow(
        canvas,
        style.shadow,
        content_bounds(shape, style),
        |canvas| {
            if let Some(fill) = super::fill_paint(style) {
                canvas.draw_circle(centre, radius, &fill);
            }
            draw_number(
                canvas,
                centre,
                disc.width(),
                number,
                number_colour(style.fill),
            );
        },
    );
}

/// The drawn content bounds (the disc grown by the stroke): the single source both `draw` (for the
/// `with_shadow` layer) and [`crate::bounds`] (for the dirty region) consume, so they cannot drift.
pub(crate) fn content_bounds(shape: &StepCounter, style: &Style) -> sk::Rect {
    super::outset(crate::rect(shape.disc(style)), style.thickness)
}

/// White on dark fills, black on light ones (and without a fill).
fn number_colour(fill: Option<Rgba>) -> Rgba {
    match fill {
        Some(c) if c.a >= 128 => {
            let luma = 0.299 * f32::from(c.r) + 0.587 * f32::from(c.g) + 0.114 * f32::from(c.b);
            if luma < 160.0 {
                Rgba::WHITE
            } else {
                Rgba::BLACK
            }
        }
        _ => Rgba::BLACK,
    }
}

fn draw_number(canvas: &sk::Canvas, centre: (f32, f32), diameter: f32, number: u32, colour: Rgba) {
    let Some(typeface) = typeface() else {
        return;
    };
    let text = number.to_string();
    let mut font = sk::Font::from_typeface(typeface, diameter * DIGIT_SCALE);
    font.set_embolden(true);
    font.set_edging(sk::font::Edging::AntiAlias);
    font.set_subpixel(true);
    let (_, ink) = font.measure_str(&text, None);
    if ink.width() > diameter * MAX_WIDTH {
        font.set_size(font.size() * diameter * MAX_WIDTH / ink.width());
    }
    // Centre the digits' ink, not their advance box.
    let (_, ink) = font.measure_str(&text, None);
    let origin = (centre.0 - ink.center_x(), centre.1 - ink.center_y());
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(crate::color(colour));
    canvas.draw_str(&text, origin, &font, &paint);
}

thread_local! {
    static TYPEFACE: Option<sk::Typeface> = load_typeface();
}

fn typeface() -> Option<sk::Typeface> {
    TYPEFACE.with(|t| t.clone())
}

fn load_typeface() -> Option<sk::Typeface> {
    // The custom empty manager reads font data without scanning system fonts; it needs FreeType,
    // which Skia lacks on Windows, where the system manager loads data just as well.
    let manager = sk::FontMgr::custom_empty().unwrap_or_default();
    manager.new_from_data(sk::Data::new_copy(epaint_default_fonts::UBUNTU_LIGHT), None)
}
