//! The status bar (§2): file name, Sibling position, pixel size, zoom, animation state, message.
//!
//! Each field is a label with a fixed accessibility label (from [`crate::strings`]), so screen
//! readers and the headless tests find it by that name and read its text as the value.

use lecoq_codec::Size;

use crate::view::Zoom;

use crate::strings::{
    self, ANIMATION as LABEL_ANIMATION, FILE_NAME as LABEL_FILE_NAME, MESSAGE as LABEL_MESSAGE,
    PIXEL_SIZE as LABEL_PIXEL_SIZE, SIBLING_POSITION as LABEL_POSITION, ZOOM as LABEL_ZOOM,
};

/// The animation state of an animated Image (§3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationState {
    Playing,
    /// The first edit stopped it on frame 1 (§3.4).
    Frozen,
}

impl AnimationState {
    /// The spec's text (§2), which screen readers get.
    fn text(self) -> &'static str {
        match self {
            Self::Playing => strings::ANIMATED,
            Self::Frozen => strings::FROZEN,
        }
    }

    /// The text drawn beside the painted symbol: the default fonts have no ▶ or ⏸, and egui
    /// panics on a missing glyph.
    fn drawn_text(self) -> &'static str {
        match self {
            Self::Playing => strings::ANIMATED_DRAWN,
            Self::Frozen => strings::FROZEN_DRAWN,
        }
    }
}

/// What the status bar shows this frame. `None` fields are left out.
pub struct StatusBar<'a> {
    pub file_name: Option<&'a str>,
    /// Unsaved changes: ` •` follows the file name.
    pub modified: bool,
    /// `n / N`, or `? / ?` while the Siblings are being scanned.
    pub position: Option<String>,
    pub pixel_size: Option<Size>,
    pub zoom: Option<Zoom>,
    pub animation: Option<AnimationState>,
    /// The transient message slot ("Last image in folder", "View only: …", …).
    pub message: Option<&'a str>,
}

pub fn show(ui: &mut egui::Ui, bar: &StatusBar<'_>) {
    egui::Panel::bottom("status_bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            // The same height empty or not, so the canvas (and the display-size decode box taken
            // from it) doesn't change when the first Image appears.
            ui.set_min_height(ui.spacing().interact_size.y);
            ui.spacing_mut().item_spacing.x = 16.0;
            if let Some(name) = bar.file_name {
                let marker = if bar.modified {
                    strings::MODIFIED_MARKER
                } else {
                    ""
                };
                field(ui, LABEL_FILE_NAME, &format!("{name}{marker}"));
            }
            if let Some(position) = &bar.position {
                field(ui, LABEL_POSITION, position);
            }
            if let Some(size) = bar.pixel_size {
                field(ui, LABEL_PIXEL_SIZE, &size.to_string());
            }
            if let Some(zoom) = bar.zoom {
                field(ui, LABEL_ZOOM, &zoom.label());
            }
            if let Some(animation) = bar.animation {
                animation_field(ui, animation);
            }
            if let Some(message) = bar.message {
                field(ui, LABEL_MESSAGE, message);
            }
        });
    });
}

fn field(ui: &mut egui::Ui, label: &str, text: &str) {
    ui.label(text).accessible_name(label);
}

/// The animation state: a painted symbol, then text. Its accessible value is the spec's text.
fn animation_field(ui: &mut egui::Ui, state: AnimationState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let side = ui.text_style_height(&egui::TextStyle::Body) * 0.7;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
        let colour = ui.visuals().text_color();
        match state {
            AnimationState::Playing => {
                let points = vec![rect.left_top(), rect.right_center(), rect.left_bottom()];
                ui.painter().add(egui::Shape::convex_polygon(
                    points,
                    colour,
                    egui::Stroke::NONE,
                ));
            }
            AnimationState::Frozen => {
                // Two bars: pause.
                let bar = rect.width() / 3.0;
                for left in [rect.left(), rect.right() - bar] {
                    let bar = egui::Rect::from_min_size(
                        egui::pos2(left, rect.top()),
                        egui::vec2(bar, rect.height()),
                    );
                    ui.painter().rect_filled(bar, 0.0, colour);
                }
            }
        }
        let response = ui
            .label(state.drawn_text())
            .accessible_name(LABEL_ANIMATION);
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_value(state.text());
        });
    });
}
