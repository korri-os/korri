---
id: 01M1VRN166MX8E60PFGPNDAWYD
slug: complete-pico-s-caliper-state-and-visual-acceptance
title: "Complete Pico's Caliper state and visual acceptance"
origin: parked
status: To Do
priority: medium
labels:[]
created: 2026-09-06
source: user
---

# Complete Pico's Caliper state and visual acceptance

## Why it matters

The first adapter slice discovers and safely places all current parts, routes scoped inputs and simulates host consequences, but that integration coverage does not prove every supported state is directly reviewable or every part is visually correct at each physical size.

## Acceptance Criteria

- [ ] Expose remaining treaty-backed review states, including catalog error, running, settings saving/failure and overlay failure, using shared fixtures rather than a duplicate component manifest.
- [ ] Make the supported scope of placed-page source/interaction controls explicit; implement real fixture-driven page bindings where required instead of advertising inert selectors.
- [ ] Complete per-part visual and accessibility review at RG353M, THOR and Odin 2 Portal physical sizes, including attract wake and location selection.
- [ ] Keep kiosk deployment and real korrid/native operations out of the fixture acceptance workflow.

## Related

- `surfaces/pico/caliper/adapter.ts`
- `surfaces/pico/caliper/verify-browser.mjs`
- `surfaces/pico/README.md`
