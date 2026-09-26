# Pico's face

`pico8.woff2` is PICO-8's own glyph design, built by this repository.

| File | What it is |
|---|---|
| `pico8-glyphs.txt` | The design: one pixel bitmap per character. |
| `pico8.woff2` | The font, generated from the table. Do not edit. |
| `../../scripts/build-font.py` | The builder. Deterministic: same table, same bytes. |

## Licence

The glyph design is Zep's (Lexaloffle Games), released under
[CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/): see the
[PICO-8 FAQ](https://www.lexaloffle.com/pico-8.php?page=faq). The bitmaps for
printable ASCII were read from jacobpierce's reproduction of the design. That
TTF was not vendored: its FontStruct metadata names a CC BY-NC-SA licence,
which conflicts with the MIT licence on its repository, and a non-commercial
file does not belong in a product. The table carries only the design, and
`build-font.py` makes a new file from it.

The characters after "Additions" in the table are Pico's own, drawn in the
same 3×5 cell for copy PICO-8 has no glyph for: `· … ‹ › ◀ ▶ ▸ ← → ↑ ↓ – —`,
curly quotes, `×`, `−`, `█` and `✓`.

## How it draws

- The em is six glyph pixels tall: five rows of ink and one of air. A
  `font-size` of 6 × N device pixels draws every glyph pixel as an N × N block,
  which is why every size token in `pico-tokens.css` is a multiple of six.
- Capitals map to the lowercase glyph, so copy looks the same whatever its
  case. PICO-8 itself draws button icons for some capitals; Pico draws its
  face buttons as elements instead (`PicoHint`).
- Accented Latin letters map to their base letter. A missing glyph would fall
  back to another face in the middle of a word.

## Changing it

```sh
./scripts/build-font.py      # from surfaces/pico
```

Then update the checksum in `test/fonts.test.ts` and look at the rendered text
in a browser. A pinned checksum proves the bytes are the ones built, not that
the glyphs look right.

```
sha256  f1cc9fa26350ad56630c949266508239e379fadb07c74a7f156da1286bbec7ac  pico8.woff2
```
