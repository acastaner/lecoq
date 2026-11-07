//! Commands: what user input (keys, menus, drops, the launch argument) turns into.
//!
//! Input handling maps events to a [`Command`]; [`crate::LecoqApp::execute`] carries it out. Tests
//! send the same commands, or raw input through the harness. Add a variant per new action.

use std::path::PathBuf;

use lecoq_core::{Property, Restack};

use crate::tools::ToolId;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Open this file as the Image (§3.5).
    Open(PathBuf),
    /// Make this the active tool (§4.1). Ignored for unavailable tools and view-only Images.
    SelectTool(ToolId),
    /// Ctrl+Z (§4.8).
    Undo,
    /// Ctrl+Shift+Z or Ctrl+Y.
    Redo,
    /// Delete (§3.1, §4.2): remove the selected Annotations; with nothing selected, move the file
    /// to the system trash and show the next Sibling (#22).
    Delete,
    /// A property-bar change (§4.3): restyles the selection, or sets the active tool's default
    /// when nothing is selected. `merge` folds it into the previous step when that changed the same
    /// property on the same Annotations (one widget interaction is one step).
    SetProperty { property: Property, merge: bool },
    /// Enter or Apply with the crop tool: crop the Image to the region (§4.7).
    ApplyCrop,
    /// Cancel with the crop tool: drop the region and go back to the select tool (Esc does the
    /// same through `SelectTool`).
    CancelCrop,
    /// Image › Rotate left / Rotate right (§4.7): turn the Image a quarter.
    Rotate { clockwise: bool },
    /// Image › Flip horizontal / Flip vertical (§4.7).
    Flip { horizontal: bool },
    /// Image › Resize…: open the Resize dialog (§6.4).
    ShowResizeDialog,
    /// Resize the Image to this many pixels (the Resize dialog's **Resize**, §4.7).
    Resize { width: u32, height: u32 },
    /// Ctrl+D (§4.2): duplicate the selected Annotations, offset by 10 px, without touching the
    /// clipboard.
    Duplicate,
    /// Bring to front / Send to back (property bar), Ctrl+] / Ctrl+[ (§4.2).
    Restack(Restack),
    /// Show the next Sibling (§3.1: wheel down, →).
    NextSibling,
    /// Show the previous Sibling (§3.1: wheel up, ←).
    PreviousSibling,
    /// Show the first Sibling (§3.1: Home), in the order next/previous walk.
    FirstSibling,
    /// Show the last Sibling (§3.1: End), in the order next/previous walk.
    LastSibling,
    /// Show the native open dialog and open the file chosen in it (Ctrl+O, §3.5).
    OpenDialog,
    /// Zoom in one step around the canvas centre (`+`, §3.1).
    ZoomIn,
    /// Zoom out one step around the canvas centre (`-`, §3.1).
    ZoomOut,
    /// Multiply the zoom by `factor`, keeping the Image point under `around` (in points) in place
    /// (Ctrl+wheel, §3.1).
    ZoomAround { factor: f32, around: egui::Pos2 },
    /// Move a zoomed Image by this many points (Space+drag, middle-drag, Shift+wheel; #43). A
    /// view change only: not an undo step, and the Image stays saved.
    Pan(egui::Vec2),
    /// Back to fit-to-window (`0`, §3.1).
    ZoomFit,
    /// Enter or leave fullscreen (F11, §2).
    ToggleFullscreen,
    /// Ctrl+S (§6.1): write the Image back in its own format, or Save As (defaulting to PNG) for a
    /// read-only format (§5.1).
    Save,
    /// Ctrl+Shift+S: the native Save As dialog, offering PNG, JPEG and WebP.
    SaveAs,
    /// Ctrl+C (§6.2): with nothing selected, copy the Flattened Image as PNG.
    Copy,
    /// Ctrl+V (§6.3): insert the clipboard image as an Inset; in the empty window, make it the
    /// Image (§3.5).
    Paste,
    /// Ctrl+Shift+V, *File › New from clipboard*: open the clipboard image in a new window.
    NewFromClipboard,
    /// Ctrl+drop (§3.5, §4.6): insert this image file as an Inset.
    InsertFile(PathBuf),
    /// File › Move to trash and its quick action (§2): trash the file on screen even with
    /// Annotations selected (Delete would remove those instead).
    Trash,
    /// File › Copy to… and Alt+C (§6.1, issue #53): open the folder picker and copy the Image's
    /// file into the chosen Target folder.
    CopyTo,
    /// File › Move to… and Alt+M (§6.1, issue #53): open the folder picker and move the Image's
    /// file into the chosen Target folder.
    MoveTo,
    /// Help › Keyboard shortcuts.
    ShowShortcuts,
    /// Help › About lecoq.
    ShowAbout,
    /// View → *Allow multiple instances* (§8.3, §8.4). Persisted; takes effect at the next start.
    SetAllowMultipleInstances(bool),
    /// View → *Follow files after copy/move* (§8.3, issue #53). Persisted.
    SetFollowFiles(bool),
}
