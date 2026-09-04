# Vendored faces

Both are SIL Open Font License 1.1. They are vendored rather than fetched
because a preview and a handheld both have to render with no network, and a CDN
font is a broken surface waiting for an offline device.

| File | Family | Source |
|---|---|---|
| `press-start-2p.woff2` | Press Start 2P | `fonts.gstatic.com/s/pressstart2p/v16` |
| `vt323.woff2` | VT323 | `fonts.gstatic.com/s/vt323/v18` |

Both are the **latin** subset:

```
unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA,
               U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+2074, U+20AC,
               U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD
```

```
sha256  afec86997fdaf54af1f59358fa2c1e2a0f1d04146edad18e5cd141d0384a7548  press-start-2p.woff2
sha256  8ddbebcc1048154132e1d78eb9b1f7850bca1b7d857035ccf1cb4318ebc615b6  vt323.woff2
```

## How this went wrong once, so it does not again

`fonts.googleapis.com/css2` returns **several** `@font-face` blocks for one
family, one per subset, distinguished only by `unicode-range`. Taking the first
URL in that response vendors whichever subset Google happens to list first —
Cyrillic for Press Start 2P, Vietnamese for VT323. Those files are valid woff2,
they load without error, and `document.fonts.load()` resolves with a face. They
simply contain no Latin glyphs, so every character silently falls back to
monospace and the whole surface renders in the wrong font while every automated
check stays green.

Pick the block whose `unicode-range` starts `U+0000-00FF`, and **look at the
rendered text** before believing it. The failure is invisible to the font
loading API and to anything that does not rasterise glyphs.

The checksums above are pinned by `test/fonts.test.ts` so a replacement has to
be deliberate. That test cannot prove glyph coverage — only a browser can — so
re-check by eye when changing either file.
