//! Input → commands: the keyboard shortcuts (§6.1), Sibling navigation and zoom (§3.1) and files
//! dropped onto the window (§3.5).
//!
//! Each frame, [`commands`] turns this frame's input into [`Command`]s for
//! [`crate::LecoqApp::execute`]. Widgets don't act on keys or drops themselves.

use egui::{Event, Key, KeyboardShortcut, Modifiers, MouseWheelUnit};
use lecoq_core::Restack;

use crate::Command;
use crate::tools::ToolId;

// The §6.1 shortcuts. `Modifiers::COMMAND` is Ctrl, and Cmd on macOS. The menus show the same
// shortcuts beside their entries.
pub const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
pub const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
pub const SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
// The window is started with eframe's clipboard shortcuts off (`startup.rs`), so these arrive as
// plain key events rather than egui's Copy and Paste events.
pub const COPY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::C);
pub const PASTE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::V);
pub const NEW_FROM_CLIPBOARD: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::V);
pub const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
pub const REDO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
pub const REDO_Y: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Y);
pub const DUPLICATE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::D);
pub const RAISE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::CloseBracket);
pub const LOWER: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::OpenBracket);

/// The arrow keys move the keyboard focus (§8.6): → and ↓ to the next control, ← and ↑ to the
/// previous one, in Tab order. egui's own arrow navigation goes by geometry, which jumps from the
/// property bar up into the menu bar; lecoq's chrome is rows (menu bar, property bar) and a column
/// (tool strip), so Tab order is what the arrows mean. Only while a control has the focus and no
/// text is being typed (text fields keep their arrows for the caret).
pub fn arrows_move_focus(ctx: &egui::Context, typing: bool) {
    if typing || ctx.memory(|m| m.focused()).is_none() {
        return;
    }
    let direction = ctx.input(|input| {
        input.events.iter().rev().find_map(|event| match event {
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.any() => match key {
                Key::ArrowRight | Key::ArrowDown => Some(egui::FocusDirection::Next),
                Key::ArrowLeft | Key::ArrowUp => Some(egui::FocusDirection::Previous),
                _ => None,
            },
            _ => None,
        })
    });
    if let Some(direction) = direction {
        ctx.memory_mut(|m| m.move_focus(direction));
    }
}

/// Points of smooth (touchpad) scrolling that count as one wheel notch.
const POINTS_PER_STEP: f32 = 50.0;

/// The commands this frame's input asks for, in order. `canvas` is the canvas as of the last frame:
/// Ctrl+wheel only zooms while the pointer is over it. `typing` is true while text is being typed
/// (the text tool, a dialog's text field): it has the keyboard to itself, Ctrl+C/V/Z included.
/// `pannable` is whether the Image is zoomed beyond Fit (§3.1): a plain wheel pans then ([`crate::pan`])
/// instead of navigating Siblings. The arrow keys navigate in either state.
pub fn commands(
    ctx: &egui::Context,
    canvas: Option<egui::Rect>,
    typing: bool,
    pannable: bool,
) -> Vec<Command> {
    let mut commands = Vec::new();
    // A focused control (reached with Tab, §8.6) keeps the plain keys: the arrows move the focus,
    // Enter and Space press it, Esc lets go of it. Shortcuts with Ctrl (Cmd) still work.
    let plain_keys = !ctx.egui_wants_keyboard_input();
    ctx.input_mut(|input| {
        if input.consume_shortcut(&OPEN) {
            commands.push(Command::OpenDialog);
        }
        // Save As first: egui ignores an extra Shift when matching Ctrl+S.
        if input.consume_shortcut(&SAVE_AS) {
            commands.push(Command::SaveAs);
        }
        if input.consume_shortcut(&SAVE) {
            commands.push(Command::Save);
        }
        if !typing {
            command_keys(input, &mut commands);
        }
        // F11 means nothing to a focused control.
        if !typing && input.consume_key(Modifiers::NONE, Key::F11) {
            commands.push(Command::ToggleFullscreen);
        }
        if plain_keys {
            // `+` is Shift+= on many layouts; `=` alone also zooms in, as in most viewers.
            if input.consume_key(Modifiers::NONE, Key::Plus)
                || input.consume_key(Modifiers::NONE, Key::Equals)
            {
                commands.push(Command::ZoomIn);
            }
            if input.consume_key(Modifiers::NONE, Key::Minus) {
                commands.push(Command::ZoomOut);
            }
            if input.consume_key(Modifiers::NONE, Key::Num0) {
                commands.push(Command::ZoomFit);
            }
            // → / ← move through the Siblings.
            if input.consume_key(Modifiers::NONE, Key::ArrowRight) {
                commands.push(Command::NextSibling);
            }
            if input.consume_key(Modifiers::NONE, Key::ArrowLeft) {
                commands.push(Command::PreviousSibling);
            }
            editor_keys(input, &mut commands);
        }
        // Ctrl+wheel (egui's zoom modifier) and touchpad pinch.
        let factor = input.zoom_delta();
        if factor != 1.0
            && let (Some(around), Some(canvas)) = (input.pointer.hover_pos(), canvas)
            && canvas.contains(around)
        {
            commands.push(Command::ZoomAround { factor, around });
        }
        // Several files dropped at once: only the first is opened, as the new Image.
        let dropped = input.raw.dropped_files.first().map(|f| f.path().to_owned());
        if let Some(path) = dropped {
            // Ctrl+drop inserts the file as an Inset instead (§3.5, §4.6).
            if input.modifiers.command {
                commands.push(Command::InsertFile(path));
            } else {
                commands.push(Command::Open(path));
            }
        }
    });
    wheel(ctx, pannable, &mut commands);
    commands
}

