//! Every string the UI shows or announces (§8.6): one table, English only.
//!
//! Widgets, messages and dialogs take their text from here; the only user-facing text elsewhere is
//! each tool's name and hint, which sit with the rest of its data in the `TOOLS` table
//! (`crate::tools`) so a new tool is one row. A fixed string is a `const`; text with a value in it
//! (a file name, a reason, a shortcut formatted for the platform) is a function. Accessibility
//! labels are the names screen readers announce and the headless tests find controls by, so
//! changing one is a visible change.
//!
//! Not here: error details from libraries (codecs, the OS) that messages pass through as the
//! `reason`, and the `config.toml` keys.

use lecoq_core::{Arrowheads, HighlightMode, ObfuscateMode};

use crate::tools::ToolId;

// ---- Window ----

pub const APP_NAME: &str = "lecoq";

/// The window title: `<file> - lecoq`, or `lecoq` alone.
pub fn window_title(file_name: Option<&str>) -> String {
    match file_name {
        Some(name) => format!("{name} - {APP_NAME}"),
        None => APP_NAME.to_owned(),
    }
}

/// The canvas text when there is no Image (§2): "Open (Ctrl+O), drop or paste (Ctrl+V) an image".
/// The shortcuts come formatted for the platform (Cmd on macOS).
pub fn empty_window_hint(open: &str, paste: &str) -> String {
    format!("Open ({open}), drop or paste ({paste}) an image")
}

/// The canvas's accessibility label.
pub const CANVAS: &str = "Image canvas";

// ---- Menu bar (§2) ----

pub const MENU_FILE: &str = "File";
pub const MENU_EDIT: &str = "Edit";
pub const MENU_IMAGE: &str = "Image";
pub const MENU_VIEW: &str = "View";
pub const MENU_HELP: &str = "Help";

// File
pub const OPEN: &str = "Open…";
pub const SAVE: &str = "Save";
pub const SAVE_AS: &str = "Save As…";
pub const NEW_FROM_CLIPBOARD: &str = "New from clipboard";
pub const COPY_TO: &str = "Copy to…";
pub const MOVE_TO: &str = "Move to…";
pub const MOVE_TO_TRASH: &str = "Move to trash";

// Edit
pub const UNDO: &str = "Undo";
pub const REDO: &str = "Redo";
pub const COPY: &str = "Copy";
pub const PASTE: &str = "Paste";
pub const DUPLICATE: &str = "Duplicate";
pub const DELETE: &str = "Delete";
pub const BRING_FORWARD: &str = "Bring forward";
pub const SEND_BACKWARD: &str = "Send backward";
pub const BRING_TO_FRONT: &str = "Bring to front";
pub const SEND_TO_BACK: &str = "Send to back";

// Image (§4.7)
pub const ROTATE_LEFT: &str = "Rotate left";
pub const ROTATE_RIGHT: &str = "Rotate right";
pub const FLIP_HORIZONTAL: &str = "Flip horizontal";
pub const FLIP_VERTICAL: &str = "Flip vertical";
pub const RESIZE_MENU: &str = "Resize…";

// View
pub const ZOOM_IN: &str = "Zoom in";
pub const ZOOM_OUT: &str = "Zoom out";
pub const FIT_TO_WINDOW: &str = "Fit to window";
pub const FULLSCREEN: &str = "Fullscreen";
pub const NEXT_IMAGE: &str = "Next image";
pub const PREVIOUS_IMAGE: &str = "Previous image";
pub const ALLOW_MULTIPLE_INSTANCES: &str = "Allow multiple instances";
pub const FOLLOW_FILES: &str = "Follow files after copy/move";

// Help
pub const KEYBOARD_SHORTCUTS: &str = "Keyboard shortcuts";
pub const ABOUT: &str = "About lecoq";

// Quick actions (§2): accessibility labels, also their tooltips.
pub const QUICK_SAVE: &str = "Save";
pub const QUICK_SAVE_AS: &str = "Save as";
pub const QUICK_COPY: &str = "Copy";
pub const QUICK_UNDO: &str = "Undo";
pub const QUICK_IMAGE: &str = "Rotate, flip or resize";
pub const QUICK_TRASH: &str = "Move to trash";

/// A quick action's tooltip: its label and, when it has one, its shortcut.
pub fn with_shortcut(label: &str, shortcut: &str) -> String {
    if shortcut.is_empty() {
        label.to_owned()
    } else {
        format!("{label} ({shortcut})")
    }
}

