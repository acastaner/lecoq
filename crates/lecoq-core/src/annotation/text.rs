//! Text (§4.1 T, §4.4): a box of text with one style (font, size, text colour, fill, shadow).
//!
//! The box's size comes from laying the text out, which needs fonts and so lives in
//! `lecoq-render`. The renderer registers its measuring function with [`set_measure`]; a [`Text`]
//! keeps the result (and the font and size it was measured with) so its bounds are known without
//! fonts. Anything that changes the text, the width or the style re-measures:
//! [`Text::set_text`], [`Text::set_width`], [`Shape::transform`], and
//! [`crate::Annotation::relayout`] (called by the Document after a style change; Image operations
//! that scale the font size, #33, must call it too).

use std::sync::OnceLock;

use super::Shape;
use crate::geometry::{Affine, Point, Rect, Vector};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

/// What to lay out: the text, its font and size (Image pixels), and the wrap width (`None`: no
/// wrapping, the box is as wide as the longest line).
#[derive(Clone, Copy, Debug)]
pub struct TextSpec<'a> {
    pub text: &'a str,
    pub font: &'a str,
    pub size: f32,
    pub wrap: Option<f32>,
}

/// Lays out a [`TextSpec`] and returns its (width, height) in Image pixels.
pub type MeasureFn = fn(&TextSpec<'_>) -> (f32, f32);

static MEASURE: OnceLock<MeasureFn> = OnceLock::new();

/// Register the function that lays text out (the renderer's). The first registration wins.
pub fn set_measure(f: MeasureFn) {
    let _ = MEASURE.set(f);
}

/// Lay out `spec` with the registered function, or estimate it when none is registered.
pub fn measure(spec: &TextSpec<'_>) -> (f32, f32) {
    match MEASURE.get() {
        Some(f) => f(spec),
        None => {
            let longest = spec.text.lines().map(|l| l.chars().count()).max();
            let lines = spec.text.split('\n').count();
            let width = longest.unwrap_or(0) as f32 * 0.6 * spec.size;
            (
                spec.wrap.unwrap_or(width),
                lines.max(1) as f32 * 1.2 * spec.size,
            )
        }
    }
}

/// Padding around the text inside the box, as a fraction of the font size.
const PADDING: f32 = 0.2;
/// §4.4: a new box is wide enough for this many characters (digit widths).
const MIN_CHARS: &str = "00000";

/// The laid-out size and what it was laid out with.
#[derive(Clone, Debug, Default, PartialEq)]
struct Layout {
    font: std::sync::Arc<str>,
    size: f32,
    /// The text's laid-out width and height.
    width: f32,
    height: f32,
    /// The narrowest an unwrapped box gets (§4.4: 4–5 characters).
    min_width: f32,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Text {
    /// The top-left corner of the box.
    pub origin: Point,
    text: String,
    /// A fixed width for the text (inside the padding), set by the left and right handles (§4.4).
    width: Option<f32>,
    /// Not in the clipboard JSON: measured again when a Document adds the Text.
    #[serde(skip)]
    layout: Layout,
}

impl Text {
    /// §4.3: font, size, text colour, fill colour (or none), shadow.
    pub const PROPERTIES: &[PropertyKind] = &[
        PropertyKind::Font,
        PropertyKind::FontSize,
        PropertyKind::TextColour,
        PropertyKind::FillColour,
        PropertyKind::Shadow,
    ];

    /// A box with its top-left corner at `origin`, laid out in `style`.
    pub fn new(origin: Point, text: &str, style: &Style) -> Self {
        let mut t = Self {
            origin,
            text: text.to_owned(),
            width: None,
            layout: Layout {
                font: std::sync::Arc::from(""),
                size: 0.0,
                width: 0.0,
                height: 0.0,
                min_width: 0.0,
            },
        };
        t.restyle(style);
        t
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: &str) {
        if self.text != text {
            self.text = text.to_owned();
            self.measure();
        }
    }

    /// The fixed width of the text, if one was set.
    pub fn width(&self) -> Option<f32> {
        self.width
    }

    pub fn set_width(&mut self, width: Option<f32>) {
        self.width = width.map(|w| w.max(self.layout.size));
        self.measure();
    }

    /// Where the text starts: the box's top-left corner inside the padding.
    pub fn text_origin(&self) -> Point {
        let pad = self.padding();
        self.origin + Vector::new(pad, pad)
    }

    /// Lay out again in `style` (its font or size changed).
    pub fn restyle(&mut self, style: &Style) {
        let unchanged = self.layout.font == style.font && self.layout.size == style.font_size;
        if unchanged && self.layout.height > 0.0 {
            return;
        }
        self.layout.font = style.font.clone();
        self.layout.size = style.font_size;
        self.layout.min_width = measure(&TextSpec {
            text: MIN_CHARS,
            font: &self.layout.font,
            size: self.layout.size,
            wrap: None,
        })
        .0;
        self.measure();
    }

    fn padding(&self) -> f32 {
        PADDING * self.layout.size
    }

    fn measure(&mut self) {
        let (width, height) = measure(&TextSpec {
            text: &self.text,
            font: &self.layout.font,
            size: self.layout.size,
            wrap: self.width,
        });
        self.layout.width = width;
        self.layout.height = height;
    }

    /// The text's size inside the padding.
    fn inner_size(&self) -> (f32, f32) {
        let width = match self.width {
            Some(w) => w,
            None => self.layout.width.max(self.layout.min_width),
        };
        (width, self.layout.height)
    }
}

impl Shape for Text {
    fn bounds(&self) -> Rect {
        let (w, h) = self.inner_size();
        let pad = self.padding();
        Rect::from_min_size(self.origin, w + 2.0 * pad, h + 2.0 * pad)
    }

    fn hit(&self, p: Point, _style: &Style, tolerance: f32) -> bool {
        // Anywhere in the box, filled or not.
        self.bounds().expand(tolerance).contains(p)
    }

    /// Text stays upright (§4.7). Moves and handle drags keep the top-left corner on the
    /// transformed one, and a horizontal scale sets a fixed width (§4.4: the left and right
    /// handles); the height always comes from the layout. Rotations and flips centre the upright
    /// box on the transformed centre.
    fn transform(&mut self, t: &Affine) {
        let b = self.bounds();
        if t.b == 0.0 && t.c == 0.0 && t.a > 0.0 && t.d > 0.0 {
            let to = t.apply_rect(b);
            if (t.a - 1.0).abs() > 1e-4 {
                let pad = self.padding();
                self.width = Some((to.width() - 2.0 * pad).max(self.layout.size));
                self.measure();
            }
            self.origin = Point::new(to.min.x, t.apply(b.min).y);
        } else {
            let c = t.apply(b.center());
            self.origin = Point::new(c.x - b.width() / 2.0, c.y - b.height() / 2.0);
        }
    }

    /// §4.7: the box moves to its new anchor but stays upright. Crop and resize keep the top-left
    /// corner on the Image (a resize also scales a fixed width here, and the font size through
    /// `Annotation::image_transform`, which then relayouts); rotations and flips centre the box on
    /// its transformed centre.
    fn image_transform(&mut self, t: &ImageTransform) {
        let a = t.affine();
        if a.b == 0.0 && a.c == 0.0 && a.a > 0.0 && a.d > 0.0 {
            self.origin = a.apply(self.origin);
            if let Some(w) = self.width {
                self.width = Some(w * a.a);
                self.measure();
            }
        } else {
            let b = self.bounds();
            let c = a.apply(b.center());
            self.origin = Point::new(c.x - b.width() / 2.0, c.y - b.height() / 2.0);
        }
    }

    fn properties(&self) -> &'static [PropertyKind] {
        Self::PROPERTIES
    }
}
