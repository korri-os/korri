#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.pillow
"""Draw Pico's fixture cover art and write src/fixtures/sample-art.ts.

Every picture here is original: drawn by this script from shapes, gradients
and Pico's own glyph table, so fixtures can be committed with no licence
question. Each one has a different shape, because Korri's covers arrive at
whatever ratio their platform uses and the surface must never assume one:

    1:1  PICO-8 label        3:4  handheld box      2:3  store capsule
    9:16 phone portrait      4:3  old console box   16:9 screenshot
    2:1  banner              2.14:1 store header

The pictures use full colour on purpose. The surface remaps every cover to the
sixteen PICO-8 colours at runtime, and art already in the palette would give
that remap nothing to decide. The output is deterministic: same script, same
bytes.

    ./scripts/draw-sample-art.py       # from surfaces/pico
"""
import base64
import io
import math
import random
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
GLYPHS = ROOT / "src" / "fonts" / "pico8-glyphs.txt"
OUT = ROOT / "src" / "fixtures" / "sample-art.ts"
LONG_SIDE = 128


def read_glyphs() -> dict[str, list[str]]:
    glyphs: dict[str, list[str]] = {}
    lines = GLYPHS.read_text().splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        if not line or line.startswith(";"):
            i += 1
            continue
        code = int(line.split(" ", 1)[0], 16)
        glyphs[chr(code)] = lines[i + 1 : i + 6]
        i += 6
    return glyphs


GLYPH = read_glyphs()


def text_width(text: str, scale: int) -> int:
    return sum((len(GLYPH.get(c.lower(), GLYPH["?"])[0]) + 1) * scale for c in text) - scale


def draw_text(img: Image.Image, text: str, x: int, y: int, scale: int, ink, shadow=None) -> None:
    px = img.load()
    for layer, (dx, dy, colour) in enumerate(([(scale, scale, shadow)] if shadow else []) + [(0, 0, ink)]):
        cx = x
        for c in text:
            rows = GLYPH.get(c.lower(), GLYPH["?"])
            for r, row in enumerate(rows):
                for col, bit in enumerate(row):
                    if bit != "#":
                        continue
                    for sy in range(scale):
                        for sx in range(scale):
                            X, Y = cx + col * scale + sx + dx, y + r * scale + sy + dy
                            if 0 <= X < img.width and 0 <= Y < img.height:
                                px[X, Y] = colour
            cx += (len(rows[0]) + 1) * scale


def title(img: Image.Image, text: str, y: int, ink, shadow, scale: int = 2) -> None:
    """A logo: the title in the glyph table, centred, with a hard shadow."""
    while scale > 1 and text_width(text, scale) > img.width - 8:
        scale -= 1
    draw_text(img, text, (img.width - text_width(text, scale)) // 2, y, scale, ink, shadow)


def lerp(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))


def gradient(img: Image.Image, top, bottom, y0=0, y1=None) -> None:
    d = ImageDraw.Draw(img)
    y1 = img.height if y1 is None else y1
    for y in range(y0, y1):
        d.line([(0, y), (img.width, y)], fill=lerp(top, bottom, (y - y0) / max(1, y1 - y0 - 1)))


def canvas(ratio: float) -> Image.Image:
    if ratio >= 1:
        w, h = LONG_SIDE, round(LONG_SIDE / ratio)
    else:
        w, h = round(LONG_SIDE * ratio), LONG_SIDE
    return Image.new("RGB", (w, h))


def stars(img: Image.Image, rng: random.Random, n: int, y1: int, colour=(255, 250, 235)) -> None:
    px = img.load()
    for _ in range(n):
        px[rng.randrange(img.width), rng.randrange(max(1, y1))] = colour


