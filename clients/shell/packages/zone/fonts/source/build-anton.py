#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 python3Packages.fonttools python3Packages.brotli git
"""Subset Anton for the Modern and Blend looks.

    ./scripts/build-anton.py              # from surfaces/zone
    ./scripts/build-anton.py --out DIR    # write the file elsewhere

Anton Regular came into Zone with commit e05b1790f and left it when Zone
Caps arrived. Its source is read back from that commit, never from the
network, and checked against the sha256 it had then, so the input cannot
drift. The script writes:

  src/fonts/Anton-latin.woff2   Anton, subset to Google Fonts' latin and
                                latin-ext ranges plus the few symbols
                                Zone writes (× … ▲), every layout feature
                                kept

The licence travels beside it as src/fonts/Anton-OFL.txt (the same text
the commit carried as OFL.txt; OFL.txt is Jersey 10's today).

The subset is byte-for-byte deterministic for one fontTools version: no
timestamp is touched and the glyph order follows the source. Prints the
sha256 of the source and of the output.
"""
import argparse
import hashlib
import subprocess
import sys
from io import BytesIO
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
FONTS = ROOT / "src" / "fonts"
SOURCE_COMMIT = "e05b1790f"
SOURCE_PATH = "surfaces/zone/src/fonts/Anton-Regular.ttf"
SOURCE_SHA256 = "a4ba3a92350ebb031da0cb47630ac49eb265082ca1bc0450442f4a83ab947cab"
OUTPUT = "Anton-latin.woff2"

# Google Fonts' latin and latin-ext unicode ranges, then the symbols Zone's
# own words use that those ranges leave out.
LATIN = (
    "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,"
    "U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD"
)
LATIN_EXT = (
    "U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+1D00-1DBF,U+1E00-1E9F,"
    "U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF"
)
ZONE_SYMBOLS = "U+00D7,U+2026,U+25B2"


def source_bytes() -> bytes:
    repo = subprocess.run(
        ["git", "-C", str(ROOT), "rev-parse", "--show-toplevel"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()
    data = subprocess.run(
        ["git", "-C", repo, "show", f"{SOURCE_COMMIT}:{SOURCE_PATH}"],
        check=True, capture_output=True,
    ).stdout
    digest = hashlib.sha256(data).hexdigest()
    if digest != SOURCE_SHA256:
        raise SystemExit(f"{SOURCE_COMMIT}:{SOURCE_PATH} is {digest}, expected {SOURCE_SHA256}")
    return data


def unicodes() -> list[int]:
    found: list[int] = []
    for item in ",".join([LATIN, LATIN_EXT, ZONE_SYMBOLS]).split(","):
        lo, _, hi = item.removeprefix("U+").partition("-")
        found.extend(range(int(lo, 16), int(hi or lo, 16) + 1))
    return sorted(set(found))


def build(out: Path) -> Path:
    font = TTFont(BytesIO(source_bytes()))
    options = subset.Options()
    options.layout_features = ["*"]
    options.name_IDs = ["*"]
    options.name_languages = ["*"]
    options.notdef_outline = True
    options.flavor = "woff2"
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=unicodes())
    subsetter.subset(font)
    out.mkdir(parents=True, exist_ok=True)
    target = out / OUTPUT
    font.flavor = "woff2"
    font.save(target, reorderTables=False)
    return target


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, default=FONTS)
    args = parser.parse_args()
    target = build(args.out)
    print(f"{SOURCE_SHA256}  {SOURCE_COMMIT}:{SOURCE_PATH}")
    print(f"{hashlib.sha256(target.read_bytes()).hexdigest()}  {target}")


if __name__ == "__main__":
    sys.exit(main())