/// The Ctrl (Cmd) shortcuts of §6.1 other than open and save: the clipboard, History (§4.8),
/// duplicate and stacking (§4.2). Match the most specific modifiers first: egui ignores an extra
/// Shift when matching.
fn command_keys(input: &mut egui::InputState, commands: &mut Vec<Command>) {
    if input.consume_shortcut(&NEW_FROM_CLIPBOARD) {
        commands.push(Command::NewFromClipboard);
    }
    if input.consume_shortcut(&PASTE) {
        commands.push(Command::Paste);
    }
    if input.consume_shortcut(&COPY) {
        commands.push(Command::Copy);
    }
    if input.consume_shortcut(&REDO) || input.consume_shortcut(&REDO_Y) {
        commands.push(Command::Redo);
    }
    if input.consume_shortcut(&UNDO) {
        commands.push(Command::Undo);
    }
    if input.consume_shortcut(&DUPLICATE) {
        commands.push(Command::Duplicate);
    }
    if input.consume_shortcut(&RAISE) {
        commands.push(Command::Restack(Restack::Up));
    }
    if input.consume_shortcut(&LOWER) {
        commands.push(Command::Restack(Restack::Down));
    }
}

/// The editor's plain keys: Delete (§4.2), Enter for crop and the tool keys (§4.1).
fn editor_keys(input: &mut egui::InputState, commands: &mut Vec<Command>) {
    if input.consume_key(Modifiers::NONE, Key::Delete) {
        commands.push(Command::Delete);
    }
    if input.consume_key(Modifiers::NONE, Key::Enter) {
        commands.push(Command::ApplyCrop);
    }
    for tool in ToolId::all() {
        if let Some(key) = tool.key()
            && input.consume_key(Modifiers::NONE, key)
        {
            commands.push(Command::SelectTool(tool));
        }
    }
}

/// §3.1: wheel down is the next Sibling, wheel up the previous one. Ctrl+wheel zooms instead
/// (above) and Shift+wheel pans ([`crate::pan`], #43). When the Image is `pannable` (zoomed beyond
/// Fit), a plain wheel pans instead ([`crate::pan`], #50), so it navigates no Sibling here.
fn wheel(ctx: &egui::Context, pannable: bool, commands: &mut Vec<Command>) {
    if pannable {
        return;
    }
    let id = egui::Id::unique("lecoq_wheel_points");
    let mut points: f32 = ctx.data(|d| d.get_temp(id)).unwrap_or(0.0);
    ctx.input(|input| {
        for event in &input.events {
            let Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if modifiers.ctrl || modifiers.command || modifiers.shift || delta.y == 0.0 {
                continue;
            }
            // A positive y moves the content down: the wheel turned up.
            let steps = match unit {
                MouseWheelUnit::Line | MouseWheelUnit::Page => -delta.y.signum() as i32,
                MouseWheelUnit::Point => {
                    points += delta.y;
                    let steps = -(points / POINTS_PER_STEP).trunc() as i32;
                    points %= POINTS_PER_STEP;
                    steps
                }
            };
            let command = if steps > 0 {
                Command::NextSibling
            } else {
                Command::PreviousSibling
            };
            commands.extend(std::iter::repeat_n(command, steps.unsigned_abs() as usize));
        }
    });
    ctx.data_mut(|d| d.insert_temp(id, points));
}
