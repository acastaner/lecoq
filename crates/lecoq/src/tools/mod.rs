//! The editor's tools (§4.1): the tool strip's entries and what each does with the pointer.
//!
//! Everything static about a tool lives in one row of the [`TOOLS`] table — its config key, name,
//! hint, shortcut key, property list and constructor. Adding a tool is one entry there plus its
//! module here (a type implementing [`Tool`]; for a drag-a-box shape, reuse [`drag_box::DragBox`]
//! as `rectangle.rs` does). A tool that creates a new Annotation type also gets an arm in
//! [`ToolId::for_shape`], the one tool fact keyed by shape rather than by tool, and a painted icon
//! in [`crate::ui::tool_strip`].
//!
//! Tools work in Image pixels ([`lecoq_core::Point`]); the canvas maps the pointer for them.

mod crop;
mod drag_box;
mod drag_segment;
mod freehand;
pub mod select;
mod step_counter;
pub mod text;

use lecoq_core::{
    AnnotationShape, Arrow, Document, Ellipse, Freehand, Highlight, Inset, Line, Obfuscate, Point,
    PropertyKind, Rect, Rectangle, StepCounter, Style, Text,
};

/// Every tool, in tool-strip order (§4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolId {
    Select,
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Freehand,
    Text,
    StepCounter,
    Highlight,
    Obfuscate,
    Crop,
    /// Not in the tool strip: Insets are pasted or dropped, not drawn (§4.6). It owns their default
    /// style (§4.3), saved as `tools.inset`.
    Inset,
}

/// One row of the tool descriptor table: everything static about a tool, so a new tool is one
/// entry here plus its module. Behaviour (`create`, `default_style`) is held as a constructor
/// pointer rather than inline, since trait objects and styles aren't `const`.
struct ToolInfo {
    id: ToolId,
    /// The tool's key under `tools` in `config.toml` (§8.3): `tools.rectangle`, ….
    config_key: &'static str,
    /// The tool's name: its button's accessibility label.
    name: &'static str,
    /// The property bar's hint when the tool has no controls to show.
    hint: &'static str,
    /// The single-key shortcut (Greenshot's). `None` for the Inset, which isn't in the tool strip.
    key: Option<egui::Key>,
    /// Shown in the tool strip, in this order (§4.1). The Inset is pasted or dropped, not drawn
    /// (§4.6), so it is styled but not in the strip.
    in_strip: bool,
    /// The property-bar controls for this tool's defaults when nothing is selected (§4.3).
    properties: &'static [PropertyKind],
    /// Builds the tool's behaviour; `None` while it isn't built yet (its button is disabled).
    create: Option<fn() -> Box<dyn Tool>>,
    /// The tool's style before the user changes any of its properties (§4.3, §8.3); `None` means
    /// [`Style::default`].
    default_style: Option<fn() -> Style>,
}

/// Every tool, in tool-strip order then the Inset (§4.1, §4.3). The single source for each tool's
/// static data; [`ToolId::ALL`] and [`ToolId::STYLED`] are derived from it.
const TOOLS: &[ToolInfo] = &[
    ToolInfo {
        id: ToolId::Select,
        config_key: "select",
        name: "Select",
        hint: "Click an Annotation to select it",
        key: Some(egui::Key::Escape),
        in_strip: true,
        properties: &[],
        create: Some(|| Box::new(select::Select::default())),
        default_style: None,
    },
    ToolInfo {
        id: ToolId::Rectangle,
        config_key: "rectangle",
        name: "Rectangle",
        hint: "",
        key: Some(egui::Key::R),
        in_strip: true,
        properties: Rectangle::PROPERTIES,
        create: Some(|| Box::new(drag_box::rectangle())),
        default_style: None,
    },
    ToolInfo {
        id: ToolId::Ellipse,
        config_key: "ellipse",
        name: "Ellipse",
        hint: "",
        key: Some(egui::Key::E),
        in_strip: true,
        properties: Ellipse::PROPERTIES,
        create: Some(|| Box::new(drag_box::ellipse())),
        default_style: None,
    },
    ToolInfo {
        id: ToolId::Line,
        config_key: "line",
        name: "Line",
        hint: "",
        key: Some(egui::Key::L),
        in_strip: true,
        properties: Line::PROPERTIES,
        create: Some(|| Box::new(drag_segment::line())),
        default_style: None,
    },
    ToolInfo {
        id: ToolId::Arrow,
        config_key: "arrow",
        name: "Arrow",
        hint: "",
        key: Some(egui::Key::A),
        in_strip: true,
        properties: Arrow::PROPERTIES,
        create: Some(|| Box::new(drag_segment::arrow())),
        default_style: None,
    },
    ToolInfo {
        id: ToolId::Freehand,
        config_key: "freehand",
        name: "Freehand",
        hint: "",
        key: Some(egui::Key::F),
        in_strip: true,
        properties: Freehand::PROPERTIES,
        create: Some(|| Box::new(freehand::FreehandTool::default())),
        default_style: None,
    },
    ToolInfo {
        id: ToolId::Text,
        config_key: "text",
        name: "Text",
        hint: "",
        key: Some(egui::Key::T),
        in_strip: true,
        properties: Text::PROPERTIES,
        create: Some(|| Box::new(text::TextTool)),
        default_style: None,
    },
    ToolInfo {
        id: ToolId::StepCounter,
        config_key: "step_counter",
        name: "Step counter",
        hint: "",
        key: Some(egui::Key::I),
        in_strip: true,
        properties: StepCounter::PROPERTIES,
        create: Some(|| Box::new(step_counter::PlaceCounter::default())),
        default_style: Some(step_counter::default_style),
    },
    ToolInfo {
        id: ToolId::Highlight,
        config_key: "highlight",
        name: "Highlight",
        hint: "",
        key: Some(egui::Key::H),
        in_strip: true,
        properties: Highlight::PROPERTIES,
        create: Some(|| Box::new(drag_box::highlight())),
        default_style: None,
    },
    ToolInfo {
        id: ToolId::Obfuscate,
        config_key: "obfuscate",
        name: "Obfuscate",
        hint: "",
        key: Some(egui::Key::O),
        in_strip: true,
        properties: Obfuscate::PROPERTIES,
        create: Some(|| Box::new(drag_box::obfuscate())),
        default_style: None,
    },
    ToolInfo {
        id: ToolId::Crop,
        config_key: "crop",
        name: "Crop",
        hint: "Drag the region to keep",
        key: Some(egui::Key::C),
        in_strip: true,
        properties: &[],
        create: Some(|| Box::new(crop::Crop::default())),
        default_style: None,
    },
    ToolInfo {
        id: ToolId::Inset,
        config_key: "inset",
        name: "Inset",
        hint: "",
        key: None,
        in_strip: false,
        properties: Inset::PROPERTIES,
        create: None,
        default_style: None,
    },
];

