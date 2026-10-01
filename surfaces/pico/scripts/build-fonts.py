#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.fonttools python3Packages.brotli
"""Build Pico's faces from their glyph tables.

    ./scripts/build-fonts.py      # from surfaces/pico

Reads src/fonts/<id>.glyphs.txt and src/fonts/pico-symbols.txt, and writes:

  src/fonts/<id>.woff2   one face per table
  src/pico-fonts.css     @font-face rules, and the family and rows a root
                         naming each face with data-pico-font gets
  src/pico-fonts.ts      the faces as data, for the surface and its tests

One glyph pixel is 100 font units, and the em is the face's rows: its ascent
plus its descent. A font-size of rows x N device pixels then draws every glyph
pixel as an N x N block. Every symbol in pico-symbols.txt that a face lacks is
added to it, standing on the baseline; the caret block is drawn as tall as the
face's capitals and as wide as its "n", so it sits with the letters. An
accented letter a face lacks draws as its base letter.

The output is byte-for-byte deterministic: the timestamps are fixed, so an
unchanged table rebuilds to the same bytes. Print-out: one sha256 per face, for
test/fonts.test.tsx.
"""
import hashlib
import unicodedata
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
FONTS = ROOT / "src" / "fonts"
CSS = ROOT / "src" / "pico-fonts.css"
TS = ROOT / "src" / "pico-fonts.ts"

# The order Pico lists its faces in; the first is the default.
ORDER = [
    "tiny5", "tom-thumb", "x11-4x6", "kenney-mini", "m3x6",
    "x11-5x7", "spleen-5x8", "m5x7", "monogram", "kenney-pixel",
]
DEFAULT = "tiny5"
PIXEL = 100
CARET = 0x2588
# 2024-01-01T00:00:00Z in seconds since 1904-01-01, the OpenType epoch.
FIXED_TIME = 3786825600
LICENSE_URLS = {"tiny5": "https://openfontlicense.org"}


def read_table(path: Path) -> dict:
    meta: dict[str, str] = {}
    glyphs: dict[int, dict] = {}
    lines = path.read_text().splitlines()
    ascent = descent = None
    i = 0
    while i < len(lines):
        line = lines[i]
        if not line or line.startswith(";"):
            i += 1
            continue
        head, _, rest = line.partition(" ")
        if len(head) in (4, 5) and all(c in "0123456789ABCDEF" for c in head):
            if ascent is None:
                raise SystemExit(f"{path.name}: glyphs before metrics")
            code = int(head, 16)
            words = rest.split()
            options = {words[k]: int(words[k + 1]) for k in range(1, len(words) - 1, 2)}
            rows = lines[i + 1 : i + 1 + ascent + descent]
            if len(rows) != ascent + descent or len({len(r) for r in rows}) != 1 or any(set(r) - {"#", "."} for r in rows):
                raise SystemExit(f"{path.name}: bad glyph {head} at line {i + 1}")
            if code in glyphs:
                raise SystemExit(f"{path.name}: duplicate glyph {head}")
            left = options.get("left", 0)
            glyphs[code] = dict(rows=rows, left=left, advance=options.get("advance", len(rows[0]) + left))
            i += 1 + ascent + descent
            continue
        if head == "metrics":
            words = rest.split()
            ascent, descent = int(words[1]), int(words[3])
        else:
            meta[head] = rest
        i += 1
    return dict(meta=meta, ascent=ascent, descent=descent, glyphs=glyphs)


def inked_height(glyph: dict, ascent: int) -> int:
    rows = [r for r, row in enumerate(glyph["rows"]) if "#" in row]
    return 0 if not rows else ascent - rows[0]


def add_symbols(face: dict, symbols: dict) -> str:
    ascent, descent = face["ascent"], face["descent"]
    glyphs = face["glyphs"]
    added = ""
    for code, symbol in symbols["glyphs"].items():
        if code in glyphs:
            continue
        if code == CARET:
            cap = inked_height(glyphs[ord("H")], ascent)
            width = max(3, glyphs[ord("n")]["advance"] - 1)
            rows = ["." * width] * (ascent - cap) + ["#" * width] * cap + ["." * width] * descent
            glyphs[code] = dict(rows=rows, left=0, advance=width + 1)
        else:
            width = len(symbol["rows"][0])
            pad = ["." * width]
            rows = pad * (ascent - symbols["ascent"]) + symbol["rows"] + pad * descent
            glyphs[code] = dict(rows=rows, left=0, advance=width + 1)
        added += chr(code)
    return added