def ridge(img: Image.Image, rng: random.Random, base: int, amp: int, colour, step=6) -> None:
    d = ImageDraw.Draw(img)
    pts = [(0, img.height)]
    x, y = 0, base
    while x <= img.width + step:
        pts.append((x, y))
        x += step
        y = max(base - amp, min(base + amp, y + rng.randint(-amp // 2, amp // 2)))
    pts.append((img.width, img.height))
    d.polygon(pts, fill=colour)


# ---------------------------------------------------------------- pictures


def summit() -> Image.Image:  # 1:1, a PICO-8 label: a climb to a snowy peak
    rng = random.Random(1)
    img = canvas(1)
    gradient(img, (22, 30, 78), (120, 72, 150))
    stars(img, rng, 40, 70)
    d = ImageDraw.Draw(img)
    d.polygon([(10, 128), (64, 34), (118, 128)], fill=(70, 80, 110))
    d.polygon([(64, 34), (50, 58), (58, 54), (64, 62), (72, 52), (78, 58)], fill=(245, 245, 255))
    d.polygon([(-10, 128), (30, 80), (70, 128)], fill=(40, 48, 80))
    d.polygon([(60, 128), (104, 76), (140, 128)], fill=(52, 60, 96))
    d.rectangle([60, 22, 62, 34], fill=(230, 230, 230))
    d.polygon([(63, 22), (74, 26), (63, 30)], fill=(250, 60, 90))
    d.ellipse([28, 92, 36, 100], fill=(240, 40, 70))
    d.rectangle([31, 89, 33, 92], fill=(60, 200, 90))
    title(img, "celeste", 104, (255, 230, 120), (120, 30, 60))
    return img


def knight() -> Image.Image:  # 2:3 store capsule: a hooded wanderer with a lantern
    rng = random.Random(2)
    img = canvas(2 / 3)
    gradient(img, (16, 20, 40), (40, 60, 110))
    d = ImageDraw.Draw(img)
    for _ in range(14):
        x = rng.randrange(img.width)
        d.polygon([(x - 6, 0), (x + 6, 0), (x, rng.randint(12, 36))], fill=(28, 34, 60))
    d.ellipse([6, 54, 80, 128], fill=(60, 90, 150))
    d.rectangle([0, 112, img.width, img.height], fill=(22, 24, 40))
    # Cloak and hood: a tall triangle with a round hood and a shadowed face.
    d.polygon([(43, 58), (22, 116), (64, 116)], fill=(150, 40, 60))
    d.ellipse([33, 56, 53, 78], fill=(170, 50, 70))
    d.ellipse([37, 62, 49, 76], fill=(20, 16, 30))
    d.rectangle([39, 68, 40, 69], fill=(255, 220, 120))
    d.rectangle([46, 68, 47, 69], fill=(255, 220, 120))
    # The lantern, held out, and its light.
    d.line([(56, 88), (62, 94)], fill=(90, 70, 50), width=2)
    d.ellipse([56, 90, 76, 110], fill=(120, 110, 80))
    d.rectangle([62, 94, 69, 104], fill=(255, 200, 80))
    d.rectangle([64, 97, 67, 101], fill=(255, 250, 210))
    title(img, "hollow", 8, (235, 240, 255), (30, 40, 90))
    title(img, "knight", 22, (235, 240, 255), (30, 40, 90))
    return img


def blocks() -> Image.Image:  # 3:4 handheld box: falling blocks
    img = canvas(3 / 4)
    gradient(img, (240, 200, 120), (200, 110, 60))
    d = ImageDraw.Draw(img)
    d.rectangle([14, 30, 82, 122], fill=(30, 30, 50))
    colours = [(230, 60, 80), (80, 200, 120), (70, 150, 240), (250, 210, 60), (180, 90, 200)]
    cell = 8
    shapes = [
        [(0, 0), (1, 0), (2, 0), (1, 1)],
        [(0, 0), (0, 1), (1, 1), (1, 2)],
        [(0, 0), (1, 0), (0, 1), (1, 1)],
        [(0, 0), (0, 1), (0, 2), (0, 3)],
        [(0, 0), (1, 0), (2, 0), (2, 1)],
    ]
    placed = [(0, 7, 0), (3, 9, 1), (5, 8, 2), (7, 7, 3), (1, 3, 4), (4, 1, 0)]
    for i, (gx, gy, s) in enumerate(placed):
        for cx, cy in shapes[s]:
            x, y = 18 + (gx + cx) * cell, 34 + (gy + cy) * cell
            if y + cell > 122:
                continue
            c = colours[i % len(colours)]
            d.rectangle([x, y, x + cell - 2, y + cell - 2], fill=c)
            d.line([(x, y), (x + cell - 2, y)], fill=lerp(c, (255, 255, 255), 0.5))
    title(img, "tetris", 8, (40, 30, 60), (255, 250, 230))
    return img


def cavern() -> Image.Image:  # 2.14:1 store header: a cave cross-section with gold
    rng = random.Random(4)
    img = canvas(2.14)
    gradient(img, (60, 36, 24), (24, 14, 10))
    d = ImageDraw.Draw(img)
    for y in range(0, img.height, 6):
        for x in range((y // 6) % 2 * 6, img.width, 12):
            d.rectangle([x, y, x + 10, y + 4], fill=(90, 56, 36))
    d.rectangle([0, 44, img.width, 50], fill=(30, 20, 14))
    for x in (18, 70, 110):
        d.rectangle([x, 20, x + 1, 30], fill=(120, 80, 40))
        d.ellipse([x - 3, 13, x + 4, 21], fill=(255, 190, 60))
        d.ellipse([x - 1, 15, x + 2, 19], fill=(255, 250, 200))
    for _ in range(16):
        x, y = rng.randrange(img.width), rng.randrange(52, img.height - 2)
        d.rectangle([x, y, x + 2, y + 1], fill=(255, 215, 40))
    d.rectangle([44, 30, 52, 43], fill=(210, 150, 110))
    d.rectangle([43, 26, 53, 30], fill=(160, 90, 40))
    title(img, "spelunky", 4, (255, 220, 90), (90, 30, 20))
    return img


def tower() -> Image.Image:  # 9:16 phone portrait: a tower and its lantern
    rng = random.Random(5)
    img = canvas(9 / 16)
    gradient(img, (10, 14, 44), (90, 50, 120), 0, 100)
    gradient(img, (40, 30, 60), (20, 16, 30), 100, img.height)
    stars(img, rng, 50, 90)
    d = ImageDraw.Draw(img)
    d.ellipse([46, 10, 62, 26], fill=(255, 245, 210))
    d.rectangle([26, 44, 46, 128], fill=(80, 80, 100))
    for y in range(50, 128, 8):
        d.line([(26, y), (46, y)], fill=(60, 60, 80))
    d.polygon([(22, 44), (36, 26), (50, 44)], fill=(120, 50, 70))
    d.ellipse([31, 48, 41, 58], fill=(255, 200, 70))
    d.ellipse([33, 50, 39, 56], fill=(255, 250, 200))
    for x in range(0, img.width, 6):
        d.rectangle([x, 118 + (x // 6) % 2 * 2, x + 5, 128], fill=(30, 60, 40))
    title(img, "lantern", 78, (255, 210, 90), (60, 20, 60), scale=1)
    title(img, "keep", 86, (255, 210, 90), (60, 20, 60), scale=1)
    return img


def shore() -> Image.Image:  # 4:3 old console box: a beach and a crab
    img = canvas(4 / 3)
    gradient(img, (120, 200, 250), (250, 220, 200), 0, 50)
    gradient(img, (40, 140, 200), (20, 80, 150), 50, 70)
    gradient(img, (245, 215, 150), (220, 180, 110), 70, img.height)
    d = ImageDraw.Draw(img)
    d.ellipse([90, 14, 114, 38], fill=(255, 240, 150))
    for x in range(0, img.width, 16):
        d.arc([x, 64, x + 16, 76], 180, 360, fill=(250, 250, 255))
    d.ellipse([50, 78, 78, 92], fill=(240, 80, 60))
    d.ellipse([44, 74, 52, 82], fill=(240, 80, 60))
    d.ellipse([76, 74, 84, 82], fill=(240, 80, 60))
    d.rectangle([57, 74, 59, 79], fill=(250, 250, 250))
    d.rectangle([69, 74, 71, 79], fill=(250, 250, 250))
    for x in (52, 58, 70, 76):
        d.line([(x, 90), (x - 4 if x < 64 else x + 4, 96)], fill=(200, 60, 50), width=2)
    title(img, "tide pool", 4, (255, 255, 255), (30, 90, 160))
    return img


def comet() -> Image.Image:  # 16:9 screenshot: a courier ship chasing a comet
    rng = random.Random(7)
    img = canvas(16 / 9)
    gradient(img, (8, 6, 28), (40, 16, 70))
    stars(img, rng, 70, img.height)
    d = ImageDraw.Draw(img)
    d.ellipse([84, 36, 140, 92], fill=(200, 110, 60))
    d.ellipse([84, 36, 140, 92], outline=(240, 170, 110))
    d.arc([76, 54, 148, 76], 160, 380, fill=(240, 220, 170), width=2)
    for i in range(24):
        t = i / 24
        x, y = 20 + i * 2, 16 + i
        r = round(1 + t * 4)
        d.ellipse([x - r, y - r, x + r, y + r], fill=lerp((120, 200, 255), (255, 255, 255), t))
    d.polygon([(26, 50), (44, 44), (40, 56)], fill=(230, 230, 240))
    d.polygon([(26, 50), (20, 46), (20, 54)], fill=(255, 140, 60))
    title(img, "comet courier", 60, (140, 240, 255), (40, 20, 90))
    return img


def bramble() -> Image.Image:  # 2:1 banner: a runner at sunset through a forest
    rng = random.Random(8)
    img = canvas(2)
    gradient(img, (250, 120, 90), (255, 210, 120), 0, 40)
    d = ImageDraw.Draw(img)
    d.ellipse([52, 18, 76, 42], fill=(255, 240, 180))
    ridge(img, rng, 38, 6, (150, 70, 90))
    ridge(img, rng, 46, 5, (80, 40, 70))
    for x in range(0, img.width, 14):
        h = rng.randint(14, 26)
        d.polygon([(x, 64), (x + 6, 64 - h), (x + 12, 64)], fill=(30, 60, 50))
    d.rectangle([0, 56, img.width, 64], fill=(40, 30, 40))
    d.ellipse([90, 40, 97, 47], fill=(250, 220, 190))
    d.line([(93, 47), (91, 54)], fill=(60, 140, 240), width=3)
    d.line([(91, 54), (86, 58)], fill=(60, 140, 240), width=2)
    d.line([(91, 54), (96, 58)], fill=(60, 140, 240), width=2)
    title(img, "bramble run", 4, (255, 255, 240), (150, 50, 60))
    return img


PICTURES = [
    ("SUMMIT", "1:1", summit),
    ("KNIGHT", "2:3", knight),
    ("BLOCKS", "3:4", blocks),
    ("CAVERN", "2.14:1", cavern),
    ("TOWER", "9:16", tower),
    ("SHORE", "4:3", shore),
    ("COMET", "16:9", comet),
    ("BRAMBLE", "2:1", bramble),
]


def data_url(img: Image.Image) -> str:
    # An adaptive 64-colour palette keeps each picture small while leaving the
    # runtime remap plenty of off-palette colour to decide about.
    small = img.quantize(colors=64, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    buf = io.BytesIO()
    small.save(buf, format="PNG", optimize=True)
    return "data:image/png;base64," + base64.b64encode(buf.getvalue()).decode()


def main() -> None:
    out = [
        "/**",
        " * Original cover art for fixtures and parts, one picture per cover shape.",
        " *",
        " * GENERATED by scripts/draw-sample-art.py — edit the script, not this file.",
        " * Every picture is drawn by that script, so it carries no licence question",
        " * and renders with no network. The shapes are the point: Korri's covers",
        " * arrive at whatever ratio their platform uses, and a surface tested only",
        " * against squares will crop the first real box it meets.",
        " */",
    ]
    total = 0
    for name, shape, draw in PICTURES:
        img = draw()
        url = data_url(img)
        total += len(url)
        out.append(f"/** {shape}, {img.width}x{img.height}. */")
        out.append(f'export const PICO_ART_{name} = "{url}"')
    OUT.write_text("\n".join(out) + "\n")
    print(f"{OUT.relative_to(ROOT)}: {len(PICTURES)} pictures, {total} bytes of data URL")


if __name__ == "__main__":
    main()
