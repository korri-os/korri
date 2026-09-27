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

Three test files hold the rules that prose cannot enforce. Read them before adding
a component; they will find what review misses.

- `test/decomposition-gate.test.ts` — every rendered unit is a component with a
  part beside it, at every layer; one component per file; no `className` literal
  or class selector defined twice. Direct runtime imports can point down or stay
  within a layer, never up. Part suffixes must match their component's directory.
  Type-only imports do not compose UI and are excluded from the direction check.
- `test/authoring-gate.test.ts` — every part default-exports a function and a
  name; every atom, molecule, and organism part roots in **one imported
  component** so it emits real Inspector controls; no raw colour or pixel value
  outside `src/pico-tokens.css`; no colour blend of any kind (no `color-mix`,
  fractional opacity or smooth gradient); the virtual pixel is registered and
  derived from both axes; no inline styles; no forbidden import; no
  design-part registry and no story files.

- `test/css-ownership-gate.test.ts` checks that components import their own CSS,
  shared motion has explicit consumers, and global CSS stays limited to screen
  scope. It rejects CSS import chains, foreign classes and wildcard child
  selectors. Its ownership checks cover static class names, not every possible
  selector match.

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
  pico.css            global font, reset, screen scope and reduced motion
  PicoSurface.css     catalog-surface rules owned by the composition root
  fonts/              pico8-glyphs.txt (the design) and pico8.woff2 (built)
  fixtures/           the fixture host, and sample-art.ts (generated covers)
  PicoSurface.tsx     the composition root — the only file that reads the treaty
  pico-screen-view.ts what the screen shows, decided once: status outranks catalog
  pico-home-view.ts   catalog -> the home's own tagged state, converted once
  ui/atoms|molecules|organisms|templates
  pages/
```

Templates arrange slots without supplying the final content. Pages fill those
slots with facts and actions. `PicoRunnerPicker` fills `PicoGameOverlay` with
runner choices. `PicoSettingsPanel` fills `PicoPanelScreen` with settings groups;
`PicoSettings` adds the outer shell and action handling. Both content components
therefore live in `pages/`, not `ui/organisms/`. Same-layer composition is allowed.
Their Caliper parts now use `src/pages/<Name>.page.part.tsx`; saved links to the
old organism part paths must be updated.

Each component imports its sibling `<Name>.css` from its `.tsx` file. It carries
its `<Name>.<layer>.part.tsx` beside it. A stylesheet styles only elements its
component renders. It must not target another component's classes or arbitrary
children. Class names identify their owner.

Share values through tokens and structure through components. A component that
uses shared keyframes imports `pico-motion.css` itself. Do not use CSS `@import`:
Vite can read those files outside Caliper's take overlay. Explicit JS/TS imports
keep stylesheet dependencies visible and let takes replace those files.

`src/index.ts` imports tokens and global screen CSS. Import the public package
entry to load the complete surface. The `./pico.css` export contains only the
font, reset and screen scope; it does not load tokens or component styles alone.

Parents can set documented custom properties on their own elements. For example,
Notice and LocationPicker set `--pico-card-width`; Card owns the width rule.
`--pico-title-xl` lets a short container reduce large titles. Layout-neutral
content elements own that inherited setting inside size containers. They do not
style the children directly. Shared styles stay shared; repeated declarations
such as `display: grid` need no abstraction.

Import order now follows component dependencies. Verify browser output when
changing ownership, since a hidden cascade dependency can change the result.

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

## Caliper

Caliper shows Pico's parts at true device size. It is a dev-only Vite plugin,
and `vite.config.ts` is its only wiring. The portal builds Pico with its own
Vite config, so this file never reaches a device.

Register a Caliper checkout once per machine, then run it from here:

```sh
(cd /path/to/caliper && bun install && bun link)
cd surfaces/pico
bun install
bun run caliper   # then open http://localhost:5173/__caliper/
```

Caliper finds everything else from the source: the 52 `*.part.tsx` files, the
entry (`package.json` `exports["."]`), stylesheets and the wrapper
(`<div className="pico-theme pico-screen">` in `src/PicoSurface.tsx`). Its Setup
panel shows where each came from.

Caliper injects the two global stylesheets imported by `src/index.ts`. Each
part loads its component styles through its own imports. Badge loads three
stylesheets and Button loads four, including shared motion. Unrelated component
styles no longer hide a missing import. Caliper's `css` option can replace the
global list for projects that keep their global imports elsewhere.

Without a linked Caliper, `bun install` reports one package it cannot install.
The portal's production install skips dev dependencies and is not affected.

### Named states

Each part keeps its default example. Additional uppercase function exports show
meaningful alternate inputs of the same real component. Caliper lists these
states under the part; **All states** shows them together. Fixtures use the
existing surface treaty and component props, with no backend or request swapping.

The audit covers all 52 parts. `PicoPaletteBar` and `PicoPixelDisc` have no inputs,
so their default examples are their only states. A second export would duplicate
the same output. Other parts include alternate inputs such as missing artwork,
long content, disabled actions, loading, errors, and empty collections where
those cases belong to the component's contract.

`test/named-states.test.tsx` discovers exported states and mounts them with no
props. Focused state tests assert their content and behavior. Some components
return nothing for valid inputs; an `Empty` preview for those documented cases
is not a loading failure. Internal interaction states still require interaction
with the real component. These examples are not an exhaustive combination of
all props and do not prove that every layout fits every size.

`fixtures/PicoPartFrame.tsx` supplies the documented art-size properties and
space for the hero cart's lift in two isolated examples. It changes only the
preview container. The [verification record](../../docs/acceptance/pico-named-states-2026-09-27.md)
lists intentional empty states, browser checks, and limits.

To review all states of one part at both configured device sizes:

```sh
caliper-render --url http://localhost:5173 \
  --part src/pages/PicoHome.page.part.tsx --state '*' --device '*'
```

Set `CHROMIUM` to the Chromium executable. Read the verdict and browser errors,
then inspect the PNGs and any reported overflow. A successful mount alone does
not prove that the content is visible.

## Verification

```sh
nix run .#pico-check
```

The Caliper browser check lives in the Caliper repository
(`scripts/verify-browser.mjs`). Earlier Caliper review records are in
[`docs/acceptance/pico-caliper-2026-09-06.md`](../../docs/acceptance/pico-caliper-2026-09-06.md);
they describe the old launcher and adapter, which are gone.
