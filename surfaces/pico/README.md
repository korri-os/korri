# Pico

Korri's fantasy-console surface: PICO-8's sixteen colours on black, Zep's
PICO-8 glyphs, and a shelf of cartridges that take the shape of their art,
with the chosen one large on a stage above them.

Pico is a **surface**, not a theme file. It owns its own layout, components, and
stylesheet, and it is designed to be one of several — and eventually to live in
its own repository.

## The boundary

Pico depends on exactly one thing from Korri: the treaty in
`contracts/surface/korri-surface.ts`, imported **for types only**. Its only
other import is `qrcode`, a pure encoder for the identity backup. A gate
enforces both; there is no runtime dependency in either direction.

What that forbids, deliberately:

- No Effect, no atoms, no router, no `@platform` module, no other surface.
- No device facts Korri does not publish. Pico shows **no battery and no signal
  meter**, because the treaty states neither, and a plausible-looking invented
  battery is worse than none.
- No interpreting failure codes: Korri hands Pico finished, user-facing copy.

## The gates are the specification

Two test files hold the rules that prose cannot enforce. Read them before adding
a component; they will find what review misses.

- `test/decomposition-gate.test.ts` — every rendered unit is a component with a
  part beside it, at every layer; one component per file; no `className` literal
  or class selector defined twice.
- `test/authoring-gate.test.ts` — every part default-exports a function and a
  name; every atom, molecule, and organism part roots in **one imported
  component** so it emits real Inspector controls; no raw colour or pixel value
  outside `src/pico-tokens.css`; no colour blend of any kind (no `color-mix`,
  fractional opacity or smooth gradient); the virtual pixel is registered and
  derived from both axes; no inline styles; no forbidden import; no
  design-part registry and no story files.

Every assertion has been observed failing against a deliberate tripwire. If you
add one, break it once before you trust it.

```sh
nix run .#pico-check      # gates, behaviour tests, and typecheck
```

## Layout

```
src/
  pico-tokens.css     the only file allowed a raw colour or pixel value; the
                      palette, the virtual pixel and every role
  pico-motion.css     stepped keyframes only; nothing in Pico eases
  pico.css            entry: tokens, motion, one stylesheet per component
  fonts/              pico8-glyphs.txt (the design) and pico8.woff2 (built)
  fixtures/           the fixture host, and sample-art.ts (generated covers)
  PicoSurface.tsx     the composition root — the only file that reads the treaty
  pico-screen-view.ts what the screen shows, decided once: status outranks catalog
  pico-home-view.ts   catalog -> the home's own tagged state, converted once
  ui/atoms|molecules|organisms|templates
  pages/
```

Each component carries its own `<Name>.css` and `<Name>.<layer>.part.tsx`
beside it. A class name is prefixed with the component that owns it, which is
what makes a duplicated visual decision a build failure rather than a slow
drift.

`scripts/` holds the two generators. Both are deterministic Nix-shebang
scripts, and their output is committed:

```sh
./scripts/build-font.py         # src/fonts/pico8.woff2 from pico8-glyphs.txt
./scripts/draw-sample-art.py    # src/fixtures/sample-art.ts, original covers
```

## The look

- **One virtual pixel.** `--pico-px` is the screen's short side divided by
  360, in whole device pixels, never below 2. Every size is a multiple of it,
  and type is a whole number of glyph pixels, so the face never blurs.
- **Sixteen colours, nothing between them.** Under a question the screen goes
  solid; focus is a hard shadow that colour-cycles, the way PICO-8's cursor
  does.
- **Carts take the shape of their art.** Covers are remapped to the palette at
  their own ratio, about 64 × 64 palette pixels by area, and never cropped. A
  game with no art gets a square sticker with its initials.
- **Cards say what they are by colour.** Yellow asks, red warns, blue tells.
- **Layout answers the container, not the device.** The body, the stage, the
  shelf and the launch slot are size containers; thresholds are in em of the
  small face, so they scale with the pixel. The ladder checked is 1920×1080,
  1280×720, 640×480, 480×800, 1280×300 and 320×240. Every threshold is a
  guess until measured on the real panels.

## Supported UI

Pico renders the catalog (shelf/grid/hero), game detail and launch locations,
Find, grouped settings, gameplay overlay and attract mode. The portal selects
it with `?surface=pico`. Settings text editing remains unavailable.

## Caliper integration

Choose `surfaces/pico` in Caliper. Restart the launcher after changing
`caliper.project.mjs`: reactivating an existing session is not a reliable
configuration reload.

