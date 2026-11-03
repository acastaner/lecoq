//! Panning a zoomed Image (#43): Space+left-drag, middle-drag and Shift+wheel, plus a plain scroll
//! while the Image is zoomed beyond Fit (#50).
//!
//! Self-contained so it composes with the editor tools: a left-drag on the canvas belongs to the
//! active tool unless Space is held, and middle-drag is always a pan. Tools ask
//! [`PanGesture::owns_pointer`] before acting on the pointer. A plain wheel pans when the Image is
//! pannable (zoomed beyond Fit, the same condition the grab cursor uses); at Fit it is left alone
//! for Sibling navigation (§3.1, [`crate::input`]). Ctrl+wheel is zoom ([`crate::input`]).

use egui::{Key, PointerButton};

use crate::Command;

/// The drag in progress, if any. Lives across frames, so it is held by the app.
#[derive(Default)]
pub struct PanGesture {
    /// The button of the pan drag in progress.
    dragging: Option<PointerButton>,
}

impl PanGesture {
    /// The pan this frame's input asks for. `canvas` is the canvas as of the last frame: a pan drag
    /// only starts on it, and the wheel only pans over it. `pannable` is whether the Image is
    /// zoomed beyond Fit (the same notion the grab cursor uses, §3.1): only then does a plain scroll
    /// pan rather than navigate Siblings (#50).
    pub fn commands(
        &mut self,
        ctx: &egui::Context,
        canvas: Option<egui::Rect>,
        pannable: bool,
    ) -> Vec<Command> {
        let space = space_held(ctx);
        let line = ctx.options(|o| o.input_options.line_scroll_speed);
        let mut commands = Vec::new();
        ctx.input(|input| {
            let pointer = &input.pointer;
            if let Some(button) = self.dragging {
                if !pointer.button_down(button) {
                    self.dragging = None;
                }
            } else if let (Some(origin), Some(canvas)) = (pointer.press_origin(), canvas)
                && canvas.contains(origin)
            {
                if pointer.button_pressed(PointerButton::Middle) {
                    self.dragging = Some(PointerButton::Middle);
                } else if space && pointer.button_pressed(PointerButton::Primary) {
                    self.dragging = Some(PointerButton::Primary);
                }
            }
            if self.dragging.is_some() && pointer.delta() != egui::Vec2::ZERO {
                commands.push(Command::Pan(pointer.delta()));
            }

            // Wheel panning. Raw events, so the step isn't smoothed over frames. Shift+wheel always
            // pans horizontally; a plain wheel pans on both axes only when the Image is pannable,
            // otherwise it is left for Sibling navigation ([`crate::input`]).
            let over_canvas = matches!(
                (input.pointer.hover_pos(), canvas),
                (Some(at), Some(canvas)) if canvas.contains(at)
            );
            if !over_canvas {
                return;
            }
            for event in &input.raw.events {
                let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = event
                else {
                    continue;
                };
                let step = match unit {
                    egui::MouseWheelUnit::Point => 1.0,
                    egui::MouseWheelUnit::Line => line,
                    egui::MouseWheelUnit::Page => canvas.map_or(0.0, |c| c.width()),
                };
                if modifiers.shift_only() {
                    // Some platforms already turn Shift+wheel into a horizontal delta.
                    let dx = (delta.x + delta.y) * step;
                    if dx != 0.0 {
                        commands.push(Command::Pan(egui::vec2(dx, 0.0)));
                    }
                } else if pannable && !modifiers.any() {
                    // A plain scroll pans the zoomed Image on both axes. The raw delta already
                    // carries the platform's natural-scroll direction, so the content tracks the
                    // gesture; out-of-bounds pans are clamped by the view.
                    let pan = *delta * step;
                    if pan != egui::Vec2::ZERO {
                        commands.push(Command::Pan(pan));
                    }
                }
            }
        });
        commands
    }

    /// Does the pointer belong to panning (a pan drag in progress, or Space held so a left-drag
    /// would start one)? The active tool leaves the pointer alone while it does.
    pub fn owns_pointer(&self, ctx: &egui::Context) -> bool {
        self.dragging.is_some() || space_held(ctx)
    }

    /// Is a pan drag actually under way (Space+left or middle button down)? The cursor shows the
    /// closed grabbing hand while it is (#48).
    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }
}

/// Space is the pan modifier, except while a text field (the text tool, a dialog) has the keyboard.
fn space_held(ctx: &egui::Context) -> bool {
    !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_down(Key::Space))
}
