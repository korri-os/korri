"""Draws Velvet's test pictures: the heroes, posters, logos and covers its
preview scenarios and tests name. Every pixel is made here, from flat bands
and blocks; nothing is copied from a game, a store or the mockup.

    python3 preview/art/build.py        (in packages/velvet)

The files are small on purpose (a hero is 640 x 206); a scenario gives each
its stored size (1920 x 620 and so on), which a surface uses for its shape
only: the shell scales a stored picture before a surface sees it.
"""
import os
import struct
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))


def png(path, w, h, px):
    raw = b"".join(b"\x00" + bytes(px[y * w * 4:(y + 1) * w * 4]) for y in range(h))

    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(os.path.join(HERE, path), "wb") as f:
        f.write(data)


def rnd(seed):
    state = [seed & 0xFFFFFFFF]

    def nxt():
        state[0] = (state[0] * 1664525 + 1013904223) & 0xFFFFFFFF
        return state[0] / 4294967296

    return nxt


def hexc(c):
    return [(c >> 16) & 255, (c >> 8) & 255, c & 255]


def mix(a, b, t):
    return [round(a[i] * (1 - t) + b[i] * t) for i in range(3)]


def scene(path, w, h, sky, sun, land, seed):
    """A flat landscape: banded sky, a sun, ridges, a figure of blocks."""
    r = rnd(seed)
    sky, sun, land = hexc(sky), hexc(sun), hexc(land)
    px = [0] * (w * h * 4)
    bands = 7
    ridges = [(0.55 + 0.1 * i, [h * (0.5 + 0.11 * i) + (r() - 0.5) * h * 0.2 for _ in range(9)]) for i in range(4)]
    cx, cy, rad = w * (0.55 + r() * 0.3), h * (0.25 + r() * 0.2), h * (0.16 + r() * 0.1)
    for y in range(h):
        for x in range(w):
            band = int(y / h * bands)
            c = mix(sky, [8, 8, 12], band / bands * 0.75)
            if (x - cx) ** 2 + (y - cy) ** 2 < rad * rad:
                c = sun
            for depth, (shade, pts) in enumerate(ridges):
                seg = x / w * (len(pts) - 1)
                i = min(int(seg), len(pts) - 2)
                top = pts[i] + (pts[i + 1] - pts[i]) * (seg - i)
                step = h * 0.03
                if y > round(top / step) * step:
                    c = mix(land, [6, 6, 10], 0.18 + depth * 0.2)
            o = (y * w + x) * 4
            px[o:o + 4] = c + [255]
    # A small figure of blocks, off to the right of the middle.
    fx, fy, u = int(w * 0.62), int(h * 0.42), max(2, h // 26)
    for by in range(9):
        for bx in range(5):
            if r() < 0.55 or (bx == 2 and by < 7):
                col = mix(sun, [250, 244, 232], 0.4) if by < 3 else mix(sky, [20, 16, 24], 0.55)
                for yy in range(u):
                    for xx in range(u):
                        for mx in (bx, 4 - bx):
                            X, Y = fx + mx * u + xx, fy + by * u + yy
                            if 0 <= X < w and 0 <= Y < h:
                                o = (Y * w + X) * 4
                                px[o:o + 4] = col + [255]
    png(path, w, h, px)


FONT = {
    "A": "01110,10001,10001,11111,10001,10001,10001", "B": "11110,10001,10001,11110,10001,10001,11110",
    "C": "01111,10000,10000,10000,10000,10000,01111", "D": "11110,10001,10001,10001,10001,10001,11110",
    "E": "11111,10000,10000,11110,10000,10000,11111", "G": "01111,10000,10000,10011,10001,10001,01111",
    "H": "10001,10001,10001,11111,10001,10001,10001", "I": "11111,00100,00100,00100,00100,00100,11111",
    "K": "10001,10010,10100,11000,10100,10010,10001", "L": "10000,10000,10000,10000,10000,10000,11111",
    "M": "10001,11011,10101,10101,10001,10001,10001", "N": "10001,11001,10101,10011,10001,10001,10001",
    "O": "01110,10001,10001,10001,10001,10001,01110", "P": "11110,10001,10001,11110,10000,10000,10000",
    "R": "11110,10001,10001,11110,10100,10010,10001", "S": "01111,10000,10000,01110,00001,00001,11110",
    "T": "11111,00100,00100,00100,00100,00100,00100", "U": "10001,10001,10001,10001,10001,10001,01110",
    "W": "10001,10001,10001,10101,10101,11011,10001", "Y": "10001,10001,01010,00100,00100,00100,00100",
    " ": "00000,00000,00000,00000,00000,00000,00000",
}


def logo(path, w, h, lines, ink, shade):
    """A wordmark of blocks on a clear ground, with a hard shade under it."""
    ink, shade = hexc(ink), hexc(shade)
    px = [0] * (w * h * 4)
    rows = len(lines) * 8 - 1
    cols = max(len(line) * 6 - 1 for line in lines)
    # The word fills its picture, as a stored logo does: its blocks stretch to the shape.
    ux, uy = (w - 6) / cols, (h - 6) / rows
    u = min(ux, uy)
    ox, oy = 2, 2

    def block(X, Y, col):
        for y in range(int(Y), int(Y + uy + 0.999)):
            for x in range(int(X), int(X + ux + 0.999)):
                if 0 <= x < w and 0 <= y < h:
                    o = (y * w + x) * 4
                    px[o:o + 4] = col + [255]

    for pas, (dx, dy, col) in enumerate(((max(1, u / 2), max(1, u / 2), shade), (0, 0, ink))):
        for li, line in enumerate(lines):
            start = ox + (cols - (len(line) * 6 - 1)) * ux / 2
            for ci, ch in enumerate(line):
                glyph = FONT.get(ch, FONT[" "]).split(",")
                for gy, bits in enumerate(glyph):
                    for gx, bit in enumerate(bits):
                        if bit == "1":
                            block(start + (ci * 6 + gx) * ux + dx, oy + (li * 8 + gy) * uy + dy, col)
    png(path, w, h, px)


def main():
    # Heroes: about 3.1 to 1, as a stored hero is.
    scene("hero-ember.png", 640, 206, 0xB0452A, 0xF2C46B, 0x4A1F1A, 11)
    scene("hero-tide.png", 640, 206, 0x2A7A8C, 0xE8F0D8, 0x123844, 23)
    scene("hero-moss.png", 640, 206, 0x4A7A3A, 0xF0E08A, 0x1E3A1C, 37)
    scene("hero-dusk.png", 640, 206, 0x5A3A8A, 0xF29AB0, 0x231438, 41)
    # Posters: 2 to 3.
    scene("poster-ember.png", 200, 300, 0xB0452A, 0xF2C46B, 0x4A1F1A, 12)
    scene("poster-tide.png", 200, 300, 0x2A7A8C, 0xE8F0D8, 0x123844, 24)
    scene("poster-moss.png", 200, 300, 0x4A7A3A, 0xF0E08A, 0x1E3A1C, 38)
    scene("poster-sand.png", 200, 300, 0xC9A45A, 0xFFF4D0, 0x5A4320, 52)
    scene("poster-dusk.png", 200, 300, 0x5A3A8A, 0xF29AB0, 0x231438, 43)
    # Square covers.
    scene("cover-ember.png", 128, 128, 0xB0452A, 0xF2C46B, 0x4A1F1A, 13)
    scene("cover-tide.png", 128, 128, 0x2A7A8C, 0xE8F0D8, 0x123844, 25)
    # Logos, in the shapes stored logos come in: long (3.4 to 1), usual (2.3 to 1), tall (1.5 to 1).
    logo("logo-ember.png", 392, 169, ["EMBER", "ROAD"], 0xF7E9D0, 0x7A2A16)
    logo("logo-tide.png", 472, 140, ["TIDE POOL"], 0xE8F6F4, 0x0E3A44)
    logo("logo-moss.png", 388, 249, ["MOSS", "AND", "STONE"], 0xF4F0C8, 0x1E3A1C)
    logo("logo-dusk.png", 415, 178, ["DUSK", "RUNNER"], 0xF6D8E6, 0x2A1444)
    logo("logo-sand.png", 385, 152, ["SAND", "KING"], 0xFFF4D0, 0x5A4320)


if __name__ == "__main__":
    main()
