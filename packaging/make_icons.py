#!/usr/bin/env python3
"""Rebuild the Windows and macOS icons, and the window-icon PNG, from packaging/linux/lecoq.svg (#42).

    packaging/make_icons.py

Writes packaging/windows/lecoq.ico, packaging/macos/lecoq.icns, and
crates/lecoq/icon/lecoq-256.png (the eframe window/title-bar icon, embedded via include_bytes!).
All three are committed, so the release jobs and `cargo build` need no SVG renderer; run this again
whenever the SVG changes. Needs an SVG renderer (`rsvg-convert` from librsvg, else the cairosvg
Python package) and Pillow, for instance in a container:

    docker run --rm -v "$PWD:/w" -w /w debian:bookworm-slim sh -c \
      'apt-get update && apt-get install -y librsvg2-bin python3-pil && python3 packaging/make_icons.py'
"""

import io
import subprocess
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent
SVG = ROOT / "linux" / "lecoq.svg"
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
ICNS_SIZES = [16, 32, 64, 128, 256, 512, 1024]
PNG_SIZE = 256


def _render_bytes(size: int) -> bytes:
    """Rasterise the SVG to PNG bytes at `size`x`size`, preferring rsvg-convert."""
    try:
        return subprocess.run(
            ["rsvg-convert", "-w", str(size), "-h", str(size), str(SVG)],
            check=True,
            capture_output=True,
        ).stdout
    except FileNotFoundError:
        import cairosvg  # type: ignore

        return cairosvg.svg2png(url=str(SVG), output_width=size, output_height=size)


def render(size: int) -> Image.Image:
    return Image.open(io.BytesIO(_render_bytes(size))).convert("RGBA")


def main() -> None:
    # Each size is rendered from the SVG rather than scaled down from the largest one.
    ico = [render(size) for size in ICO_SIZES]
    out = ROOT / "windows" / "lecoq.ico"
    out.parent.mkdir(exist_ok=True)
    ico[-1].save(out, format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=ico[:-1])

    icns = [render(size) for size in ICNS_SIZES]
    out = ROOT / "macos" / "lecoq.icns"
    out.parent.mkdir(exist_ok=True)
    icns[-1].save(out, format="ICNS", append_images=icns[:-1])

    # The eframe window/title-bar icon, embedded by crates/lecoq/src/startup.rs.
    png = render(PNG_SIZE)
    out = REPO / "crates" / "lecoq" / "icon" / "lecoq-256.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    png.save(out, format="PNG")


if __name__ == "__main__":
    main()
