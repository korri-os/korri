#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 python3Packages.fonttools python3Packages.brotli
"""Build Zone Caps, Zone's own pixel capitals, from the grid drawn by hand.

    ./scripts/build-zone-caps.py              # from surfaces/zone
    ./scripts/build-zone-caps.py --out DIR    # write the two files elsewhere

Reads src/fonts/zone-caps.grid and writes:

  src/fonts/ZoneCaps.ttf     the face, for the tests that read its outlines
  src/fonts/ZoneCaps.woff2   the same face, which zone-fonts.css embeds

One art pixel is 100 font units. The em is the cap: 10 art px, 1000 units.
The ascent holds the cap and 3 px of accent headroom (1300); the descent
holds a comma's or a Q's tail (300). A font-size of 10 x --zone-px CSS px
then draws every art pixel as one --zone-px square.

Every pixel run is one square-cornered rectangle, wound clockwise, so the
outlines are exact pixels and nothing is rounded. Advance widths come from
the grid (the ink and one pixel of gap) and there is no kerning: the digits
are all 7 px wide, so they are tabular.

The output is byte-for-byte deterministic: the timestamps are fixed and the
glyph order follows the code points, so an unchanged grid rebuilds to the
same bytes. Prints one sha256 per file.
"""
import argparse
import hashlib
import sys
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
GRID = ROOT / "src" / "fonts" / "zone-caps.grid"
FONTS = ROOT / "src" / "fonts"

PIXEL = 100
CAP = 10
ASCENT = 13
DESCENT = 3
FAMILY = "Zone Caps"
# 2026-10-20T00:00:00Z in seconds since 1904-01-01, the OpenType epoch.
FIXED_TIME = 3843849600
VERSION = "Version 1.000"


def fail(line_no: int, message: str) -> None:
    raise SystemExit(f"{GRID.name}:{line_no}: {message}")


def parse_code(token: str, line_no: int) -> int:
    if token.startswith("U+"):
        return int(token[2:], 16)
    if len(token) == 1:
        return ord(token)
    fail(line_no, f"not a code point or a character: {token}")
    return 0


def options(tokens: list[str]) -> dict[str, int]:
    found: dict[str, int] = {}
    for token in tokens:
        if "=" in token:
            key, _, value = token.partition("=")
            found[key] = int(value)
    return found


def read_grid(path: Path) -> dict:
    """Glyphs and marks as {rows, top, advance}; compose lines as triples."""
    glyphs: dict[int, dict] = {}
    marks: dict[str, dict] = {}
    composes: list[tuple[int, int, str | None, int | None, int]] = []
    lines = path.read_text(encoding="utf-8").splitlines()
    current: dict | None = None
    for index, raw in enumerate(lines):
        line_no = index + 1
        line = raw.rstrip()
        if line and set(line) <= {"#", "."}:
            if current is None:
                fail(line_no, "a row outside any glyph")
            if current["rows"] and len(line) != len(current["rows"][0]):
                fail(line_no, "a row of another width")
            current["rows"].append(line)
            continue
        current = None
        if not line or line.startswith(";"):
            continue
        words = line.split()
        kind = words[0]
        if kind == "glyph":
            code = parse_code(words[1], line_no)
            if code in glyphs:
                fail(line_no, f"U+{code:04X} drawn twice")
            current = {"rows": [], "top": CAP, **options(words[2:]), "line": line_no}
            glyphs[code] = current
        elif kind == "mark":
            name = words[1]
            if name in marks:
                fail(line_no, f"mark {name} drawn twice")
            current = {"rows": [], "top": ASCENT, **options(words[2:]), "line": line_no}
            marks[name] = current
        elif kind == "compose":
            code = parse_code(words[1], line_no)
            base = parse_code(words[2], line_no)
            rest = [w for w in words[3:] if "=" not in w]
            mark = rest[0] if rest else None
            composes.append((code, base, mark, options(words[3:]).get("dx"), line_no))
        else:
            fail(line_no, f"unknown line: {line}")
    return dict(glyphs=glyphs, marks=marks, composes=composes)


def pixels(rows: list[str], top: int, dx: int = 0) -> set[tuple[int, int]]:
    """Ink as (column, row-above-baseline) pairs; a row spans y-1 .. y."""
    found: set[tuple[int, int]] = set()
    for r, row in enumerate(rows):
        for c, cell in enumerate(row):
            if cell == "#":
                found.add((c + dx, top - r))
    return found


def width_of(rows: list[str]) -> int:
    return len(rows[0]) if rows else 0


