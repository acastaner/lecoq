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

**Sibling**: another image file in the same directory as the open Image. Moving to the next/previous Sibling is how lecoq works as a viewer.
_Avoid_: neighbour, adjacent file.

**Handle**: a grab point drawn on a selected Annotation for reshaping it. Most Annotations show a ring of handles around a bounding box, and dragging one resizes the whole shape.
_Avoid_: anchor, grabber, knob.

**Endpoint handle**: the Handle at each end of a selected Line or Arrow. A lone selected Line or Arrow shows one Endpoint handle at each end instead of the bounding box, so dragging an end moves only that end.
