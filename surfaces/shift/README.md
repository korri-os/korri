# Shift

Korri's cinematic handheld surface: a full-screen scene where the focused game's
art becomes the environment and a spring-driven rail keeps that game centred.

Shift is a **surface**, not a theme file. It owns its own layout, motion,
components, and stylesheet, and it is designed to be one of several — and
eventually to live in its own repository.

## The boundary

Shift depends on exactly one thing from Korri: the treaty in
`contracts/surface/korri-surface.ts`, imported **for types only**. There is no
runtime dependency in either direction.

```
Local korrid (Rust)
              ↓
      portal host adapter
              ↓
    contracts/surface  ← the whole contract
              ↓
            Shift
```

What that forbids, deliberately:

- No korrid client, no generated Rust types, no Linux shell binding.
- No host state, routing, or service calls.
- No interpreting failure codes: the host hands Shift finished, user-facing copy.

What Shift owns:

- Every pixel — layout, tokens, motion, and `src/shift.css`.
- Which parts of the model it chooses to present.
- How absent data is presented (a title monogram stands in for missing cover art).

## Using it

A React host renders the component:

```tsx
import { ShiftSurface } from "@korri/shift"

<ShiftSurface model={model} host={host} />
```

Any other host mounts it through the treaty:

```ts
import { shiftSurface } from "@korri/shift"

const instance = shiftSurface.mount(container, model, host)
instance.update(nextModel)
instance.unmount()
```

## Today's dependencies

`framer-motion` and `lucide-react` carry the motion and iconography Shift was
designed around; `@korri/intrinsic-design` provides the container-adaptive token
generator its stylesheet instantiates. They are declared here because they are
Shift's, even though the host currently installs them — Shift is compiled from
source by whichever host bundles it, which is what makes extraction later a
packaging change rather than a rewrite.

## Not here yet

Shift's fuller design (library browsing, search, game detail, store, favorites,
collections) exists in this repository's `legacy` branch. Those screens are not
ported because Korri cannot yet supply the data behind them. They come across
when the capability does, not before.
