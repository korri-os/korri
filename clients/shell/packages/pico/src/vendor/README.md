# Vendored code

A surface package ships as plain files with no `node_modules` (the shell's
verifier refuses symlinks), so the one third-party library Pico uses is
copied here unchanged, with its licence.

| Folder | What | Version | Licence |
|---|---|---|---|
| `qrcode/` | `lib/core/` of [qrcode](https://github.com/soldair/node-qrcode): the QR encoder, no renderers. `segments.js` requires `../dijkstrajs/dijkstra.js` instead of the package name. | 1.5.4 | MIT, `qrcode/LICENSE` |
| `dijkstrajs/` | `dijkstra.js` of [dijkstrajs](https://github.com/tcort/dijkstrajs), which `qrcode` uses to split text into segments. | 1.0.3 | MIT, `dijkstrajs/LICENSE.md` |

Pico's web build used the same `qrcode` 1.5.4 (`surfaces/pico/package.json`).
