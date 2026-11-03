# Right-click returns to the Select tool

While a drawing tool is active, a right-click (secondary button, acted on at press) switches back to the Select tool, cancelling any shape that is mid-draw; in Select mode a right-click does nothing. This spends the right mouse button on a quick "escape to Select" gesture rather than on the Annotation context menu that [ADR 0005](0005-scroll-to-pan-a-zoomed-image.md) had reserved it for, because leaving a tool is a far more frequent need than any menu, and until now the only ways back to Select were Esc and the tool strip.

Decided as part of the [Improve annotation UX](https://github.com/acastaner/lecoq/issues/47) epic.

## Considered Options

- **Keep the right button reserved for a future Annotation context menu (the status quo of ADR 0005).** Rejected: the menu does not exist yet and has no concrete design, while "get me out of this tool" is wanted on every drawing session. A right-click context menu can still be reached later through a modifier or a long-press if it is ever built; the plain right-click is better spent now.
- **A dedicated on-screen "done" affordance instead of a mouse gesture.** Rejected: Esc and the tool strip already cover the deliberate, pointer-leaves-the-canvas case. The value of right-click is that it is where the pointer already is, mid-drawing.

## Consequences

- The right button is no longer free for an Annotation context menu; the reservation recorded in ADR 0005 is withdrawn. A future menu would need a different trigger.
- Right-click is uniform across every drawing tool, including Text (it commits the in-progress text) and Crop (it discards the pending crop region), because all of them route through the same switch-to-Select path.
- A right-button *drag* carries no separate meaning — it deselects exactly like a right-click.
