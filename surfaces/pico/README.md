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
  pico-home-view.ts   catalog -> the home's own tagged state, converted once
  ui/atoms|molecules|organisms|templates
  pages/
```

Each component carries its own `<Name>.css` and `<Name>.<layer>.part.tsx`
beside it. A class name is prefixed with the component that owns it, which is
what makes a duplicated visual decision a build failure rather than a slow
drift.

## What this slice does not do yet

Stated plainly so nobody mistakes absence for completeness:

- **No gameplay overlay.** `PicoSurface` renders nothing for that presentation
  rather than drawing the library over a running game.
- **No game detail, settings, or search.** Home only.
- **No portal wiring.** `clients/portal` still mounts Shift directly; switching
  surfaces is a portal change, not a surface one.
- **Back is not wired.** The button bar names it; nothing subscribes to the
  treaty's semantic input yet.
