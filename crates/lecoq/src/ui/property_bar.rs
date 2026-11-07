//! The property bar (§2, §4.3): the active tool's controls, or the selection's.
//!
//! One control per [`PropertyKind`], in [`control`]. A new property adds one arm there (and its
//! accessibility label constant). Controls report changes as [`Property`] values; the app turns
//! them into commands.

use lecoq_core::{
    Arrowheads, HighlightMode, ObfuscateMode, Property, PropertyKind, Restack, Rgba, Style,
};

use crate::strings;

/// What the property bar shows this frame.
pub struct PropertyBar<'a> {
    pub style: &'a Style,
    pub properties: &'a [PropertyKind],
    /// Annotations are selected: the controls restyle them, and Delete is offered.
    pub selection: bool,
    /// The crop tool's region size, when it has one: shown with Apply and Cancel (§2).
    pub crop: Option<lecoq_codec::Size>,
    /// Shown when there are no controls.
    pub hint: &'a str,
    /// The installed font families for the font control (listed on first use of the text tool).
    pub fonts: &'a [String],
}

/// What the user did in the property bar this frame.
#[derive(Default)]
pub struct Output {
    pub changes: Vec<Property>,
    /// The property whose control is still being interacted with (a colour picker open, a value
    /// being dragged): its changes form one History step.
    pub active: Option<PropertyKind>,
    pub delete: bool,
    pub apply_crop: bool,
    pub cancel_crop: bool,
    /// Bring to front or Send to back was clicked.
    pub restack: Option<Restack>,
}

pub fn show(ui: &mut egui::Ui, bar: &PropertyBar<'_>) -> Output {
    let mut out = Output::default();
    egui::Panel::top("property_bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for kind in bar.properties {
                control(ui, *kind, bar, &mut out);
            }
            if bar.selection
                && ui
                    .button(strings::DELETE)
                    .accessible_name(strings::DELETE)
                    .clicked()
            {
                out.delete = true;
            }
            if bar.selection {
                for (label, restack) in [
                    (strings::BRING_TO_FRONT, Restack::Front),
                    (strings::SEND_TO_BACK, Restack::Back),
                ] {
                    if ui.button(label).accessible_name(label).clicked() {
                        out.restack = Some(restack);
                    }
                }
            }
            if let Some(size) = bar.crop {
                ui.label(size.to_string())
                    .accessible_name(strings::CROP_SIZE);
                if ui
                    .button(strings::APPLY_BUTTON)
                    .accessible_name(strings::APPLY)
                    .clicked()
                {
                    out.apply_crop = true;
                }
                if ui
                    .button(strings::CANCEL_BUTTON)
                    .accessible_name(strings::CANCEL)
                    .clicked()
                {
                    out.cancel_crop = true;
                }
            } else if bar.properties.is_empty() && !bar.selection {
                ui.weak(bar.hint);
            }
        });
    });
    out
}

