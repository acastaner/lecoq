# lecoq

*Lire, Éditer, Capturer, Orner et Quadriller*. Le Coq, the rooster. Formerly zimged: it
was renamed after v0.1.6, and settings from `zimged/config.toml` are picked up on first launch.

A small, very fast image viewer and quick editor. Double-click an image and it is on screen almost
at once. The wheel or the arrow keys move through the other images in the same folder. Mark an image
up with shapes, text, numbered steps, highlights and blur, then save or copy it.

Cold start (launch to first image painted) is about 107 ms at p50 on a 12th Gen Intel Core
i5-12600KF under Wayland, against a budget of 150 ms at p90. See `tools/bench`.

## Viewer

- Opens JPEG, PNG, WebP, GIF, BMP, ICO, TIFF and AVIF. Animated GIF and WebP play.
- The wheel or ←/→ moves to the next or previous image in the folder, in the same order as your
  file manager (Dolphin, GNOME Files, Explorer or Finder). The list follows files added, removed
  or renamed while lecoq is open.
- Ctrl+wheel or `+`/`-` zooms, `0` fits the image to the window, F11 is fullscreen.
- Delete moves the file to the system trash and shows the next image.
- EXIF orientation is applied, and images with an embedded colour profile are converted to sRGB.

## Editor

- Tools, with Greenshot's shortcuts: rectangle (R), ellipse (E), line (L), arrow (A), freehand (F),
  text (T), step counter (I), highlight (H: text, area, grayscale or magnify), obfuscate (O:
  pixelate or blur) and crop (C).
- Annotations stay editable until you leave the image: select, move, resize, restyle, reorder,
  duplicate (Ctrl+D) or delete them. They follow the image through crop, rotate, flip and resize.
- Highlight and obfuscate are live: what you see is exactly what gets saved.
- One undo history covers everything (Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y). Saving doesn't clear it.
- Each tool remembers its last style between runs.
- Images over 100 MP open view-only.

## Saving and the clipboard

- Ctrl+S saves in place, keeping EXIF and the colour profile, through a temporary file and an
  atomic rename. Save As writes PNG, JPEG or WebP.
- Ctrl+C copies the whole image, or just the selected annotations, which paste back as editable
  annotations in any lecoq window.
- Ctrl+V pastes a clipboard image on top as an inset; in an empty window it opens it as the image.
  Ctrl+drop of a file does the same.

## Installing

Packages are attached to each [GitHub release](https://github.com/acastaner/lecoq/releases).

- **Linux:** `.deb`, `.rpm`, a tarball, an AUR PKGBUILD and a Flatpak bundle. One binary runs
  natively on Wayland and X11, on any distribution with glibc 2.36 or later (Debian 12, Ubuntu
  23.04, Fedora 37 and newer).
- **Windows 10+:** an MSI and a winget manifest. The MSI is unsigned, so SmartScreen may warn.
- **macOS (Apple silicon and Intel):** `lecoq.app` in a DMG, ad-hoc signed and not notarised.
  Right-click › Open the first time.

lecoq registers under *Open with* for every format it reads and never makes itself the default
viewer.

Windows and macOS builds are best-effort: they aren't tested, and the performance budgets apply
only to Linux.

## Not yet supported

HEIC/HEIF, screenshot capture, a preferences dialog and translations.

## Building

lecoq is a Rust workspace; the pinned toolchain in `rust-toolchain.toml` is picked up by `rustup`.

```bash
cargo build --release
```

The gate (`lint` and `test`) runs locally in a container on a fresh clone of the committed HEAD:

```bash
./tools/local-ci/run.sh
```

See `tools/local-ci/README.md` for details, and `packaging/README.md` for how releases are built.

## Documentation

- [`docs/spec/v1.md`](docs/spec/v1.md): the v1 specification, including the performance budgets.
- [`GLOSSARY.md`](GLOSSARY.md): the project's domain vocabulary.
- [`docs/adr/`](docs/adr): architecture decision records.
- [`packaging/release-notes/`](packaging/release-notes): per-release notes.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