// ---- Tool strip (§4.1) ----

// Tool names and hints are reached through `ToolId::name` / `ToolId::hint`, backed by the `TOOLS`
// table (`crate::tools`); see this module's header.

// Key names beside menu entries and in tooltips (written out: egui's arrow symbols aren't in the
// fonts loaded at startup).
pub const KEY_ESC: &str = "Esc";
pub const KEY_DELETE: &str = "Delete";
pub const KEY_F11: &str = "F11";
pub const KEY_PLUS: &str = "+";
pub const KEY_MINUS: &str = "-";
pub const KEY_ZERO: &str = "0";
pub const KEY_LEFT: &str = "Left";
pub const KEY_RIGHT: &str = "Right";
pub const KEY_HOME: &str = "Home";
pub const KEY_END: &str = "End";
pub const KEY_ENTER: &str = "Enter";

// ---- Property bar (§4.3) ----

pub const LINE_COLOUR: &str = "Line colour";
pub const FILL_COLOUR: &str = "Fill colour";
pub const NO_FILL: &str = "No fill";
pub const BORDER_COLOUR: &str = "Border colour";
pub const NO_BORDER: &str = "No border";
pub const THICKNESS: &str = "Thickness";
pub const SHADOW: &str = "Shadow";
pub const COUNTER_SIZE: &str = "Size";
pub const ARROWHEADS: &str = "Arrowheads";
pub const TEXT_COLOUR: &str = "Text colour";
pub const FONT: &str = "Font";
pub const FONT_SIZE: &str = "Size";
pub const HIGHLIGHT_COLOUR: &str = "Highlight colour";
pub const MAGNIFICATION: &str = "Magnification";
pub const STRENGTH: &str = "Strength";
pub const CROP_SIZE: &str = "Crop size";
pub const APPLY: &str = "Apply";
pub const CANCEL: &str = "Cancel";

// The short captions drawn beside the controls (the controls carry the full labels above).
pub const CAPTION_LINE: &str = "Line";
pub const CAPTION_FILL: &str = "Fill";
pub const CAPTION_BORDER: &str = "Border";
pub const CAPTION_TEXT: &str = "Text";
pub const CAPTION_MODE: &str = "Mode";
pub const CAPTION_COLOUR: &str = "Colour";
pub const CAPTION_SIZE: &str = "Size";
pub const NONE: &str = "None";
pub const APPLY_BUTTON: &str = "Apply (Enter)";
pub const CANCEL_BUTTON: &str = "Cancel (Esc)";
pub const PIXELS_SUFFIX: &str = " px";

pub fn highlight_mode(mode: HighlightMode) -> &'static str {
    match mode {
        HighlightMode::Text => "Text",
        HighlightMode::Area => "Area",
        HighlightMode::Grayscale => "Grayscale",
        HighlightMode::Magnify => "Magnify",
    }
}

/// A highlight mode button's accessibility label: "Text highlight", "Magnify highlight", … ("Text"
/// alone is the text tool's button).
pub fn highlight_mode_label(mode: HighlightMode) -> String {
    format!("{} highlight", highlight_mode(mode))
}

/// An obfuscate mode: its button's text and accessibility label.
pub fn obfuscate_mode(mode: ObfuscateMode) -> &'static str {
    match mode {
        ObfuscateMode::Pixelate => "Pixelate",
        ObfuscateMode::Blur => "Blur",
    }
}

pub fn arrowheads(a: Arrowheads) -> &'static str {
    match a {
        Arrowheads::End => "End",
        Arrowheads::Start => "Start",
        Arrowheads::Both => "Both",
        Arrowheads::None => "None",
    }
}

// ---- Status bar (§2) ----

pub const FILE_NAME: &str = "File name";
pub const SIBLING_POSITION: &str = "Sibling position";
pub const PIXEL_SIZE: &str = "Pixel size";
pub const ZOOM: &str = "Zoom";
pub const ANIMATION: &str = "Animation";
pub const MESSAGE: &str = "Message";

/// The zoom read-out at fit-to-window.
pub const ZOOM_FIT: &str = "Fit";

/// The zoom read-out as a percentage.
pub fn zoom_percent(scale: f32) -> String {
    format!("{:.0}%", scale * 100.0)
}

/// The marker after the file name for unsaved changes.
pub const MODIFIED_MARKER: &str = " •";

