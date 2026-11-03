//! The lecoq domain (spec §7.1): the Document (Image pixels, Annotations, Selection, History),
//! Annotation types and their geometric transforms, Image operations, Step counter numbering and
//! the clipboard JSON. No UI and no I/O.
//!
//! Tests drive this crate through the headless App (spec §9), not directly.

pub mod annotation;
pub mod clipboard;
pub mod document;
pub mod geometry;
mod history;
pub mod image_op;
pub mod style;

pub use annotation::{
    Annotation, AnnotationId, AnnotationShape, Arrow, Ellipse, Freehand, Highlight, Inset, Line,
    Obfuscate, Rectangle, Shape, StepCounter, Text,
};
pub use clipboard::CopiedAnnotation;
pub use document::{Document, PixelData, Pixels, Restack};
pub use geometry::{Affine, Handle, Point, Rect, Vector};
pub use history::HISTORY_PIXEL_CAP;
pub use image_op::{ImageTransform, PixelRect};
pub use style::{Arrowheads, HighlightMode, ObfuscateMode, Property, PropertyKind, Rgba, Style};
