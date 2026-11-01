# Pico package (spike)

Pico's home screen with the Ready catalog, as a package the spike shell loads
at run time (`../../CONTRACT.md`). Throwaway, like the rest of the spike.

| File | What it is |
|---|---|
| `main.slint` | `Surface`: header, stage, shelf, floor row. A port of `PicoHome`, `PicoScreenShell`, `PicoCartShelf`, `PicoGameStage`, `PicoGameFacts` and their CSS. |
| `cart.slint` | One cartridge (`PicoCart.css`): shell, system silhouette, ridges, art or initials sticker, NEW band, resume tab. |
| `palette.slint` | PICO-8's sixteen and Pico's roles (`pico-tokens.css`). |
| `logic.ts`, `logic.js` | Pico's own `update`, `initialNavigation` and `shownScreen`, with an adapter from the shell's `Pressed` messages. `logic.js` is the bun bundle; rebuild it after any change. |
| `fonts/tiny5.ttf` | Pico's default face, converted from `surfaces/pico/src/fonts/tiny5.woff2` with fontTools. Licence beside it. |
| `preview/` | Renders without the shell, and the Caliper reference. |

Input: Left and Right move the chosen cart (Pico's `MovedFocus` then
`SelectedCart`), Back is Pico's `PressedBack`. Confirm, Menu, Options, Up and
Down are not passed on: each opens a screen or layout this package does not
draw.

Motion, all stepped as in `pico-motion.css`: the chosen shelf cart's shadow
cycles red, orange, pink every 200 ms once the cursor is on the shelf
(`pico-cycle`), and the stage's cartridge drops 12 virtual pixels in 4 steps
over 240 ms when the choice changes (`pico-land`). The shelf scrolls only as
far as it must, with one cart of margin.

## Preview

```sh
bun preview/run-logic.ts --brief                       # view JSON: start, then after right, right
bun preview/make-wrapper.ts start 640 480 preview/gen/start.slint
slint-viewer preview/gen/start.slint --screenshot out.png --size 640x480   # Slint 1.18.1
MODEL=preview/gen/pico-fixture-model.json bun preview/make-wrapper.ts start 640 480 preview/gen/fx.slint
```

`preview/pico-fixture-model.ts` prints Pico's own fixture model, the data the
Caliper reference draws, so the two can be compared pixel for pixel.

## Known differences from Pico

- Cover art is shown as it arrives, not sampled to 64 cells and remapped to
  the sixteen (`use-pico-quantized-art.ts`). Doing that in QuickJS took 1.4 s
  on x86_64 for eight covers, too slow for a surface switch. The cart's shape
  does use the sampled canvas's ratio, so every cart has Pico's size.
- Wrapped titles use the face's line spacing (1.0), not Pico's 1.1.
- Pico's narrow-and-tall stage layout (art above facts) is not ported; neither
  640x480 nor 1280x720 uses it. A stage too short for art drops it, as Pico's does.
- Shelf slots have one width. Pico packs a cart whose art is narrower than
  the shelf box tighter; no cover in the fixture is that narrow.
