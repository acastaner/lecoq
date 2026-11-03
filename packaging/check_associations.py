#!/usr/bin/env python3
"""Check that every package associates lecoq with exactly the formats it reads (#42, spec §8.5).

    packaging/check_associations.py

The source of truth is lecoq_codec::READ_EXTENSIONS. Compared with it: the Linux desktop file
and AppStream metainfo (as MIME types), the WiX source's `Extensions`, the macOS Info.plist's
CFBundleDocumentTypes, and the winget installer manifest's FileExtensions. Also checks that none
of them makes lecoq the default handler: no WiX <ProgId>/<Extension> and no write of an
extension key's default value, and LSHandlerRank Alternate on every macOS document type.

Run by the release workflow's gate-record job; standard library only.
"""

import plistlib
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PKG = ROOT / "packaging"

# The MIME types the desktop file and metainfo must list for each extension (all of them for the
# desktop file, the first for the metainfo).
MIME = {
    "jpg": ["image/jpeg"],
    "jpeg": ["image/jpeg"],
    "png": ["image/png"],
    "webp": ["image/webp"],
    "gif": ["image/gif"],
    "bmp": ["image/bmp", "image/x-bmp", "image/x-ms-bmp"],
    "ico": ["image/vnd.microsoft.icon", "image/x-icon"],
    "tif": ["image/tiff"],
    "tiff": ["image/tiff"],
    "avif": ["image/avif"],
}

errors: list[str] = []


def compare(what: str, found: set[str], expected: set[str]) -> None:
    if found != expected:
        missing, extra = sorted(expected - found), sorted(found - expected)
        errors.append(f"{what}: missing {missing}, unexpected {extra}")


def read_extensions() -> set[str]:
    src = (ROOT / "crates/lecoq-codec/src/lib.rs").read_text()
    m = re.search(r"pub const READ_EXTENSIONS: &\[&str\] = &\[(.*?)\];", src, re.S)
    if not m:
        sys.exit("ERROR: READ_EXTENSIONS not found in crates/lecoq-codec/src/lib.rs")
    return set(re.findall(r'"([a-z0-9]+)"', m.group(1)))


def main() -> int:
    exts = read_extensions()
    unknown = exts - MIME.keys()
    if unknown:
        errors.append(f"READ_EXTENSIONS has {sorted(unknown)}: add its MIME types to MIME here")
    mimes = {m for e in exts & MIME.keys() for m in MIME[e]}

    # Linux.
    desktop = (PKG / "linux/lecoq.desktop").read_text()
    line = next((l for l in desktop.splitlines() if l.startswith("MimeType=")), "MimeType=")
    compare("linux/lecoq.desktop MimeType", set(filter(None, line[9:].split(";"))), mimes)
    metainfo = ET.parse(PKG / "linux/io.github.acastaner.lecoq.metainfo.xml")
    compare(
        "linux metainfo <provides><mediatype>",
        {m.text for m in metainfo.iterfind("provides/mediatype")},
        {MIME[e][0] for e in exts & MIME.keys()},
    )

    # Windows: the WiX source.
    wxs = (PKG / "windows/lecoq.wxs").read_text()
    m = re.search(r'<\?define Extensions = "([^"]*)" \?>', wxs)
    compare("windows/lecoq.wxs Extensions", set(m.group(1).split(";")) if m else set(), exts)
    body = re.sub(r"<!--.*?-->", "", wxs, flags=re.S)
    if re.search(r"<(ProgId|Extension)\b", body):
        errors.append("windows/lecoq.wxs: <ProgId>/<Extension> write the extension's default value")
    for key in re.findall(r'<RegistryValue\b[^>]*\bKey="([^"]*)"[^>]*>', body):
        tail = key.rsplit("\\", 1)[-1]
        if tail.startswith("."):
            errors.append(f"windows/lecoq.wxs: writes under {key}, an extension's own key")

    # Windows: the winget installer manifest (a flat list, so no YAML parser is needed).
    yaml = (PKG / "windows/winget/acastaner.lecoq.installer.yaml").read_text()
    m = re.search(r"^FileExtensions:\n((?:  - .*\n)+)", yaml, re.M)
    found = set(re.findall(r"  - (\S+)", m.group(1))) if m else set()
    compare("windows/winget installer FileExtensions", found, exts)

    # macOS.
    with open(PKG / "macos/Info.plist", "rb") as f:
        plist = plistlib.load(f)
    types = plist.get("CFBundleDocumentTypes", [])
    compare(
        "macos/Info.plist CFBundleTypeExtensions",
        {e for t in types for e in t.get("CFBundleTypeExtensions", [])},
        exts,
    )
    for t in types:
        if t.get("LSHandlerRank") != "Alternate":
            errors.append(f"macos/Info.plist: {t.get('CFBundleTypeName')} is not LSHandlerRank Alternate")
        if not t.get("LSItemContentTypes"):
            errors.append(f"macos/Info.plist: {t.get('CFBundleTypeName')} has no LSItemContentTypes")

    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    if not errors:
        print(f"associations OK: {' '.join(sorted(exts))}")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
