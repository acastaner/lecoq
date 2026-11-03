//! The tool strip (§2): a vertical strip on the left with every tool in §4.1 order.
//!
//! Each button's accessibility label is the tool's name ("Rectangle"); its tooltip adds the key.
//! Tools that aren't built yet, and every tool on a view-only Image, are disabled.
//!
//! The icons are painted, not glyphs: egui's built-in fonts lack most of these symbols, and UI
//! fonts aren't loaded before the first paint (§1.3).

use egui::{Color32, Pos2, Rect, Stroke, pos2, vec2};

use crate::Command;
use crate::strings;
use crate::tools::ToolId;

const BUTTON: f32 = 30.0;

pub fn show(ui: &mut egui::Ui, active: ToolId, can_edit: bool) -> Option<Command> {
    let mut command = None;
    egui::Panel::left("tool_strip")
        .resizable(false)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                for tool in ToolId::all() {
                    let enabled = tool.is_available() && (can_edit || tool == ToolId::Select);
                    let button =
                        egui::Button::selectable(tool == active, "").min_size(vec2(BUTTON, BUTTON));
                    let name = tool.name();
                    let tooltip = strings::with_shortcut(name, key_label(tool).unwrap_or(""));
                    let response = ui
                        .add_enabled(enabled, button)
                        .on_hover_text(&tooltip)
                        .on_disabled_hover_text(&tooltip)
                        .accessible_name(name);
                    let colour = if enabled {
                        ui.visuals().widgets.inactive.fg_stroke.color
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    paint_icon(ui.painter(), response.rect.shrink(8.0), tool, colour);
                    if response.clicked() {
                        command = Some(Command::SelectTool(tool));
                    }
                }
            });
        });
    command
}

/// The tool's key as shown in tooltips and the shortcuts list (§4.1).
pub fn key_label(tool: ToolId) -> Option<&'static str> {
    match tool.key()? {
        egui::Key::Escape => Some(strings::KEY_ESC),
        key => Some(key.name()),
    }
}

fn paint_icon(p: &egui::Painter, r: Rect, tool: ToolId, c: Color32) {
    let s = Stroke::new(1.5, c);
    let at = |u: f32, v: f32| -> Pos2 { pos2(r.min.x + u * r.width(), r.min.y + v * r.height()) };
    match tool {
        ToolId::Select => {
            let corners = [at(0., 0.), at(1., 0.), at(1., 1.), at(0., 1.), at(0., 0.)];
            p.extend(egui::Shape::dashed_line(&corners, s, 3.0, 2.0));
        }
        ToolId::Rectangle => {
            p.rect_stroke(r.shrink2(vec2(0.0, 2.0)), 0.0, s, egui::StrokeKind::Middle);
        }
        ToolId::Ellipse => {
            p.add(egui::Shape::ellipse_stroke(r.center(), r.size() / 2.0, s));
        }
        ToolId::Line => {
            p.line_segment([at(0., 1.), at(1., 0.)], s);
        }
        ToolId::Arrow => {
            p.line_segment([at(0., 1.), at(1., 0.)], s);
            p.line_segment([at(1., 0.), at(0.45, 0.05)], s);
            p.line_segment([at(1., 0.), at(0.95, 0.55)], s);
        }
        ToolId::Freehand => {
            let points = (0..=12)
                .map(|i| {
                    let u = i as f32 / 12.0;
                    at(u, 0.5 - 0.35 * (u * std::f32::consts::TAU).sin())
                })
                .collect();
            p.add(egui::Shape::line(points, s));
        }
        ToolId::Text => {
            p.line_segment([at(0., 0.), at(1., 0.)], s);
            p.line_segment([at(0.5, 0.), at(0.5, 1.)], s);
        }
        ToolId::StepCounter => {
            p.circle_stroke(r.center(), r.width() / 2.0, s);
            p.line_segment([at(0.5, 0.25), at(0.5, 0.75)], s);
            p.line_segment([at(0.5, 0.25), at(0.38, 0.37)], s);
        }
        ToolId::Highlight => {
            p.rect_filled(r.shrink2(vec2(0.0, 3.0)), 0.0, c.gamma_multiply(0.5));
        }
        ToolId::Obfuscate => {
            let n = 3;
            let cell = r.width() / n as f32;
            for i in 0..n {
                for j in 0..n {
                    if (i + j) % 2 == 0 {
                        let min = pos2(r.min.x + i as f32 * cell, r.min.y + j as f32 * cell);
                        p.rect_filled(Rect::from_min_size(min, vec2(cell, cell)), 0.0, c);
                    }
                }
            }
        }
        ToolId::Crop => {
            p.line_segment([at(0.25, 0.), at(0.25, 0.75)], s);
            p.line_segment([at(0.25, 0.75), at(1., 0.75)], s);
            p.line_segment([at(0., 0.25), at(0.75, 0.25)], s);
            p.line_segment([at(0.75, 0.25), at(0.75, 1.)], s);
        }
        // Not in the strip.
        ToolId::Inset => {}
    }
}
