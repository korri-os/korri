---
id: 01M1N42XR11KT03RAAZYGJECXR
slug: remove-or-feed-shift-s-preview-only-battery-and-network-atom
title: "Remove or feed Shift's preview-only battery and network atoms"
origin: parked
status: To Do
priority: medium
labels:
  - surface
  - shift
  - dead-code
created: 2026-09-04
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: feat/pico-surface-first-slice
  repo: korri
  invoked_by: se-work
---

# Remove or feed Shift's preview-only battery and network atoms

## Why it matters

ShiftStatusBar accepts battery and network props, and ShiftBattery / ShiftNetworkIcon / shift-network-state exist to render them, but nothing passes them in production: `grep -n 'battery|network' surfaces/shift/src/ShiftSurface.tsx` returns nothing, and clients/portal never supplies them either. The treaty carries no battery or radio facts, so these can only ever be fed by fixtures. The result is a surface whose previews show indicators the device never shows — the design is reviewed against a screen that does not exist. Either the treaty gains real device facts (a Korri-side change with a real source), or the atoms and their props come out. Pico deliberately shipped without them for this reason, so the two surfaces now disagree about what a status bar is.

## Acceptance Criteria

- [ ] Either SurfaceModel publishes battery/network facts sourced from korrid and ShiftSurface passes them, or ShiftBattery, ShiftNetworkIcon, shift-network-state, and the ShiftStatusBar props are deleted
- [ ] No component in surfaces/shift renders a device reading that production never supplies
- [ ] shift-check passes

## Related

- `surfaces/shift/src/ui/molecules/ShiftStatusBar.tsx`
- `surfaces/shift/src/ui/atoms/ShiftBattery.tsx`
- `surfaces/shift/src/shift-network-state.ts`
- `surfaces/shift/src/ShiftSurface.tsx`
- `contracts/surface/korri-surface.ts`
