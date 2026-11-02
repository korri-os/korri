#!/usr/bin/env python3
"""Write src/plan/faces.ts from Zone's three fonts (fonts/*.ttf).

The logic plans where words stand (contract A2: the plan decides when a row
of buttons wraps and when tabs wrap), so it measures words as .slint draws
them: each glyph's advance and, for Anton, its pair kerning (GPOS kern,
format 1 and 2 pair adjustments). Run from the package folder with
fontTools on the path:

    python3 preview/write-faces.py

test/faces.test.ts fails when the fonts change and this was not run again.
"""
import hashlib
import json
from pathlib import Path

from fontTools.ttLib import TTFont

FACES = {"zoneCaps": "ZoneCaps.ttf", "anton": "Anton-latin.ttf", "jersey": "Jersey10-latin.ttf"}
# Printable ASCII, Latin-1 and the punctuation Korri's words use.
CHARS = [chr(c) for c in range(0x20, 0x7F)] + [chr(c) for c in range(0xA0, 0x100)] + list("…·–—‘’“”•→←↑↓×")


def kerning(font, cmap, chars):
    """Pair kerning (x advance) between the given characters, from GPOS kern lookups."""
    if "GPOS" not in font:
        return {}
    gpos = font["GPOS"].table
    glyph_char = {}
    for ch in chars:
        g = cmap.get(ord(ch))
        if g is not None:
            glyph_char.setdefault(g, []).append(ch)
    lookups = set()
    for record in gpos.FeatureList.FeatureRecord:
        if record.FeatureTag == "kern":
            lookups.update(record.Feature.LookupListIndex)
    pairs = {}
    for index in sorted(lookups):
        lookup = gpos.LookupList.Lookup[index]
        subtables = lookup.SubTable
        if lookup.LookupType == 9:
            subtables = [s.ExtSubTable for s in subtables]
        for sub in subtables:
            if getattr(sub, "LookupType", 2) != 2 and lookup.LookupType != 9:
                continue
            coverage = sub.Coverage.glyphs
            if sub.Format == 1:
                for first, pairset in zip(coverage, sub.PairSet):
                    for record in pairset.PairValueRecord:
                        value = getattr(record.Value1, "XAdvance", 0) if record.Value1 else 0
                        if value:
                            for a in glyph_char.get(first, []):
                                for b in glyph_char.get(record.SecondGlyph, []):
                                    pairs.setdefault(a + b, value)
            elif sub.Format == 2:
                class1 = sub.ClassDef1.classDefs
                class2 = sub.ClassDef2.classDefs
                for first in coverage:
                    if first not in glyph_char:
                        continue
                    row = sub.Class1Record[class1.get(first, 0)]
                    for second, chars2 in glyph_char.items():
                        record = row.Class2Record[class2.get(second, 0)]
                        value = getattr(record.Value1, "XAdvance", 0) if record.Value1 else 0
                        if value:
                            for a in glyph_char[first]:
                                for b in chars2:
                                    pairs.setdefault(a + b, value)
    return pairs


def face(path):
    font = TTFont(path)
    cmap = font.getBestCmap()
    hmtx = font["hmtx"].metrics
    advances = {}
    for ch in CHARS:
        g = cmap.get(ord(ch))
        if g is not None:
            advances[ch] = hmtx[g][0]
    missing = hmtx[".notdef"][0] if ".notdef" in hmtx else round(sum(advances.values()) / len(advances))
    return {
        "unitsPerEm": font["head"].unitsPerEm,
        "missing": missing,
        "advances": advances,
        "kerning": kerning(font, cmap, CHARS),
    }


def main():
    root = Path(__file__).resolve().parent.parent
    faces = {}
    digest = hashlib.sha256()
    for name, file in FACES.items():
        data = (root / "fonts" / file).read_bytes()
        digest.update(data)
        faces[name] = face(root / "fonts" / file)
    body = json.dumps(faces, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    out = root / "src" / "plan" / "faces.ts"
    out.write_text(
        "/**\n"
        " * Zone's three faces as .slint draws them: each glyph's advance and Anton's\n"
        " * pair kerning, in font units. Written by preview/write-faces.py from\n"
        " * fonts/*.ttf; do not edit. src/plan/measure.ts reads it.\n"
        " */\n"
        f'export const ZONE_FONTS_SHA256 = "{digest.hexdigest()}"\n\n'
        "export interface ZoneFace {\n"
        "  readonly unitsPerEm: number\n"
        "  /** The advance of a character the face does not hold. */\n"
        "  readonly missing: number\n"
        "  readonly advances: Readonly<Record<string, number>>\n"
        "  /** Two characters to the kerning between them. */\n"
        "  readonly kerning: Readonly<Record<string, number>>\n"
        "}\n\n"
        f"export const ZONE_FACES: Readonly<Record<\"zoneCaps\" | \"anton\" | \"jersey\", ZoneFace>> = {body}\n"
    )


if __name__ == "__main__":
    main()