/// The animation state as screen readers get it (§2).
pub const ANIMATED: &str = "▶ animated";
pub const FROZEN: &str = "⏸ frozen at frame 1";
/// The same, drawn beside a painted symbol: the default fonts have no ▶ or ⏸.
pub const ANIMATED_DRAWN: &str = "animated";
pub const FROZEN_DRAWN: &str = "frozen at frame 1";

// ---- Messages (the status bar's message slot, §2) ----

pub const FIRST_IMAGE: &str = "First image in folder";
pub const LAST_IMAGE: &str = "Last image in folder";
pub const MOVED_TO_TRASH: &str = "Moved to trash";

/// "Copied to <folder>" / "Moved to <folder>" after a Copy to / Move to (§2, stories 25–26). The
/// folder is the Target folder's last path component.
pub fn copied_to(folder: &str) -> String {
    format!("Copied to {folder}")
}

pub fn moved_to(folder: &str) -> String {
    format!("Moved to {folder}")
}

/// A name already taken in the Target folder (stories 27–28): nothing is done. Shown in the Copy
/// to / Move to warning modal (ADR 0009).
pub fn already_exists(name: &str, folder: &str) -> String {
    format!("{name} already exists in {folder}")
}

/// A Move into the folder the file is already in (story 29): harmless, and worded neutrally — the
/// file is already where it was asked to go (ADR 0009).
pub fn already_in_folder(name: &str, folder: &str) -> String {
    format!("{name} is already in {folder}")
}

/// A cross-device Move whose copy landed but whose original couldn't be removed (story 31): worded
/// honestly, since the file *is* in the Target folder (ADR 0009).
pub fn moved_copy_couldnt_remove(name: &str, folder: &str, reason: &str) -> String {
    format!("Moved a copy of {name} to {folder}, but couldn't remove the original: {reason}")
}

pub fn cant_copy_file(name: &str, reason: &str) -> String {
    format!("Couldn't copy {name}: {reason}")
}

pub fn cant_move_file(name: &str, reason: &str) -> String {
    format!("Couldn't move {name}: {reason}")
}
pub const IMAGE_COPIED: &str = "Image copied to the clipboard";
pub const ANNOTATIONS_COPIED: &str = "Annotations copied to the clipboard";
pub const NOTHING_TO_PASTE: &str = "Nothing to paste";
pub const NO_CLIPBOARD_IMAGE: &str = "No image on the clipboard";
pub const ANIMATION_FROZEN: &str = "Animation frozen at frame 1 for editing";
pub const VIEW_ONLY: &str = "View only: image too large to edit";

pub fn cant_open(name: &str, reason: &str) -> String {
    format!("Can't open {name}: {reason}")
}

pub fn cant_save(name: &str, reason: &str) -> String {
    format!("Can't save {name}: {reason}")
}

/// The `<reason>` of [`cant_save`]: a library's own reason as is, lecoq's wording around a code.
pub fn save_reason(error: &lecoq_codec::SaveError) -> String {
    match error {
        lecoq_codec::SaveError::Library(reason) => reason.clone(),
        lecoq_codec::SaveError::WebPEncoding(code) => format!("WebP encoding failed ({code})"),
    }
}

pub fn cant_copy_image(reason: &str) -> String {
    format!("Can't copy the image: {reason}")
}

pub fn cant_copy_annotations(reason: &str) -> String {
    format!("Can't copy the Annotations: {reason}")
}

pub fn cant_paste(reason: &str) -> String {
    format!("Can't paste: {reason}")
}

pub fn cant_open_window(reason: &str) -> String {
    format!("Can't open a new window: {reason}")
}

pub fn cant_trash(name: &str, reason: &str) -> String {
    format!("Can't move {name} to trash: {reason}")
}

// Reasons lecoq itself gives.
pub const NOTHING_SELECTED: &str = "nothing selected";
pub const NO_PIXELS_TO_SAVE: &str = "no pixels to save";
pub const FILE_CHANGED_OR_GONE: &str = "the file changed or went away on disk";
pub const FILE_CHANGED: &str = "the file changed on disk";

/// The name of an Image pasted into the empty window, until it is saved (§3.5).
pub const CLIPBOARD_IMAGE: &str = "Clipboard image";

// ---- Dialogs (§6.4) ----