impl ToolId {
    /// This tool's row in [`TOOLS`].
    fn info(self) -> &'static ToolInfo {
        TOOLS
            .iter()
            .find(|info| info.id == self)
            .expect("every ToolId has a TOOLS row")
    }

    /// §4.1 order: the tools the strip shows, in order.
    pub fn all() -> impl Iterator<Item = ToolId> {
        TOOLS
            .iter()
            .filter(|info| info.in_strip)
            .map(|info| info.id)
    }

    /// Every tool with a style of its own (§4.3, §8.3): the tool strip's, and the Inset's.
    pub fn styled() -> impl Iterator<Item = ToolId> {
        TOOLS.iter().map(|info| info.id)
    }

    /// The tool's name: its button's accessibility label.
    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// The tool's key under `tools` in `config.toml` (§8.3): `tools.rectangle`, ….
    pub fn config_key(self) -> &'static str {
        self.info().config_key
    }

    /// The tool that creates this kind of Annotation: restyling an Annotation also sets that
    /// tool's default (§4.3). A new Annotation type adds an arm here.
    pub fn for_shape(shape: &AnnotationShape) -> ToolId {
        match shape {
            AnnotationShape::Rectangle(_) => ToolId::Rectangle,
            AnnotationShape::Ellipse(_) => ToolId::Ellipse,
            AnnotationShape::Line(_) => ToolId::Line,
            AnnotationShape::Arrow(_) => ToolId::Arrow,
            AnnotationShape::Freehand(_) => ToolId::Freehand,
            AnnotationShape::Inset(_) => ToolId::Inset,
            AnnotationShape::Text(_) => ToolId::Text,
            AnnotationShape::StepCounter(_) => ToolId::StepCounter,
            AnnotationShape::Highlight(_) => ToolId::Highlight,
            AnnotationShape::Obfuscate(_) => ToolId::Obfuscate,
        }
    }

    /// The single-key shortcut (Greenshot's). `None` for the Inset, which isn't in the tool strip.
    pub fn key(self) -> Option<egui::Key> {
        self.info().key
    }

    /// The tool's behaviour, or `None` while it isn't built yet (its button is disabled).
    pub fn create(self) -> Option<Box<dyn Tool>> {
        self.info().create.map(|make| make())
    }

    pub fn is_available(self) -> bool {
        self.info().create.is_some()
    }

    /// The property-bar controls for this tool's defaults when nothing is selected (§4.3).
    pub fn properties(self) -> &'static [PropertyKind] {
        self.info().properties
    }

    /// The tool's style before the user changes any of its properties (§4.3, §8.3).
    pub fn default_style(self) -> Style {
        match self.info().default_style {
            Some(style) => style(),
            None => Style::default(),
        }
    }

    /// The property bar's hint when the tool has no controls to show.
    pub fn hint(self) -> &'static str {
        self.info().hint
    }
}

/// The pointer, in Image pixels.
#[derive(Clone, Copy, Debug)]
pub struct Pointer {
    pub pos: Point,
    pub modifiers: egui::Modifiers,
}

/// What a tool gets to work with during a pointer event.
pub struct ToolContext<'a> {
    pub doc: &'a mut Document,
    /// This tool's current style (its defaults, §4.3), for the Annotations it creates.
    pub style: &'a Style,
    /// Image pixels per screen point: converts on-screen sizes (handles, click slop) to Image
    /// pixels.
    pub image_per_point: f32,
    /// The crop tool's region (§4.1 C), in Image pixels. Not a Document edit until applied.
    pub crop: &'a mut Option<Rect>,
}

/// A tool's pointer handling. The canvas calls `press`, then `drag` for each move while the
/// button is held, then `release`. A click without movement is `press` then `release`.
///
/// A tool that edits opens a gesture on the Document in `press` and commits it in `release`, so
/// one drag is one History step (§4.8).
pub trait Tool {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer);
    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer);
    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer);

    /// The rubber band being dragged out, in Image pixels (the select tool).
    fn rubber_band(&self) -> Option<lecoq_core::Rect> {
        None
    }

    /// Abandon an unfinished gesture (the tool is being switched away from mid-drag).
    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}
