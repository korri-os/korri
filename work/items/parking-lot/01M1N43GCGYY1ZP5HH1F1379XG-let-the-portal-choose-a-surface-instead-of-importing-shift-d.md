---
id: 01M1N43GCGYY1ZP5HH1F1379XG
slug: let-the-portal-choose-a-surface-instead-of-importing-shift-d
title: Let the portal choose a surface instead of importing Shift directly
origin: parked
status: To Do
priority: medium
labels:
  - portal
  - surface
  - wiring
created: 2026-09-04
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: feat/pico-surface-first-slice
  repo: korri
  invoked_by: se-work
---

# Let the portal choose a surface instead of importing Shift directly

## Why it matters

Pico exists and passes its checks but nothing can mount it: clients/portal/src/surface/SurfaceRoot.tsx imports ShiftSurface directly and vite.config.ts aliases @korri/shift by path, so the surface is a compile-time constant. The treaty was written so a host could swap surfaces without knowing which UI framework built them — KorriSurface.mount exists precisely for this — and that promise is currently untested because there has only ever been one implementation. Pico is the second, which makes this the moment the seam is either real or fiction. Note that Pico renders nothing for the gameplay-overlay presentation, so surface selection must account for a surface that implements only part of the treaty.

## Acceptance Criteria

- [ ] The portal resolves which KorriSurface to mount at runtime rather than importing one component
- [ ] Both shiftSurface and picoSurface mount through the same path and are exercised by a test
- [ ] A surface that does not implement a presentation is handled explicitly rather than rendering blank
- [ ] portal-check passes

## Related

- `clients/portal/src/surface/SurfaceRoot.tsx`
- `clients/portal/vite.config.ts`
- `surfaces/pico/src/mount.tsx`
- `surfaces/shift/src/mount.tsx`
- `contracts/surface/korri-surface.ts`