// The native file dialogs (§8.2).
pub const OPEN_DIALOG_TITLE: &str = "Open";
/// The open dialog's filter for the formats lecoq reads.
pub const IMAGES_FILTER: &str = "Images";
/// The open dialog's filter that shows every file.
pub const ALL_FILES_FILTER: &str = "All files";
pub const SAVE_AS_DIALOG_TITLE: &str = "Save As";
/// The folder picker's title, telling the user which File operation they are choosing a folder for
/// (story 6).
pub const COPY_TO_DIALOG_TITLE: &str = "Copy to folder";
pub const MOVE_TO_DIALOG_TITLE: &str = "Move to folder";

pub const SAVE_CHANGES_TITLE: &str = "Save changes?";

pub fn unsaved_changes(name: &str) -> String {
    format!("{name} has unsaved changes.")
}

pub const DISCARD: &str = "Discard";

pub const OVERWRITE_TITLE: &str = "Overwrite animated image?";

pub fn overwrite_animation(name: &str) -> String {
    format!(
        "Saving will replace the animated file {name} with a still image. \
         All frames except the first will be lost."
    )
}

pub const OVERWRITE: &str = "Overwrite";
pub const SAVE_AS_BUTTON: &str = "Save As…";

/// The Copy to / Move to warning modal's title (ADR 0009): a dismiss-only notice that the File
/// operation did not place the file as asked. The body carries the outcome-specific wording.
pub const FILE_OP_WARNING_TITLE: &str = "File operation";

pub const RESIZE_TITLE: &str = "Resize";
pub const RESIZE: &str = "Resize";
pub const WIDTH: &str = "Width";
pub const HEIGHT: &str = "Height";
pub const UNIT_PIXELS: &str = "Pixels";
pub const UNIT_PERCENT: &str = "Percent";
pub const UNIT_PIXELS_SHORT: &str = "px";
pub const UNIT_PERCENT_SHORT: &str = "%";
pub const LOCK_ASPECT_RATIO: &str = "Lock aspect ratio";
/// The read-out of the size the Image will have (`160×120`).
pub const NEW_SIZE: &str = "New size";
pub const NEW_SIZE_CAPTION: &str = "New size:";
pub const NO_SIZE: &str = "–";

pub const CLOSE: &str = "Close";

/// Help › About: the name, version, the package description and the licence (ADR 0004).
pub fn about_text(version: &str) -> String {
    format!(
        "{APP_NAME} {version}\n{}\nLicensed under the GNU GPL, version 3 or later.",
        env!("CARGO_PKG_DESCRIPTION")
    )
}

// The Keyboard shortcuts dialog: what each shortcut does (§3.1, §4.1, §6.1).
pub const DOES_SAVE: &str = "Save in place";
pub const DOES_SAVE_AS: &str = "Save As";
pub const DOES_OPEN: &str = "Open";
pub const DOES_COPY: &str = "Copy";
pub const DOES_PASTE: &str = "Paste";
pub const DOES_NEW_FROM_CLIPBOARD: &str = "Open the clipboard image in a new window";
pub const DOES_COPY_TO: &str = "Copy the file to a folder";
pub const DOES_MOVE_TO: &str = "Move the file to a folder";
pub const DOES_DUPLICATE: &str = "Duplicate the selected Annotations";
pub const DOES_UNDO: &str = "Undo";
pub const DOES_REDO: &str = "Redo";
pub const DOES_RAISE: &str = "Move the selection up one level";
pub const DOES_LOWER: &str = "Move the selection down one level";
pub const DOES_DELETE: &str = "Delete the selected Annotations, or trash the file";
pub const DOES_FULLSCREEN: &str = "Fullscreen";
pub const DOES_NEXT: &str = "Next image";
pub const DOES_PREVIOUS: &str = "Previous image";
pub const DOES_FIRST: &str = "First image";
pub const DOES_LAST: &str = "Last image";
pub const DOES_ZOOM_IN: &str = "Zoom in";
pub const DOES_ZOOM_OUT: &str = "Zoom out";
pub const DOES_FIT: &str = "Fit to window";
pub const DOES_APPLY_CROP: &str = "Apply the crop";
pub const DOES_MENUS: &str = "Move through menus and controls";
pub const DOES_ACTIVATE: &str = "Activate the focused control";

/// "Select tool", "Rectangle tool", … in the shortcuts list.
pub fn tool_shortcut(tool: ToolId) -> String {
    format!("{} tool", tool.name())
}

pub const KEYS_TAB: &str = "Tab, arrow keys";
pub const KEYS_ENTER: &str = "Enter, Space";
