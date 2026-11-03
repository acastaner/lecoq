#!/usr/bin/env python3
"""Regenerate the small format fixtures in tests/fixtures/ (spec §5.1, §5.2). Needs Pillow.

  tools/fixtures/make_fixtures.py

Every fixture is tiny and flat-coloured, so tests can check the pixel size and a few pixel colours.
The ICC profile with red and green swapped is built here by hand (a v2 matrix/TRC profile), so no
third-party profile is committed.

Pillow can't write AVIF; `avif-64x48.avif` was encoded once with the `image` crate's `avif`
feature (ravif), from the same flat COLOUR, 64×48, quality 90, and committed as is.
"""
import io, os, struct

from PIL import Image, ImageCms

OUT = os.path.join(os.path.dirname(__file__), "..", "..", "tests", "fixtures")
COLOUR = (40, 120, 200)
OTHER = (200, 40, 40)


def s15f16(v):
    return struct.pack(">i", round(v * 65536))


def xyz_tag(x, y, z):
    return b"XYZ " + bytes(4) + s15f16(x) + s15f16(y) + s15f16(z)


def desc_tag(text):
    ascii_ = text.encode("ascii") + b"\0"
    return (b"desc" + bytes(4) + struct.pack(">I", len(ascii_)) + ascii_
            + bytes(4) + bytes(4) + bytes(2) + bytes(1) + bytes(67))


def gamma_tag(gamma):
    return b"curv" + bytes(4) + struct.pack(">IH", 1, round(gamma * 256))


def matrix_profile(name, red, green, blue, gamma=2.2):
    """A v2 RGB display profile: colorants (D50 XYZ) and a pure gamma curve per channel."""
    trc = gamma_tag(gamma)
    tags = [
        (b"desc", desc_tag(name)),
        (b"cprt", b"text" + bytes(4) + b"No copyright, use freely\0"),
        (b"wtpt", xyz_tag(0.9642, 1.0, 0.8249)),
        (b"rXYZ", xyz_tag(*red)),
        (b"gXYZ", xyz_tag(*green)),
        (b"bXYZ", xyz_tag(*blue)),
        (b"rTRC", trc),
        (b"gTRC", trc),
        (b"bTRC", trc),
    ]
    table_len = 4 + 12 * len(tags)
    offset = 128 + table_len
    table, data = struct.pack(">I", len(tags)), b""
    for sig, body in tags:
        while (offset + len(data)) % 4:
            data += b"\0"
        table += sig + struct.pack(">II", offset + len(data), len(body))
        data += body
    size = offset + len(data)
    header = (struct.pack(">I", size) + bytes(4) + bytes([2, 0x10, 0, 0]) + b"mntrRGB XYZ "
              + struct.pack(">6H", 2026, 1, 1, 0, 0, 0) + b"acsp" + bytes(4) + bytes(4)
              + bytes(4) + bytes(4) + bytes(8) + bytes(4)
              + s15f16(0.9642) + s15f16(1.0) + s15f16(0.8249) + bytes(4) + bytes(16) + bytes(28))
    assert len(header) == 128
    return header + table + data


# sRGB's colorants, adapted to D50. The swapped profile puts green's where red's should be and red's
# where green's should be, so pure red in it is pure green in sRGB.
SRGB_R = (0.4361, 0.2225, 0.0139)
SRGB_G = (0.3851, 0.7169, 0.0971)
SRGB_B = (0.1431, 0.0606, 0.7141)
SWAPPED = matrix_profile("Red and green swapped (lecoq test)", SRGB_G, SRGB_R, SRGB_B)


def path(name):
    return os.path.join(OUT, name)


def flat(size, colour=COLOUR, mode="RGB"):
    return Image.new(mode, size, colour)


def main():
    flat((64, 48)).save(path("png-64x48.png"))

    rgba = Image.new("RGBA", (64, 48), (0, 0, 0, 0))
    rgba.paste(Image.new("RGBA", (32, 48), COLOUR + (255,)), (0, 0))
    rgba.save(path("rgba-64x48.png"))

    flat((64, 48)).save(path("webp-64x48.webp"), quality=90)

    flat((64, 48)).convert("P", palette=Image.Palette.ADAPTIVE).save(path("gif-64x48.gif"))

    # Animations (§3.4) with uneven timing: COLOUR for 100 ms, then 300 ms of a frame whose left half
    # is OTHER. In the GIF the second frame stores only that left half (a sub-rectangle drawn over
    # frame 1, disposal "keep"), so the right half on screen proves the decoder composited it.
    half = flat((64, 48))
    half.paste(flat((32, 48), OTHER), (0, 0))
    first = flat((64, 48)).convert("P", palette=Image.Palette.ADAPTIVE)
    first.save(path("anim-partial-64x48.gif"), save_all=True,
               append_images=[half.convert("P", palette=Image.Palette.ADAPTIVE)],
               duration=[100, 300], loop=0, disposal=1, optimize=False)
    flat((64, 48)).save(path("anim-64x48.webp"), save_all=True, append_images=[half],
                        duration=[100, 300], loop=0, lossless=True)

    flat((64, 48)).save(path("bmp-64x48.bmp"))
    # ICO entries hold RGBA (the `image` crate rejects an RGB PNG entry).
    flat((48, 48), COLOUR + (255,), "RGBA").save(path("ico-48x48.ico"), sizes=[(16, 16), (48, 48)])
    flat((64, 48)).save(path("tiff-64x48.tif"))

    # Stored 80×40, left half red and right half blue, EXIF orientation 6 (rotate 90° clockwise to
    # display): shown 40×80, red on top and blue below.
    rotated = Image.new("RGB", (80, 40), (255, 0, 0))
    rotated.paste(Image.new("RGB", (40, 40), (0, 0, 255)), (40, 0))
    exif = Image.Exif()
    exif[0x0112] = 6
    rotated.save(path("exif-orientation-6.jpg"), exif=exif.tobytes(), quality=95, subsampling=0)

    red = flat((64, 48), (255, 0, 0))
    red.save(path("red-swapped-icc.jpg"), icc_profile=SWAPPED, quality=95, subsampling=0)
    srgb = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB")).tobytes()
    red.save(path("red-srgb-icc.jpg"), icc_profile=srgb, quality=95, subsampling=0)


if __name__ == "__main__":
    main()
