#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.fonttools python3Packages.freetype-py
"""Read each of Pico's faces out of its upstream file into a glyph table.

    ./scripts/fetch-font-sources.sh      # first, once
    ./scripts/extract-font-tables.py     # from surfaces/pico

Writes src/fonts/<id>.glyphs.txt and copies the licence texts that must travel
with a face into src/fonts/licenses/. The tables are what the repository keeps
and what scripts/build-fonts.py builds from; the upstream files are not
committed. Run this only to take a new upstream version, then review the diff.

Each face is drawn by FreeType at its native pixel size, one bit per pixel: a
BDF at its only strike, a TrueType at the size where its outline grid is one
pixel. The em of a table is the ascent and descent of printable ASCII. A
Latin-1 or Latin Extended-A glyph that does not fit inside that em (most
accented capitals) is left out, and the builder draws its base letter instead,
so accents never change a face's line height.
"""
import shutil
import subprocess
import unicodedata
from functools import reduce
from math import gcd
from pathlib import Path

import freetype
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
FONTS = ROOT / "src" / "fonts"
REPO = Path(subprocess.run(["git", "-C", str(ROOT), "rev-parse", "--show-toplevel"],
                           capture_output=True, text=True, check=True).stdout.strip())
SOURCES = REPO / ".tmp" / "pico-font-sources"

# The ten faces of the survey, in the order Pico lists them.
FACES = [
    dict(id="tiny5", name="Tiny5", file="Tiny5-Regular.ttf", by="The Tiny5 Project Authors (Gissio)",
         license="SIL Open Font License 1.1", license_file="tiny5-OFL.txt", license_source="Tiny5-OFL.txt",
         copyright="Copyright 2022-2024 The Tiny5 Project Authors (https://github.com/Gissio/font_tiny5)",
         source="https://github.com/google/fonts/tree/main/ofl/tiny5"),
    dict(id="tom-thumb", name="Tom Thumb", file="tom-thumb.bdf", by="Brian Swetland and Robey Pointer",
         license="CC0 1.0 (also offered under MIT and CC BY 3.0)",
         copyright="Tom Thumb by Robey Pointer, after a font by Brian Swetland",
         source="https://robey.lag.net/2010/01/23/tiny-monospace-font.html"),
    dict(id="x11-4x6", name="X11 fixed 4x6", file="x11-4x6.bdf", by="the X Consortium fixed fonts",
         license="Public domain", copyright="Public domain font. Share and enjoy.",
         source="https://github.com/olikraus/u8g2/tree/master/tools/font/bdf"),
    dict(id="kenney-mini", name="Kenney Mini", file="kenney/Fonts/Kenney Mini.ttf", by="Kenney (www.kenney.nl)",
         license="CC0 1.0", copyright="Kenney Fonts by Kenney (www.kenney.nl), CC0 1.0",
         source="https://kenney.nl/assets/kenney-fonts"),
    dict(id="m3x6", name="m3x6", file="m3x6.ttf", by="Daniel Linssen",
         license="Free to use with attribution", copyright="m3x6 by Daniel Linssen",
         source="https://managore.itch.io/m3x6"),
    dict(id="x11-5x7", name="X11 fixed 5x7", file="x11-5x7.bdf", by="the X Consortium fixed fonts",
         license="Public domain", copyright="Public domain font. Share and enjoy.",
         source="https://github.com/olikraus/u8g2/tree/master/tools/font/bdf"),
    dict(id="spleen-5x8", name="Spleen 5x8", file="spleen-5x8.bdf", by="Frederic Cambus",
         license="BSD 2-Clause", license_file="spleen-LICENSE.txt", license_source="spleen-LICENSE",
         copyright="Copyright (c) 2018-2026, Frederic Cambus",
         source="https://github.com/fcambus/spleen"),
    dict(id="m5x7", name="m5x7", file="m5x7.ttf", by="Daniel Linssen",
         license="CC0 1.0 (attribution appreciated)", copyright="m5x7 by Daniel Linssen",
         source="https://managore.itch.io/m5x7"),
    dict(id="monogram", name="monogram", file="monogram.ttf", by="datagoblin",
         license="CC0 1.0", copyright="monogram by datagoblin, CC0 1.0",
         source="https://datagoblin.itch.io/monogram"),
    dict(id="kenney-pixel", name="Kenney Pixel", file="kenney/Fonts/Kenney Pixel.ttf", by="Kenney (www.kenney.nl)",
         license="CC0 1.0", copyright="Kenney Fonts by Kenney (www.kenney.nl), CC0 1.0",
         source="https://kenney.nl/assets/kenney-fonts"),
]

ASCII = list(range(0x20, 0x7F))
EXTENDED = list(range(0xA1, 0x180))


def symbol_codes() -> list[int]:
    codes = []
    for line in (FONTS / "pico-symbols.txt").read_text().splitlines():
        head = line.split(" ", 1)[0]
        if len(head) in (4, 5) and all(c in "0123456789ABCDEF" for c in head):
            codes.append(int(head, 16))
    return codes


