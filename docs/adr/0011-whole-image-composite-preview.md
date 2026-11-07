---
Status: accepted
---

# The annotated preview is a whole-Image composite at display density, GPU-resampled below it

While the composite is drawn from the display-size pixels (Fit, and any zoom up to the display
density), `lecoq-render` composites the Image plus every Annotation **once per Document `revision`
over the whole Image at the display texture's resolution**, and the canvas draws the visible
sub-rectangle of that texture the same way it draws the plain Image texture. The composite is
re-rendered only when `revision` changes or the zoom crosses the display density, where the
full-resolution pixels take over the current visible-region composite. Pan and zoom with
Annotations then cost what pan and zoom without them cost: the view is no longer part of the
composite's cache key below the display density, so no Skia pass runs per frame.

The price is that below the display density the annotated screen is the display-density composite
**resampled by the GPU**, not a Skia render at the window's exact physical resolution. This amends
§7.2: "the screen and the file match" no longer holds pixel-for-pixel for the on-screen *preview*
at sub-display zooms. **Flatten is unchanged** — it still renders the whole Image plus every
Annotation at full resolution with the same Skia code (ADR 0003), so the saved file is exactly what
it was. Only the preview drifts, and it drifts the same way the plain Image preview already does:
the plain Image below the display density is already a GPU upscale/downscale of the display decode,
so this treats the annotated Image and the plain Image alike rather than holding the annotated one
to a stricter standard.

Decided for [#58](https://github.com/acastaner/lecoq/issues/58), the performance review's finding 4
(pan and zoom with Annotations re-render the visible region every frame), under epic #54. ADR 0003
(one Skia renderer, "screen and file match") is the ADR this one narrows: ADR 0003 still holds for
Flatten and for the exact path at and above display density; it no longer holds for the preview
below display density.

## Decision

- **We accept the GPU-resampled preview below display density.** The alternative — keeping the
  per-frame Skia re-render so the preview stays a pixel-exact Flatten preview — made panning an
  annotated Image visibly worse than panning a plain one (a `save_layer` plus a two-pass blur per
  shadowed shape, every frame, for nothing reused between frames). The exactness it bought was a
  preview property the user cannot see at sub-display zoom, where the screen is already resampled.
- **We amend §7.2 rather than keep the promise literally true.** The gesture-scoped alternative
  (skip the re-render only while a pan/zoom gesture is in flight, re-render once it settles) keeps
  §7.2 intact but helps less — a settled-but-panned view still pays, and a Ctrl+wheel zoom is a
  stream of settles. Amending the spec is the honest record of a trade-off we are making on
  purpose, which is what an ADR is for.
- **The exactness cutover sits at the display density with a small margin**, not exactly on the
  boundary. Right at the boundary, float rounding in `wanted_density` vs `display_density` could
  flap between the resampled whole-Image path (stable key) and the exact visible-region path (view
  in the key) frame to frame, forcing a re-render on alternating frames — the very cost this change
  removes. A small relative margin (`DENSITY_MARGIN`, in the spirit of the sub-pixel slop in
  `is_pannable`) gives the boundary hysteresis. The cost is a sliver of zoom just above display
  density that is resampled when it could have been exact; that sliver is below the resolution the
  eye resolves and is the safe side to err on.

## Considered Options

- **Keep rendering the visible region, cache it and translate it across pan frames.** Rejected:
  it reuses pixels only for pure translation, not for zoom, and it reintroduces the seam at the
  edge of the cached region that the whole-Image texture removes. A whole-Image texture at display
  resolution is bounded (it is the display decode's size, §1.3) and the GPU already does the UV
  draw for the plain Image, so there is no new machinery.
- **Tile the composite.** Rejected as premature: the whole-Image texture at display resolution is
  small enough (bounded by the display box, ~window size) that tiling buys nothing here.
- **Do #55 (the three-copies fix) and stop.** #55 cuts the per-frame cost without touching §7.2,
  and it is worth doing regardless (it reduces the cost this change then amortises). But it leaves
  the per-frame Skia pass in place, so panning an annotated Image still scales with the number and
  blur radius of shadowed shapes. #58 removes the pass entirely below display density; the two
  compose, they are not alternatives.

## Consequences

- Below the display density the composite's cache key is `(revision, source.key, whole-Image view)`
  where the whole-Image view is independent of pan and zoom, so pan and zoom reuse the texture and
  only refresh where it is drawn (`dest`). Above the density, the key keeps the visible-region view
  as before, so detail stays exact when it matters.
- The dirty-region fast path (#46) is unchanged in principle: during a gesture its sub-rectangle
  now maps into the whole-Image texture instead of the visible one (the device-pixel mapping scales
  by the display texture's resolution over the whole Image). The freeze-and-correct for opaque
  neighbourhood effects works on the whole-Image buffer exactly as it did on the visible one.
- Memory and per-revision work go up for a static annotated Image that is never panned: the
  composite texture is the display texture's size rather than the visible region's, so zoomed out it
  is larger than what is on screen. At Fit the two are the same size. The display texture is itself
  bounded by the display box (§1.3), so a 100 MP Image viewed at Fit still composites at ~window
  size, not at 100 MP — the worst case is bounded by the window, not by the Image.
- HiDPI: "display density" is a moving number — the startup decode guesses the scale (§1.3) and the
  display texture is re-decoded when the canvas outgrows it. The composite keys on the current
  display decode's `display_key`, so a re-decode changes the key and re-renders once, which is
  correct; the margin absorbs fractional-scale rounding at the boundary.
- The composite goldens that capture a state below display density where the Image overhangs the
  canvas now compare against a whole-Image, resampled reference. Goldens taken at Fit with the Image
  fully visible are unchanged, because there the whole-Image view and the visible-region view are
  the same render. The golden tolerance already admits resampling differences (a channel and a
  pixel-fraction tolerance); it is not loosened by this change.
