# Skia is the single renderer for Annotations

Annotations are drawn by Skia (`skia-safe`) on the CPU, both on screen (at screen resolution) and when they are Flattened at full resolution for save or copy. egui draws only the window chrome and the selection handles. One renderer keeps the screen identical to the saved file, including highlight and obfuscate, which work live on the pixels beneath them. Skia also has blur, drop shadow, cubic and mipmap resampling, and HarfBuzz text shaping with system fonts built in.

Decided in [Lock the stack](https://github.com/acastaner/lecoq/issues/10).

## Considered Options

- **Draw on screen with egui's painter and Flatten with something else.** Rejected because two renderers would drift apart in anti-aliasing, text and effects.
- **tiny-skia + cosmic-text.** About 200 KiB against Skia's few MiB, and pure Rust. Rejected because blur, shadow and good downscaling would be our own code, against the "prefer mature libraries" rule.

## Consequences

- Skia is initialised lazily, on the first Annotation, so it never touches cold start.
- Builds download prebuilt Skia binaries, or need LLVM, Python and Ninja to build Skia from source.
