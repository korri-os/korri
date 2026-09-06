---
id: 01M1W7E6TTWVS9ZYYCB6YQJ2JM
slug: make-pico-gameplay-ranges-adjustable-in-both-directions
title: Make Pico gameplay ranges adjustable in both directions
origin: parked
status: To Do
priority: medium
labels:[]
created: 2026-09-06
source: user
---

# Make Pico gameplay ranges adjustable in both directions

## Why it matters

PicoControlRow displays left/right chevrons, but the range view only sends a positive stepped value clamped at max. The Caliper adapter now exposes the real controller, so this is product control behavior rather than a fixture wiring defect. Users need to lower values such as volume as well as raise them.

## Acceptance Criteria

- [ ] Reproduce the inability to decrease a published range with a failing interaction test before changing code.
- [ ] Provide clear decrement/increment interactions that honor the host's min, max, step and disabled state.
- [ ] Verify both directions through a live placed Gameplay Overlay in Caliper without real backend operations.

## Related

- `surfaces/pico/src/pico-overlay-view.ts`
- `surfaces/pico/src/ui/molecules/PicoControlRow.tsx`
- `docs/acceptance/pico-caliper-2026-09-06.md`