- `project-entry.ts` exports the adapter, CSS and discovered parts.
- `../pico-caliper-parts.ts` discovers only Pico's `.part.tsx` files. It lives
  above `pico/` because Caliper derives surface identity and hot-update paths
  relative to that bridge. There is no duplicate component manifest.
- `caliper/adapter.ts` mounts independent fixture sessions at the RG353M, THOR
  and Odin 2 Portal panel sizes. It never contacts korrid or the native bridge.
- `caliper/render-part.ts` preserves the authored root's props, then overlays
  only Inspector-editable inputs. This avoids Caliper dropping required arrays,
  models, children and callbacks when constructing a placed part. Part wrappers
  must remain **pure element factories**; hooks belong in their returned product
  component. Every part is tested outside a React render to enforce this.
- `caliper/preview.css` supplies definite gallery frames and lets inherited
  design controls reach Pico's registered properties without changing its
  runtime stylesheet. Caliper currently exposes numeric/percentage controls;
  its color and length control support is not supplied by this adapter.

### Using live fixture devices and placed pages

Focus/click inside a device, then use **F** for Find, **M** to cycle shelf/grid/
hero, **S** for settings, and **Escape** for Back. Tab and Enter keep native
browser focus/activation; this adapter does not implement gamepad or spatial
arrow navigation. Shortcuts ignore editable fields, repeats and modifiers.

The device Inspector also exposes **Pico input** events and a **Fixture
scenario** event, scoped to the selected device or placed page. Eleven sources
cover ready, loading, empty, catalog error, busy, running, launch problem,
settings saving/failure, and gameplay overlay/overlay failure. The source picker
is derived from the review models; it is not another component/state manifest.

All five placed pages use the same production controller as devices: Home,
Find, Settings, Game Detail and Gameplay Overlay. Source changes reseed the
model without replacing local navigation. Explicit Inspector edits to the
entry view (such as Home layout) restart navigation. Detail, launch-location
selection and confirmations are reached through real controls. Back from a
Find result restores the query; the visible launch failure takes Back before
any page hidden underneath.

Launch/game actions remain in an explicitly labelled PREVIEW busy state rather
than claiming a real game started. Setting choices and overlay toggle/choice/
range controls republish values; overlay commands acknowledge simulated
requests. Retry, dismiss and reload have fixture consequences. No disk/network
operations occur. Smaller parts retain their authored fixture data/callbacks
and expose only generated Inspector inputs, not inert scenario/input events.
A source selector is not a universal smaller-part model editor.

### Verification

```sh
nix run .#pico-check
cd surfaces/pico
CALIPER_ROOT=/path/to/caliper bun run caliper:typecheck
CHROMIUM=/path/to/system/chromium VERIFY_HMR=1 \
  PHYSICAL_REVIEW_DIR=/tmp/pico-review bun run caliper:verify
```

`caliper:typecheck` first rejects an incomplete-adapter tripwire, then checks
Pico against that launcher's actual TypeScript contract. No Caliper import enters
runtime code. The browser check requires a running launcher (default
`http://127.0.0.1:3131`, override with `CALIPER_URL`) and the project registered in
its picker. It exercises all discovered part placements, preview bounds, scoped
navigation, live page sources/interactions, RG353M title visibility, confirmation
focus/Tab/Escape/restore, placed-part resizing, an Inspector prop edit and a
live design knob. It also checks attract's repeat geometry, stepped/reduced
motion, and real pointer wake without accidentally opening a game.
`VERIFY_HMR=1` adds/edits/removes temporary parts and a live page without reloading.
`PHYSICAL_REVIEW_DIR` optionally captures every discovered part at all three
panel sizes and checks names/bounds of enabled controls after focusing each.
It writes images and `report.json`; inspect the images, not just the totals.

Use a development workspace: the browser check changes the Caliper selection/
workspace and Caliper may persist those changes to `.lab/pico/state.json`.
The 2026-09-06 review inspected all 147 captures and fixed keyboard/title clipping,
attract looping, navigation precedence and confirmation focus; see
[`docs/acceptance/pico-caliper-2026-09-06.md`](../../docs/acceptance/pico-caliper-2026-09-06.md).
The 2026-09-25 run after the C2 rebuild passed with 52 parts, 156 captures and
no focus-bound cases. The first run after a source change can time out while
Caliper reloads the edited modules; run it again before reading the failure.
This is a browser simulation at configured physical sizes, not calibrated
on-device readability or assistive-technology certification. Gamepad/spatial
navigation, hardware and persistent kiosk deployment remain outside this check.
