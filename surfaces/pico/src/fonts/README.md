# Pico's faces

Ten free pixel fonts with real upper and lower case. Pico uses Tiny5 unless its
root names another face with `data-pico-font`.

| File | What it is |
|---|---|
| `<id>.glyphs.txt` | One face: one pixel bitmap per character, read out of the upstream file. Do not edit by hand. |
| `pico-symbols.txt` | Pico's own symbols (`· … ‹ › ◀ ▶ ▸ ← → ↑ ↓ – —`, curly quotes, `×`, `−`, `█`, `✓`), drawn for Pico in a 3×5 cell. Added to every face that lacks them. |
| `<id>.woff2` | A face, built from its table. Do not edit. |
| `licenses/` | The licence texts that must travel with a face. |
| `../../scripts/fetch-font-sources.sh` | Downloads the upstream files and checks their pinned checksums. |
| `../../scripts/extract-font-tables.py` | Reads each upstream file into its table. |
| `../../scripts/build-fonts.py` | Builds the faces, `../pico-fonts.css` and `../pico-fonts.ts`. Deterministic: same tables, same bytes. |

## The faces

Rows are glyph pixels in one em (ascent plus descent); type sizes are whole
multiples of them.

| id | Face | Caps / x-height / rows | By | Licence | Upstream |
|---|---|---|---|---|---|
| `tiny5` (default) | Tiny5 | 5 / 4 / 7 | The Tiny5 Project Authors (Gissio) | SIL OFL 1.1, text in `licenses/tiny5-OFL.txt` | [google/fonts](https://github.com/google/fonts/tree/main/ofl/tiny5) |
| `tom-thumb` | Tom Thumb | 5 / 4 / 6 | Brian Swetland, Robey Pointer | CC0 1.0 (also MIT, CC BY 3.0) | [robey.lag.net](https://robey.lag.net/2010/01/23/tiny-monospace-font.html), BDF from [u8g2](https://github.com/olikraus/u8g2) |
| `x11-4x6` | X11 fixed 4x6 | 5 / 4 / 6 | X Consortium fixed fonts | Public domain | BDF from [u8g2](https://github.com/olikraus/u8g2) |
| `kenney-mini` | Kenney Mini | 5 / 4 / 7 | Kenney | CC0 1.0 | [kenney.nl](https://kenney.nl/assets/kenney-fonts) |
| `m3x6` | m3x6 | 6 / 5 / 8 | Daniel Linssen | Free to use with attribution | [itch.io](https://managore.itch.io/m3x6) |
| `x11-5x7` | X11 fixed 5x7 | 6 / 4 / 7 | X Consortium fixed fonts | Public domain | BDF from [u8g2](https://github.com/olikraus/u8g2) |
| `spleen-5x8` | Spleen 5x8 | 6 / 5 / 8 | Frederic Cambus | BSD 2-Clause, text in `licenses/spleen-LICENSE.txt` | [fcambus/spleen](https://github.com/fcambus/spleen) |
| `m5x7` | m5x7 | 7 / 5 / 9 | Daniel Linssen | CC0 1.0 (attribution appreciated) | [itch.io](https://managore.itch.io/m5x7) |
| `monogram` | monogram | 7 / 5 / 9 | datagoblin | CC0 1.0 | [itch.io](https://datagoblin.itch.io/monogram) |
| `kenney-pixel` | Kenney Pixel | 7 / 6 / 9 | Kenney | CC0 1.0 | [kenney.nl](https://kenney.nl/assets/kenney-fonts) |

Each built face also carries its copyright and licence in its name table. The
Tiny5 face Pico ships is a Modified Version under the OFL: redrawn on the pixel
grid with `█` and `✓` added. Tiny5 declares no Reserved Font Name.

PICO-8's own face (Zep's glyphs) was dropped on 2026-10-01: it draws upper and
lower case the same, so typed text could not show its case.

## How the faces draw

- One glyph pixel is 100 font units, and the em is the face's rows. A
  `font-size` of rows × N device pixels draws every glyph pixel as an N × N
  block, which is why every type token in `pico-tokens.css` multiplies by
  `--pico-font-rows`.
- A table's em is the ascent and descent of printable ASCII. An accented
  letter that does not fit inside it (most accented capitals) is left out of
  the table, and the face draws its base letter instead, so accents never change
  a face's line height.
- A symbol from `pico-symbols.txt` stands on the baseline. The caret block `█`
  is drawn as tall as the face's capitals and as wide as its `n`.

## The table format

Lines starting `;` are notes. Directive lines (`name`, `by`, `license`,
`license-file`, `copyright`) describe the face. `metrics ascent A descent D`
fixes the rows of every glyph to A + D. Each glyph is a header line,
`<hex code point> <name>`, optionally followed by `left L` (the bitmap's offset
from the pen, when ink sits left of it) and `advance N` (when it differs from
the bitmap's width plus `left`), then A + D rows of `#` ink and `.` paper.

## Changing them

```sh
./scripts/build-fonts.py                 # after editing pico-symbols.txt or the builder
./scripts/fetch-font-sources.sh          # to take a new upstream version: then
./scripts/extract-font-tables.py         # review the table diff and rebuild
```

Then update the checksums in `test/fonts.test.tsx` (the builder prints them)
and look at the rendered text in a browser. A pinned checksum proves the bytes
are the ones built, not that the glyphs look right.