def outline(rows: list[str], left: int, ascent: int):
    """One rectangle per horizontal run of ink: few points, exact pixels."""
    pen = TTGlyphPen(None)
    for r, row in enumerate(rows):
        top = (ascent - r) * PIXEL
        c = 0
        while c < len(row):
            if row[c] != "#":
                c += 1
                continue
            start = c
            while c < len(row) and row[c] == "#":
                c += 1
            x0, x1 = (left + start) * PIXEL, (left + c) * PIXEL
            # Clockwise contour: TrueType fills clockwise outer contours.
            pen.moveTo((x0, top - PIXEL))
            pen.lineTo((x0, top))
            pen.lineTo((x1, top))
            pen.lineTo((x1, top - PIXEL))
            pen.closePath()
    return pen.glyph()


def family(face_id: str, face: dict) -> str:
    return f"Pico {face['meta']['name']}"


def build(face_id: str, face: dict) -> Path:
    ascent, descent = face["ascent"], face["descent"]
    glyphs = face["glyphs"]
    names = {code: f"uni{code:04X}" for code in sorted(glyphs)}
    cmap = {code: names[code] for code in glyphs}
    # An accented letter the face lacks draws as its base letter, never as
    # another face in the middle of a word.
    for code in range(0x00C0, 0x0250):
        if code in cmap:
            continue
        base = ord(unicodedata.normalize("NFD", chr(code))[0])
        if base != code and base in names:
            cmap[code] = names[base]
    cmap.setdefault(0x00A0, names[0x20])

    order = [".notdef"] + [names[code] for code in sorted(glyphs)]
    notdef = ["." * 3] * (ascent - 5) + ["###", "#.#", "#.#", "#.#", "###"] + ["..."] * descent
    glyf = {".notdef": outline(notdef, 0, ascent)}
    metrics = {".notdef": (4 * PIXEL, 0)}
    for code, glyph in glyphs.items():
        glyf[names[code]] = outline(glyph["rows"], glyph["left"], ascent)
        metrics[names[code]] = (glyph["advance"] * PIXEL, 0)

    meta = face["meta"]
    fb = FontBuilder(PIXEL * (ascent + descent), isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyf)
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=PIXEL * ascent, descent=-PIXEL * descent, lineGap=0)
    names_table = {
        "familyName": family(face_id, face),
        "styleName": "Regular",
        "copyright": f"{meta['copyright']}. Redrawn on Pico's pixel grid by Korri's Pico surface.",
        "licenseDescription": meta["license"],
    }
    if face_id in LICENSE_URLS:
        names_table["licenseInfoURL"] = LICENSE_URLS[face_id]
    fb.setupNameTable(names_table)
    fb.setupOS2(
        version=4,
        sTypoAscender=PIXEL * ascent, sTypoDescender=-PIXEL * descent, sTypoLineGap=0,
        usWinAscent=PIXEL * ascent, usWinDescent=PIXEL * descent, fsSelection=0x80 | 0x40,
    )
    fb.setupPost(isFixedPitch=0)
    fb.setupHead(unitsPerEm=PIXEL * (ascent + descent), created=FIXED_TIME, modified=FIXED_TIME)
    out = FONTS / f"{face_id}.woff2"
    font = fb.font
    font.flavor = "woff2"
    font.save(str(out), reorderTables=True)
    # fontTools stamps `modified` on save; re-save with the fixed value so the
    # bytes depend on the table alone.
    again = TTFont(str(out), recalcTimestamp=False)
    again["head"].modified = FIXED_TIME
    again["head"].created = FIXED_TIME
    again.flavor = "woff2"
    again.save(str(out), reorderTables=True)
    return out


