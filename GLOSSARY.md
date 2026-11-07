# Glossary

**Image**: the raster file currently open in lecoq's single window.

**Annotation**: a shape, text, step counter, highlight, obfuscation or Inset placed over the Image. It stays a separate, editable object (select, move, resize, restyle, delete) for the rest of the session, including across Image operations.
_Avoid_: layer, drawing, markup.

**Inset**: an Annotation that holds pasted or dropped pixels, such as a clipboard image placed on top of the Image.
_Avoid_: image annotation, picture (confusable with Image).

**Step counter**: a numbered marker Annotation. Step counters on an Image are always numbered 1…n in the order they were created; deleting one renumbers the rest.

**Flatten**: burning every Annotation into the Image's pixels. Saving and copying the Image to the clipboard always flatten. lecoq never stores Annotations in a file of their own.

**Image operation**: an edit that changes the Image's pixels or geometry directly (crop, rotate, flip, resize), as opposed to adding an Annotation. Annotations follow the Image through it.

**History**: the single undo/redo sequence for the open Image, covering both Annotation edits and Image operations. One gesture is one step. It ends when you move to another Sibling.

**Sibling**: another image file in the same directory as the open Image. Moving to the next/previous Sibling is how lecoq works as a viewer; Home jumps to the first Sibling and End to the last, in the same order.
_Avoid_: neighbour, adjacent file.

**Handle**: a grab point drawn on a selected Annotation for reshaping it. Most Annotations show a ring of handles around a bounding box, and dragging one resizes the whole shape.
_Avoid_: anchor, grabber, knob.

**Endpoint handle**: the Handle at each end of a selected Line or Arrow. A lone selected Line or Arrow shows one Endpoint handle at each end instead of the bounding box, so dragging an end moves only that end.

**File operation**: an action on the Image's file as a whole rather than its pixels: Copy to, Move to, Move to trash. The clipboard-born Image, which has no file, offers none of them.
_Avoid_: file action, disk operation.

**Copy to**: the File operation that places a copy of the Image's file in a Target folder. Not Copy, which puts the Flattened Image on the clipboard, and not Duplicate, which clones Annotations.
_Avoid_: copy to folder, export.

**Move to**: the File operation that relocates the Image's file to a Target folder. Its sibling is Move to trash, which relocates it to the system trash.
_Avoid_: move to folder, relocate, rename.

**Target folder**: the folder a Copy to or Move to puts the file in, chosen in a native folder picker. The picker opens at the last Target folder used, remembered between sessions; with none yet, or if it no longer exists, it opens at the Image's own folder.
_Avoid_: destination, output folder.

**Context menu**: the menu a right-click on the canvas opens while the Select tool is active and no Annotation is selected. It holds the clipboard actions and the File operations.
_Avoid_: right-click menu, popup.

**Follow**: the View option that makes the window show the file at its new location after a Copy to or Move to, with the Target folder's Siblings. Off by default, so sorting a folder keeps you in it.
_Avoid_: jump to, open after copy.
