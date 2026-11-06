# Right-click in Select mode opens the file Context menu

While the Select tool is active and no Annotation is selected, a right-click on the canvas opens the Context menu: the clipboard actions (Copy, Paste) and the File operations (Copy to, Move to, Move to trash). This amends [ADR 0006](0006-right-click-returns-to-select.md), which spent the plain right-click on "back to Select" inside a drawing tool and left it doing nothing in Select mode; that drawing-tool behaviour is unchanged, and with an Annotation selected right-click still does nothing. The two uses never collide because they are keyed on the same state the tool strip shows.

The menu is about the file and the clipboard, not about the Annotation under the pointer: ADR 0006 withdrew the reservation of right-click for an *Annotation* context menu, and this does not bring that back. Every entry also lives in the menu bar, so the Context menu adds no action of its own and needs no keyboard trigger.

Decided in [Copy to / Move to: file operations with a folder picker and a right-click Context menu](https://github.com/acastaner/lecoq/issues/53).

## Considered Options

- **A modifier or long-press trigger, as ADR 0006 suggested for a future menu.** Rejected: the plain right-click was free in exactly the state where the menu is wanted (nothing being drawn, nothing selected), and a modifier would make the most common sorting gesture (right-click, Move to) two-handed.
- **Let the menu open with an Annotation selected too, listing Annotation commands.** Rejected: that is the Annotation context menu ADR 0006 declined to design, and it would need right-click to resolve which Annotation it means.
