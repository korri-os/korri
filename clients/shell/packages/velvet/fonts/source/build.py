"""Velvet's static faces, cut from the variable fonts (SIL OFL 1.1).

The shell draws with Slint's software renderer; a static instance needs no
variation support. Sources: google/fonts `BigShoulders[opsz,wght].ttf` and
`Archivo[wdth,wght].ttf`, subset to Latin.

    python3 build.py SOURCE_DIR OUT_DIR
"""
import sys
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

source, out = sys.argv[1], sys.argv[2]


def cut(file, axes, name, family, style, weight):
    font = TTFont(f"{source}/{file}")
    font.flavor = None
    face = instancer.instantiateVariableFont(font, axes, inplace=False)
    names = face["name"]
    for record in list(names.names):
        if record.nameID in (1, 16):
            record.string = family
        elif record.nameID in (2, 17):
            record.string = style
        elif record.nameID == 4:
            record.string = f"{family} {style}"
        elif record.nameID == 6:
            record.string = f"{family}-{style}".replace(" ", "")
        elif record.nameID == 25:
            names.names.remove(record)
    face["OS/2"].usWeightClass = weight
    face.save(f"{out}/{name}")
    print(name)


cut("BigShoulders.woff", {"wght": 800, "opsz": 72}, "BigShoulders-ExtraBold.ttf", "Big Shoulders", "ExtraBold", 800)
cut("Archivo.woff", {"wght": 450, "wdth": 88}, "Archivo-SemiCondensed.ttf", "Archivo", "Regular", 400)
cut("Archivo.woff", {"wght": 650, "wdth": 88}, "Archivo-SemiCondensed-Bold.ttf", "Archivo", "Bold", 700)