def resolve(grid: dict) -> dict[int, dict]:
    """Every character as its ink and its advance, composites flattened."""
    out: dict[int, dict] = {}
    for code, glyph in grid["glyphs"].items():
        width = width_of(glyph["rows"])
        out[code] = dict(
            ink=pixels(glyph["rows"], glyph["top"]),
            advance=glyph.get("advance", width + 1),
            width=width,
        )
    for code, base, mark, dx, line_no in grid["composes"]:
        if code in out:
            fail(line_no, f"U+{code:04X} drawn and composed")
        if base not in out:
            fail(line_no, f"base U+{base:04X} is not drawn above this line")
        source = out[base]
        ink = set(source["ink"])
        if mark is not None:
            if mark not in grid["marks"]:
                fail(line_no, f"no mark named {mark}")
            drawn = grid["marks"][mark]
            offset = dx if dx is not None else (source["width"] - width_of(drawn["rows"])) // 2
            ink |= pixels(drawn["rows"], drawn["top"], offset)
        out[code] = dict(ink=ink, advance=source["advance"], width=source["width"])
    return out


def outline(ink: set[tuple[int, int]]):
    """One clockwise rectangle per horizontal run of ink, top row first."""
    pen = TTGlyphPen(None)
    for y in sorted({y for _, y in ink}, reverse=True):
        for start, end in runs(sorted(x for x, row in ink if row == y)):
            x0, x1 = start * PIXEL, (end + 1) * PIXEL
            y0, y1 = (y - 1) * PIXEL, y * PIXEL
            pen.moveTo((x0, y0))
            pen.lineTo((x0, y1))
            pen.lineTo((x1, y1))
            pen.lineTo((x1, y0))
            pen.closePath()
    return pen.glyph()


def runs(columns: list[int]) -> list[tuple[int, int]]:
    """Consecutive columns as (first, last) pairs."""
    found: list[tuple[int, int]] = []
    for x in columns:
        if found and found[-1][1] == x - 1:
            found[-1] = (found[-1][0], x)
        else:
            found.append((x, x))
    return found


def build_font(chars: dict[int, dict]) -> TTFont:
    names = {code: f"uni{code:04X}" for code in sorted(chars)}
    order = [".notdef"] + [names[code] for code in sorted(chars)]
    notdef = {(c, y) for c in range(5) for y in range(1, CAP + 1) if c in (0, 4) or y in (1, CAP)}
    glyf = {".notdef": outline(notdef)}
    metrics = {".notdef": (6 * PIXEL, 0)}
    for code, char in chars.items():
        glyf[names[code]] = outline(char["ink"])
        left = min((x for x, _ in char["ink"]), default=0)
        metrics[names[code]] = (char["advance"] * PIXEL, left * PIXEL)

    fb = FontBuilder(CAP * PIXEL, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({code: names[code] for code in chars})
    fb.setupGlyf(glyf)
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=ASCENT * PIXEL, descent=-DESCENT * PIXEL, lineGap=0)
    fb.setupNameTable({
        "familyName": FAMILY,
        "styleName": "Regular",
        "uniqueFontIdentifier": f"{FAMILY} Regular {VERSION}",
        "fullName": f"{FAMILY} Regular",
        "psName": "ZoneCaps-Regular",
        "version": VERSION,
        "copyright": "Copyright 2026 The Korri Project Authors",
        "designer": "The Korri Project",
        "licenseDescription": "This Font Software is licensed under the SIL Open Font License, Version 1.1.",
        "licenseInfoURL": "https://openfontlicense.org",
    })
    fb.setupOS2(
        version=4,
        sTypoAscender=ASCENT * PIXEL, sTypoDescender=-DESCENT * PIXEL, sTypoLineGap=0,
        usWinAscent=ASCENT * PIXEL, usWinDescent=DESCENT * PIXEL,
        sCapHeight=CAP * PIXEL, sxHeight=CAP * PIXEL,
        fsSelection=0x80 | 0x40,
        achVendID="KORR",
    )
    fb.setupPost(isFixedPitch=0)
    fb.setupHead(unitsPerEm=CAP * PIXEL, created=FIXED_TIME, modified=FIXED_TIME)
    return fb.font


def save(font: TTFont, path: Path, flavor: str | None) -> None:
    font.flavor = flavor
    font.save(str(path), reorderTables=True)
    # fontTools stamps `modified` on save; re-save with the fixed value so the
    # bytes depend on the grid alone.
    again = TTFont(str(path), recalcTimestamp=False)
    again["head"].modified = FIXED_TIME
    again["head"].created = FIXED_TIME
    again.flavor = flavor
    again.save(str(path), reorderTables=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, default=FONTS, help="where to write the two files")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    chars = resolve(read_grid(GRID))
    for name, flavor in (("ZoneCaps.ttf", None), ("ZoneCaps.woff2", "woff2")):
        path = args.out / name
        save(build_font(chars), path, flavor)
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        print(f"{digest}  {name}")
    print(f"{len(chars)} characters", file=sys.stderr)


if __name__ == "__main__":
    main()