def ts_string(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def write_css(faces: list[tuple[str, dict]]) -> None:
    parts = [
        "/**",
        " * Pico's faces. Generated by scripts/build-fonts.py from src/fonts/; do not",
        " * edit. A root naming a face with data-pico-font gets its family and its",
        " * rows; with none named, pico-tokens.css uses the default face.",
        " */",
        "",
    ]
    for face_id, face in faces:
        parts += [
            "@font-face {",
            f"\tfont-family: \"{family(face_id, face)}\";",
            f"\tsrc: url(\"./fonts/{face_id}.woff2\") format(\"woff2\");",
            "\tfont-display: block;",
            "}",
            "",
        ]
    for face_id, face in faces:
        parts += [
            f"[data-pico-font=\"{face_id}\"] {{",
            f"\t--pico-font: \"{family(face_id, face)}\", monospace;",
            f"\t--pico-font-rows: {face['ascent'] + face['descent']};",
            "}",
            "",
        ]
    CSS.write_text("\n".join(parts))


def write_ts(faces: list[tuple[str, dict]]) -> None:
    lines = [
        "/**",
        " * Pico's faces, as data. Generated by scripts/build-fonts.py from",
        " * src/fonts/; do not edit.",
        " */",
        "export const PICO_FONT_IDS = [",
        *[f"  {ts_string(face_id)}," for face_id, _ in faces],
        "] as const",
        "",
        "export type PicoFontId = (typeof PICO_FONT_IDS)[number]",
        "",
        "export interface PicoFont {",
        "  readonly id: PicoFontId",
        "  readonly name: string",
        "  /** The CSS family pico-fonts.css declares for it. */",
        "  readonly family: string",
        "  /** Glyph pixels in one em: ascent plus descent. Type sizes are whole multiples. */",
        "  readonly rows: number",
        "  /** Glyph pixels from the baseline to the top of an H, and of an x. */",
        "  readonly capHeight: number",
        "  readonly xHeight: number",
        "  readonly by: string",
        "  readonly license: string",
        "  /** The licence text shipped in src/fonts/licenses/, when the licence asks for it. */",
        "  readonly licenseFile?: string",
        "}",
        "",
        "export const PICO_FONTS: readonly PicoFont[] = [",
    ]
    for face_id, face in faces:
        meta = face["meta"]
        ascent = face["ascent"]
        lines += [
            "  {",
            f"    id: {ts_string(face_id)},",
            f"    name: {ts_string(meta['name'])},",
            f"    family: {ts_string(family(face_id, face))},",
            f"    rows: {ascent + face['descent']},",
            f"    capHeight: {inked_height(face['glyphs'][ord('H')], ascent)},",
            f"    xHeight: {inked_height(face['glyphs'][ord('x')], ascent)},",
            f"    by: {ts_string(meta['by'])},",
            f"    license: {ts_string(meta['license'])},",
            *([f"    licenseFile: {ts_string(meta['license-file'])},"] if "license-file" in meta else []),
            "  },",
        ]
    lines += [
        "]",
        "",
        "/** The face Pico uses when its root names none. */",
        f"export const PICO_DEFAULT_FONT: PicoFontId = {ts_string(DEFAULT)}",
        "",
    ]
    TS.write_text("\n".join(lines))


def main() -> None:
    tables = sorted(p.name.removesuffix(".glyphs.txt") for p in FONTS.glob("*.glyphs.txt"))
    if sorted(ORDER) != tables:
        raise SystemExit(f"tables {tables} do not match ORDER {ORDER}")
    symbols = read_table(FONTS / "pico-symbols.txt")
    faces = []
    for face_id in ORDER:
        face = read_table(FONTS / f"{face_id}.glyphs.txt")
        added = add_symbols(face, symbols)
        out = build(face_id, face)
        digest = hashlib.sha256(out.read_bytes()).hexdigest()
        print(f'  "{face_id}.woff2": "{digest}",  // {out.stat().st_size} bytes, added {added or "-"}')
        faces.append((face_id, face))
    write_css(faces)
    write_ts(faces)


if __name__ == "__main__":
    main()