fn control(ui: &mut egui::Ui, kind: PropertyKind, bar: &PropertyBar<'_>, out: &mut Output) {
    let style = bar.style;
    match kind {
        PropertyKind::LineColour => {
            ui.label(strings::CAPTION_LINE);
            if let Some(c) = colour(ui, style.line, strings::LINE_COLOUR, kind, out) {
                out.changes.push(Property::LineColour(c));
            }
        }
        PropertyKind::FillColour => {
            ui.label(strings::CAPTION_FILL);
            let shown = style.fill.unwrap_or(Rgba::WHITE);
            if let Some(c) = colour(ui, shown, strings::FILL_COLOUR, kind, out) {
                out.changes.push(Property::FillColour(Some(c)));
            }
            let none = style.fill.is_none();
            if ui
                .add(egui::Button::selectable(none, strings::NONE))
                .on_hover_text(strings::NO_FILL)
                .accessible_name(strings::NO_FILL)
                .clicked()
            {
                out.changes
                    .push(Property::FillColour(if none { Some(shown) } else { None }));
            }
        }
        PropertyKind::BorderColour => {
            ui.label(strings::CAPTION_BORDER);
            let shown = style.border.unwrap_or(Rgba::BLACK);
            if let Some(c) = colour(ui, shown, strings::BORDER_COLOUR, kind, out) {
                out.changes.push(Property::BorderColour(Some(c)));
            }
            let none = style.border.is_none();
            if ui
                .add(egui::Button::selectable(none, strings::NONE))
                .on_hover_text(strings::NO_BORDER)
                .accessible_name(strings::NO_BORDER)
                .clicked()
            {
                out.changes.push(Property::BorderColour(if none {
                    Some(shown)
                } else {
                    None
                }));
            }
        }
        PropertyKind::Thickness => {
            ui.label(strings::THICKNESS);
            let mut value = style.thickness;
            let range = lecoq_core::style::THICKNESS_RANGE;
            let response = ui
                .add(egui::DragValue::new(&mut value).range(range).speed(0.1))
                .accessible_name(strings::THICKNESS);
            if response.dragged() || response.has_focus() {
                out.active = Some(kind);
            }
            if response.changed() && value != style.thickness {
                out.changes.push(Property::Thickness(value));
            }
        }
        PropertyKind::CounterSize => {
            ui.label(strings::CAPTION_SIZE);
            let mut value = style.counter_size;
            let range = lecoq_core::style::COUNTER_SIZE_RANGE;
            let response = ui
                .add(egui::DragValue::new(&mut value).range(range).speed(0.5))
                .accessible_name(strings::COUNTER_SIZE);
            if response.dragged() || response.has_focus() {
                out.active = Some(kind);
            }
            if response.changed() && value != style.counter_size {
                out.changes.push(Property::CounterSize(value));
            }
        }
        PropertyKind::Shadow => {
            let mut on = style.shadow;
            if ui
                .checkbox(&mut on, strings::SHADOW)
                .accessible_name(strings::SHADOW)
                .changed()
            {
                out.changes.push(Property::Shadow(on));
            }
        }
        PropertyKind::TextColour => {
            ui.label(strings::CAPTION_TEXT);
            if let Some(c) = colour(ui, style.line, strings::TEXT_COLOUR, kind, out) {
                out.changes.push(Property::TextColour(c));
            }
        }
        PropertyKind::Font => {
            let mut chosen = None;
            let current: &str = &style.font;
            let response = egui::ComboBox::from_id_salt("font")
                .selected_text(current)
                .width(160.0)
                .height(400.0)
                .show_ui(ui, |ui| {
                    for family in bar.fonts {
                        if ui
                            .selectable_label(family.as_str() == current, family.as_str())
                            .clicked()
                        {
                            chosen = Some(family.as_str());
                        }
                    }
                })
                .response;
            response.accessible_name(strings::FONT);
            if let Some(family) = chosen.filter(|f| *f != current) {
                out.changes
                    .push(Property::Font(std::sync::Arc::from(family)));
            }
        }
        PropertyKind::FontSize => {
            ui.label(strings::CAPTION_SIZE);
            let mut value = style.font_size;
            let range = lecoq_core::style::FONT_SIZE_RANGE;
            let response = ui
                .add(egui::DragValue::new(&mut value).range(range).speed(0.25))
                .accessible_name(strings::FONT_SIZE);
            if response.dragged() || response.has_focus() {
                out.active = Some(kind);
            }
            if response.changed() && value != style.font_size {
                out.changes.push(Property::FontSize(value));
            }
        }
        PropertyKind::Arrowheads => {
            ui.label(strings::ARROWHEADS);
            let mut value = style.arrowheads;
            let _ = egui::ComboBox::from_id_salt(strings::ARROWHEADS)
                .selected_text(strings::arrowheads(value))
                .show_ui(ui, |ui| {
                    for option in Arrowheads::ALL {
                        ui.selectable_value(&mut value, option, strings::arrowheads(option));
                    }
                })
                .response
                .accessible_name(strings::ARROWHEADS);
            if value != style.arrowheads {
                out.changes.push(Property::Arrowheads(value));
            }
        }
        PropertyKind::HighlightMode => {
            ui.label(strings::CAPTION_MODE);
            for mode in HighlightMode::ALL {
                let on = style.highlight_mode == mode;
                if ui
                    .add(egui::Button::selectable(on, strings::highlight_mode(mode)))
                    .accessible_name(strings::highlight_mode_label(mode))
                    .clicked()
                    && !on
                {
                    out.changes.push(Property::HighlightMode(mode));
                }
            }
        }
        // §4.3: the colour applies to text mode only, the magnification to magnify mode only.
        PropertyKind::HighlightColour => {
            if style.highlight_mode == HighlightMode::Text {
                ui.label(strings::CAPTION_COLOUR);
                if let Some(c) = colour(ui, style.highlight, strings::HIGHLIGHT_COLOUR, kind, out) {
                    out.changes.push(Property::HighlightColour(c));
                }
            }
        }
        PropertyKind::Magnification => {
            if style.highlight_mode == HighlightMode::Magnify {
                ui.label(strings::MAGNIFICATION);
                let mut value = style.magnification;
                let range = lecoq_core::style::MAGNIFICATION_RANGE;
                let response = ui
                    .add(egui::DragValue::new(&mut value).range(range).speed(0.05))
                    .accessible_name(strings::MAGNIFICATION);
                if response.dragged() || response.has_focus() {
                    out.active = Some(kind);
                }
                if response.changed() && value != style.magnification {
                    out.changes.push(Property::Magnification(value));
                }
            }
        }
        // The mode buttons are labelled by the mode alone: "Pixelate", "Blur".
        PropertyKind::ObfuscateMode => {
            ui.label(strings::CAPTION_MODE);
            for mode in ObfuscateMode::ALL {
                let on = style.obfuscate_mode == mode;
                if ui
                    .add(egui::Button::selectable(on, strings::obfuscate_mode(mode)))
                    .accessible_name(strings::obfuscate_mode(mode))
                    .clicked()
                    && !on
                {
                    out.changes.push(Property::ObfuscateMode(mode));
                }
            }
        }
        // §4.2: the pixelate block size or the blur radius, in Image pixels.
        PropertyKind::Strength => {
            ui.label(strings::STRENGTH);
            let mut value = style.strength;
            let range = lecoq_core::style::STRENGTH_RANGE;
            let response = ui
                .add(
                    egui::DragValue::new(&mut value)
                        .range(range)
                        .speed(0.2)
                        .suffix(strings::PIXELS_SUFFIX),
                )
                .accessible_name(strings::STRENGTH);
            if response.dragged() || response.has_focus() {
                out.active = Some(kind);
            }
            if response.changed() && value != style.strength {
                out.changes.push(Property::Strength(value));
            }
        }
    }
}

/// A colour button with a picker. Returns the new colour when it changed.
fn colour(
    ui: &mut egui::Ui,
    c: Rgba,
    label: &str,
    kind: PropertyKind,
    out: &mut Output,
) -> Option<Rgba> {
    // The picker's popup id, as egui's colour button derives it, to tell whether it is open.
    let popup_id = ui.auto_id_with("popup");
    let mut srgba = [c.r, c.g, c.b, c.a];
    let response = ui
        .color_edit_button_srgba_unmultiplied(&mut srgba)
        .accessible_name(label);
    if egui::Popup::is_id_open(ui.ctx(), popup_id) {
        out.active = Some(kind);
    }
    let new = Rgba::new(srgba[0], srgba[1], srgba[2], srgba[3]);
    (response.changed() && new != c).then_some(new)
}
