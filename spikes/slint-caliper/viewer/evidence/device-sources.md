# Device sources (read 2026-11-07)

| Device | Pixels (Korri) | Millimetres | Source of the millimetres |
|---|---|---|---|
| rg35xxsp | 640 x 480, scale 1: `nix/devices/h700/board.nix` | 71.12 x 53.34 | Anbernic, https://anbernic.com/products/rg35xxsp: "3.5-inch IPS full viewing angle, OCA full lamination/ 640*480" (read through the store's `search/suggest.json` product body; the product page answered 429). 3.5 in x 25.4 = 88.9 mm diagonal, 4:3, square pixels. Also `nix/devices/h700/README.md`. |
| rg35xxpro | 640 x 480, scale 1: `nix/devices/h700/board.nix` | 71.12 x 53.34 | Anbernic, https://anbernic.com/products/rg35xxpro: the same sentence. |
| rpminiv2 | 1240 x 1080, scale 1: `nix/devices/rpminiv2/hardware.nix` (1080x1240, transform 90) | 75 x 65 | Unresolved. Korri's own former Caliper device (`git show 28e385db3^:surfaces/pico/vite.config.ts`) says "Millimetres inferred from a 3.92 in diagonal". goretroid.com answered 429 to every product URL, and its store search no longer lists the Mini V2. |
| rg353m | 640 x 480: `nix/devices/rg353m/hardware.nix` | 72 x 52 | Caliper `src/client/device-frame.js` `HANDHELD_DEVICES`. |
| odin2portal | 1920 x 1080: `nix/devices/odin2portal/hardware.nix` (1080x1920, transform 270) | 156 x 85 | Caliper `src/client/device-frame.js` `HANDHELD_DEVICES`. |

Caliper draws a frame's height from the CSS aspect (`frameGeometry`), so the RG353M frame is
72 x 54 mm, not 72 x 52, and the Odin 2 Portal frame is 156 x 87.75 mm. The viewer does the same.