def native_ppem(path: Path) -> int:
    """The pixel size at which a pixel TrueType's outline grid is one pixel."""
    font = TTFont(str(path))
    glyf = font["glyf"]
    cmap = font.getBestCmap()
    values = []
    for name in {cmap[c] for c in ASCII if c in cmap}:
        glyph = glyf[name]
        if glyph.numberOfContours > 0:
            coords, _, _ = glyph.getCoordinates(glyf)
            values += [abs(int(v)) for xy in coords for v in xy if int(v) != 0]
    unit = reduce(gcd, values)
    ppem = round(font["head"].unitsPerEm / unit)
    if ppem > 32:
        raise SystemExit(f"{path.name}: no pixel grid found (unit {unit})")
    return ppem


def draw(face, flags, code: int):
    face.load_char(chr(code), flags)
    glyph = face.glyph
    bitmap = glyph.bitmap
    ink = []
    for y in range(bitmap.rows):
        row = []
        for x in range(bitmap.width):
            byte = bitmap.buffer[y * bitmap.pitch + x // 8]
            row.append(bool(byte & (0x80 >> (x % 8))))
        ink.append(row)
    return dict(ink=ink, left=glyph.bitmap_left, top=glyph.bitmap_top, advance=glyph.advance.x // 64)


def grid(glyph: dict, ascent: int, descent: int) -> tuple[list[str], int, int] | None:
    """Rows spanning the em, columns spanning the advance and any ink outside it."""
    ink, left, top, advance = glyph["ink"], glyph["left"], glyph["top"], glyph["advance"]
    width = len(ink[0]) if ink else 0
    inked = [(y, x) for y, row in enumerate(ink) for x, on in enumerate(row) if on]
    if inked:
        highest = top - min(y for y, _ in inked)
        lowest = top - max(y for y, _ in inked) - 1
        if highest > ascent or lowest < -descent:
            return None
    x0 = min(0, left)
    x1 = max(advance, left + width)
    rows = []
    for row_top in range(ascent, -descent, -1):
        y = top - row_top
        row = ""
        for x in range(x0, x1):
            bx = x - left
            on = 0 <= y < len(ink) and 0 <= bx < width and ink[y][bx]
            row += "#" if on else "."
        rows.append(row or ".")
    return rows, x0, advance


def extract(face_spec: dict, symbols: list[int]) -> None:
    path = SOURCES / face_spec["file"]
    face = freetype.Face(str(path))
    if face.is_scalable:
        face.set_pixel_sizes(0, native_ppem(path))
        flags = freetype.FT_LOAD_RENDER | freetype.FT_LOAD_TARGET_MONO | freetype.FT_LOAD_NO_HINTING
    else:
        face.select_size(0)
        flags = freetype.FT_LOAD_RENDER | freetype.FT_LOAD_TARGET_MONO

    def has(code: int) -> bool:
        return code == 0x20 or face.get_char_index(code) != 0

    ascii_glyphs = {c: draw(face, flags, c) for c in ASCII if has(c)}
    missing = [chr(c) for c in ASCII if c not in ascii_glyphs]
    if missing:
        raise SystemExit(f"{face_spec['id']}: no glyph for {''.join(missing)!r}")
    inked = [g for g in ascii_glyphs.values() if any(any(r) for r in g["ink"])]
    ascent = max(g["top"] for g in inked)
    descent = max(1, max(len(g["ink"]) - g["top"] for g in inked))

    out = [
        f"; {face_spec['name']}, by {face_spec['by']}.",
        f"; Licence: {face_spec['license']}.",
        f"; {face_spec['copyright']}",
        f"; Source: {face_spec['source']}",
        "; Read out of the upstream file by scripts/extract-font-tables.py; do not",
        "; edit by hand. Format: see README.md in this folder.",
        "",
        f"name {face_spec['name']}",
        f"by {face_spec['by']}",
        f"license {face_spec['license']}",
        *([f"license-file {face_spec['license_file']}"] if "license_file" in face_spec else []),
        f"copyright {face_spec['copyright']}",
        f"metrics ascent {ascent} descent {descent}",
        "",
    ]
    kept = 0
    for code in dict.fromkeys(ASCII + EXTENDED + symbols):
        if not has(code):
            continue
        result = grid(ascii_glyphs.get(code) or draw(face, flags, code), ascent, descent)
        if result is None:
            continue
        rows, left, advance = result
        width = len(rows[0])
        extra = (f" left {left}" if left != 0 else "") + (f" advance {advance}" if advance != width + left else "")
        label = unicodedata.name(chr(code), f"U+{code:04X}").lower().replace(" ", "-")
        out.append(f"{code:04X} {label}{extra}")
        out.extend(rows)
        out.append("")
        kept += 1
    (FONTS / f"{face_spec['id']}.glyphs.txt").write_text("\n".join(out))
    if "license_file" in face_spec:
        (FONTS / "licenses").mkdir(exist_ok=True)
        shutil.copyfile(SOURCES / face_spec["license_source"], FONTS / "licenses" / face_spec["license_file"])
    print(f"{face_spec['id']:<13} ascent {ascent} descent {descent}  {kept} glyphs")


def main() -> None:
    symbols = symbol_codes()
    for face_spec in FACES:
        extract(face_spec, symbols)


if __name__ == "__main__":
    main()
