# Pico

Korri's 8-bit handheld surface: a PICO-8 palette, a bitmap face, and a shelf of
cartridges where the focused game is held in the middle.

Pico is a **surface**, not a theme file. It owns its own layout, components, and
stylesheet, and it is designed to be one of several — and eventually to live in
its own repository.

## The boundary

Pico depends on exactly one thing from Korri: the treaty in
`contracts/surface/korri-surface.ts`, imported **for types only**, plus the
shared token maths in `@korri/intrinsic-design`. A gate enforces it; there is no
runtime dependency in either direction.

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
  outside `src/pico-tokens.css`; no inline styles; no forbidden import; no
  design-part registry and no story files.

Every assertion has been observed failing against a deliberate tripwire. If you
add one, break it once before you trust it.

```sh
nix run .#pico-check      # gates, behaviour tests, and typecheck
```

## Layout

```
src/
  pico-tokens.css     the only file allowed a raw colour or pixel value
  pico.css            entry: recipe, tokens, one stylesheet per component
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

### Using a live device preview

Focus/click inside a device, then use **F** for Find, **M** to cycle shelf/grid/
hero, **S** for settings, and **Escape** for Back. Tab and Enter keep native
browser focus/activation; this adapter does not implement gamepad or spatial
arrow navigation. Shortcuts ignore editable fields, repeats and modifiers.

The device Inspector also exposes **Pico input** events and a **Fixture
scenario** event, scoped to the selected device. Fixture sources cover ready,
loading, empty, busy, problem and gameplay overlay. Scenario changes reset the
model, not Pico's local navigation state. Detail, launch-location selection,
confirmations and settings are reached through their actual controls.

Launch/game actions remain in an explicitly labelled PREVIEW busy state rather
than claiming a real game started. Setting choices and overlay toggle/choice/
range controls republish values; overlay commands acknowledge simulated
requests. Retry, dismiss and reload have fixture consequences. No disk/network
operations occur. Individual placed parts retain their authored fixture data
and callbacks; selecting a device source is not a universal part-model editor.

### Verification

```sh
nix run .#pico-check
cd surfaces/pico
CALIPER_ROOT=/path/to/caliper bun run caliper:typecheck
CHROMIUM=/path/to/system/chromium VERIFY_HMR=1 bun run caliper:verify
```

`caliper:typecheck` first rejects an incomplete-adapter tripwire, then checks
Pico against that launcher's actual TypeScript contract. No Caliper import enters
runtime code. The browser check requires a running launcher (default
`http://127.0.0.1:3131`, override with `CALIPER_URL`) and the project registered in
its picker. It exercises all discovered part placements, preview bounds, scoped
navigation, RG353M placed-part resizing, an Inspector prop edit and a live
design knob. `VERIFY_HMR=1` adds,
edits and removes a temporary part, asserting that the session does not reload.

Use a development workspace: the browser check changes the Caliper selection/
workspace and Caliper may persist those changes to `.lab/pico/state.json`.
This is integration coverage, not a completed per-part visual/accessibility
review at every physical size. Hardware and persistent kiosk deployment are
outside this check.
