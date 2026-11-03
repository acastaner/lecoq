//! Annotations (GLOSSARY): one module per type, gathered into the [`AnnotationShape`] enum.
//!
//! Adding an Annotation type (ellipse, line, arrow, text, …):
//! 1. a module here with a struct implementing [`Shape`] (geometry in Image pixels), including
//!    how it follows Image operations ([`Shape::image_transform`]),
//! 2. one line in the `shapes!` list below,
//! 3. its drawing in `lecoq-render` (`shapes/<type>.rs` plus one arm in `shapes::draw`),
//! 4. a tool that creates it in the `lecoq` crate (`tools/`).

mod ellipse;
mod freehand;
mod highlight;
mod inset;
mod line;
mod obfuscate;
mod rectangle;
mod step_counter;
pub mod text;

pub use ellipse::Ellipse;
pub use freehand::Freehand;
pub use highlight::Highlight;
pub use inset::Inset;
pub use line::{Arrow, Line};
pub use obfuscate::Obfuscate;
pub use rectangle::Rectangle;
pub use step_counter::StepCounter;
pub use text::Text;

use crate::geometry::{Affine, Point, Rect};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

/// What every Annotation type provides to the Document, the select tool and the property bar.
pub trait Shape {
    /// The geometric bounds, without stroke width or shadow. The selection box is drawn around it
    /// and its handles resize it.
    fn bounds(&self) -> Rect;

    /// The bounds for this style, for types whose size is a style property (Step counter).
    fn styled_bounds(&self, _style: &Style) -> Rect {
        self.bounds()
    }

    /// Is `p` on the Annotation? `tolerance` (Image pixels) widens thin strokes so they can be
    /// clicked.
    fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool;

    /// Move, scale or otherwise transform the geometry (moves, handle resizes, and the Image
    /// operations of ADR 0001).
    fn transform(&mut self, t: &Affine);

    /// Follow an Image operation (ADR 0001). Each type decides how: shapes, lines, arrows,
    /// freehand strokes and Insets apply `t.affine()` (as [`Rectangle`] does), so they rotate and
    /// flip with the Image; text and Step counters stay upright: their box goes through
    /// `t.upright_rect(..)` (a point anchor through `t.affine()`). Style scaling on resize is done
    /// by [`Annotation::image_transform`], not here.
    fn image_transform(&mut self, t: &ImageTransform);

    /// The style properties the property bar shows for this type (§4.3), in display order.
    fn properties(&self) -> &'static [PropertyKind];

    /// Does it draw from the pixels under it (highlight, obfuscate)? Such a type means nothing
    /// without the Image, so it is left out of the PNG that copying Annotations publishes (§6.2);
    /// it is still in the clipboard JSON. Effect types return true.
    fn needs_pixels_below(&self) -> bool {
        false
    }

    /// A transformed copy.
    fn transformed(&self, t: &Affine) -> Self
    where
        Self: Clone + Sized,
    {
        let mut s = self.clone();
        s.transform(t);
        s
    }
}

/// Declares [`AnnotationShape`] with one variant per type and forwards [`Shape`] to it.
macro_rules! shapes {
    ($($variant:ident($ty:ty)),+ $(,)?) => {
        /// The geometry of an Annotation, one variant per Annotation type.
        ///
        /// Serialised (clipboard JSON, §6.2) with a `type` tag in snake_case: `{"type": "rectangle", ..}`.
        #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        pub enum AnnotationShape {
            $($variant($ty),)+
        }

        impl Shape for AnnotationShape {
            fn bounds(&self) -> Rect {
                match self { $(Self::$variant(s) => s.bounds(),)+ }
            }
            fn styled_bounds(&self, style: &Style) -> Rect {
                match self { $(Self::$variant(s) => s.styled_bounds(style),)+ }
            }
            fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool {
                match self { $(Self::$variant(s) => s.hit(p, style, tolerance),)+ }
            }
            fn transform(&mut self, t: &Affine) {
                match self { $(Self::$variant(s) => s.transform(t),)+ }
            }
            fn image_transform(&mut self, t: &ImageTransform) {
                match self { $(Self::$variant(s) => s.image_transform(t),)+ }
            }
            fn properties(&self) -> &'static [PropertyKind] {
                match self { $(Self::$variant(s) => s.properties(),)+ }
            }
            fn needs_pixels_below(&self) -> bool {
                match self { $(Self::$variant(s) => s.needs_pixels_below(),)+ }
            }
        }

        $(impl From<$ty> for AnnotationShape {
            fn from(s: $ty) -> Self {
                Self::$variant(s)
            }
        })+
    };
}

shapes! {
    Rectangle(Rectangle),
    StepCounter(StepCounter),
    Text(Text),
    Ellipse(Ellipse),
    Line(Line),
    Arrow(Arrow),
    Freehand(Freehand),
    Highlight(Highlight),
    Obfuscate(Obfuscate),
    Inset(Inset),
}

/// Identifies an Annotation within its Document. Never reused, so it survives undo and redo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnnotationId(pub u64);

/// An Annotation on the Image: its geometry and its style.
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub id: AnnotationId,
    pub shape: AnnotationShape,
    pub style: Style,
}

impl Annotation {
    pub fn bounds(&self) -> Rect {
        self.shape.styled_bounds(&self.style)
    }

    pub fn hit(&self, p: Point, tolerance: f32) -> bool {
        self.shape.hit(p, &self.style, tolerance)
    }

    /// Bring geometry that depends on the style up to date after a style change (a text box's
    /// layout, #28).
    pub fn relayout(&mut self) {
        if let AnnotationShape::Text(t) = &mut self.shape {
            t.restyle(&self.style);
        }
    }

    /// The area it covers on the Image: its bounds plus half the line. A crop removes an
    /// Annotation whose extent doesn't overlap the new Image.
    pub fn extent(&self) -> Rect {
        self.bounds().expand(self.style.thickness / 2.0)
    }

    /// Follow an Image operation (ADR 0001). A resize also scales the Style's sizes (§4.7): every
    /// size property given in Image pixels is multiplied by `t.size_scale()` here (thickness, text's
    /// font size, the Step counter's size).
    pub fn image_transform(&mut self, t: &ImageTransform) {
        self.shape.image_transform(t);
        let s = t.size_scale();
        if s != 1.0 {
            self.style.thickness *= s;
            self.style.font_size *= s;
            self.style.counter_size *= s;
            self.style.strength *= s;
            self.relayout();
        }
    }
}
