#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.fonttools python3Packages.brotli
"""Build src/fonts/pico8.woff2 from src/fonts/pico8-glyphs.txt.

One glyph pixel is 100 font units and the em is six pixels tall (five rows of
ink and one of air), so a font-size of 6px x N draws every glyph pixel as an
N x N block of device pixels. The output is byte-for-byte deterministic: the
timestamps are fixed, so a rebuild with an unchanged table changes nothing.

    ./scripts/build-font.py            # from surfaces/pico
"""
import unicodedata
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

ROOT = Path(__file__).resolve().parent.parent
TABLE = ROOT / "src" / "fonts" / "pico8-glyphs.txt"
OUT = ROOT / "src" / "fonts" / "pico8.woff2"

PIXEL = 100
ROWS = 5
UPM = PIXEL * 6
ASCENT = PIXEL * ROWS
DESCENT = -PIXEL
# 2024-01-01T00:00:00Z in seconds since 1904-01-01, the OpenType epoch.
FIXED_TIME = 3786825600


def read_table(path: Path) -> dict[int, list[str]]:
    glyphs: dict[int, list[str]] = {}
    lines = [line.rstrip("\n") for line in path.read_text().splitlines()]
    i = 0
    while i < len(lines):
        line = lines[i]
        if not line or line.startswith(";"):
            i += 1
            continue
        code = int(line.split(" ", 1)[0], 16)
        rows = lines[i + 1 : i + 1 + ROWS]
        widths = {len(row) for row in rows}
        if len(rows) != ROWS or len(widths) != 1 or any(set(r) - {"#", "."} for r in rows):
            raise SystemExit(f"bad glyph U+{code:04X} at line {i + 1}")
        if code in glyphs:
            raise SystemExit(f"duplicate glyph U+{code:04X}")
        glyphs[code] = rows
        i += 1 + ROWS
    return glyphs


def outline(rows: list[str]):
    """One rectangle per horizontal run of ink: few points, exact pixels."""
    pen = TTGlyphPen(None)
    for r, row in enumerate(rows):
        top = ASCENT - r * PIXEL
        bottom = top - PIXEL
        c = 0
        while c < len(row):
            if row[c] != "#":
                c += 1
                continue
            start = c
            while c < len(row) and row[c] == "#":
                c += 1
            left, right = start * PIXEL, c * PIXEL
            # Clockwise contour: TrueType fills clockwise outer contours.
            pen.moveTo((left, bottom))
            pen.lineTo((left, top))
            pen.lineTo((right, top))
            pen.lineTo((right, bottom))
            pen.closePath()
    return pen.glyph()


def main() -> None:
    table = read_table(TABLE)
    names = {code: f"uni{code:04X}" for code in sorted(table)}
    cmap = {code: names[code] for code in table}

    # Capitals draw as their lowercase glyph.
    for code in range(ord("A"), ord("Z") + 1):
        cmap[code] = names[code + 32]
    # Accented Latin draws as its base letter: PICO-8 has no accents, and a
    # missing glyph would fall back to another face mid-word.
    for code in range(0x00C0, 0x0250):
        base = unicodedata.normalize("NFD", chr(code))[0]
        lower = ord(base.lower())
        if code not in cmap and base != chr(code) and lower in names:
            cmap[code] = names[lower]
    cmap[0x00A0] = names[0x20]

    order = [".notdef"] + [names[code] for code in sorted(table)]
    notdef = ["###", "#.#", "#.#", "#.#", "###"]
    glyf = {".notdef": outline(notdef)}
    metrics = {".notdef": ((3 + 1) * PIXEL, 0)}
    for code, rows in table.items():
        glyf[names[code]] = outline(rows)
        metrics[names[code]] = ((len(rows[0]) + 1) * PIXEL, 0)

    fb = FontBuilder(UPM, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyf)
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=ASCENT, descent=DESCENT, lineGap=0)
    fb.setupNameTable({
        "familyName": "Pico8",
        "styleName": "Regular",
        "copyright": "Glyph design by Zep (Lexaloffle Games), CC0 1.0. Built by Korri's Pico surface.",
        "licenseDescription": "CC0 1.0 Universal",
        "licenseInfoURL": "https://creativecommons.org/publicdomain/zero/1.0/",
    })
    fb.setupOS2(
        version=4,
        sTypoAscender=ASCENT, sTypoDescender=DESCENT, sTypoLineGap=0,
        usWinAscent=ASCENT, usWinDescent=-DESCENT, fsSelection=0x80 | 0x40,
    )
    fb.setupPost(isFixedPitch=0)
    fb.setupHead(unitsPerEm=UPM, created=FIXED_TIME, modified=FIXED_TIME)
    font = fb.font
    font.flavor = "woff2"
    font.save(str(OUT), reorderTables=True)
    # fontTools stamps `modified` on save unless told otherwise; re-save with
    # the fixed value so the bytes depend on the table alone.
    from fontTools.ttLib import TTFont
    again = TTFont(str(OUT), recalcTimestamp=False)
    again["head"].modified = FIXED_TIME
    again["head"].created = FIXED_TIME
    again.flavor = "woff2"
    again.save(str(OUT), reorderTables=True)
    print(f"{OUT.relative_to(ROOT)}: {len(table)} glyphs, {len(cmap)} code points, {OUT.stat().st_size} bytes")


if __name__ == "__main__":
    main()
