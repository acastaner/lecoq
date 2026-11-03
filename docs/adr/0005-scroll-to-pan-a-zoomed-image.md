# Scroll pans a zoomed Image, instead of navigating Siblings

While the Image is zoomed past Fit (larger than the canvas), a plain scroll pans it on both axes, following the platform's natural-scroll direction. At Fit it still navigates Siblings. The split switches on one notion of zoom state — the Image is larger than the canvas — which is the same "pannable" condition the grab cursor uses, so scroll-to-pan and the cursor affordance never disagree. The arrow keys always navigate Siblings, in either state, so navigation is never lost.

This makes the trackpad-native gesture (a two-finger scroll) pan a zoomed Image, which is what a laptop user reaches for: a MacBook trackpad has no middle button, and a two-finger scroll is how every other app pans. The routing lives in the pan gesture module alongside Space+drag, middle-drag and Shift+wheel.

Decided in [Scroll-to-pan a zoomed Image](https://github.com/acastaner/lecoq/issues/50), part of the [Improve annotation UX](https://github.com/acastaner/lecoq/issues/47) epic.

## Considered Options

- **Right-click-drag to pan.** Rejected: the right button is reserved for a future Annotation context menu. Spending it on panning now would take it back later, and the zoom-state scroll rule already gives laptops a native gesture without a button.
- **A modifier for scroll-to-pan (keep plain scroll on Siblings always).** Rejected: the whole point is the gesture a trackpad user already makes, with no modifier. Shift+wheel panning stays for those who want an explicit modifier.
- **Scroll always navigates Siblings; pan only by drag/Shift+wheel (the status quo).** Rejected: it forces a zoomed laptop user onto Space+drag or Shift+wheel, and a two-finger scroll flips through the folder instead of panning, which is surprising.

## Consequences

- While zoomed, a scroll (notched wheel included) no longer changes Siblings; it pans. A mouse-wheel user who wants the next Sibling while zoomed uses → / ← (or zooms back to Fit). This is the accepted trade-off of a predictable, device-independent, zoom-state rule.
- "Pannable" stays a single shared notion (zoomed beyond Fit): the scroll router and the grab cursor read the same condition, so there is no second definition to drift.
- "Pannable" is global, not per-axis: the Image is pannable when it is larger than the canvas on *either* axis. When it is larger on one axis only, a plain scroll on the other axis neither pans (that axis already fits) nor navigates Siblings; the arrow keys still navigate. This is the accepted consequence of keeping one global condition rather than routing scroll per axis.
- Out-of-bounds scrolls are clamped by the view, exactly as the other pan inputs are, so panning stops where the Image edge meets the canvas edge.
