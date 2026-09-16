---
id: 01M2GS38QWXK9HM7ENCMAW6GXG
slug: reduce-sustained-browser-load-and-heat-on-the-r36t-max
title: Reduce sustained browser load and heat on the R36T Max
origin: parked
status: To Do
priority: high
labels:
  - r36tmax
  - performance
  - portal
created: 2026-09-14
source: se-debug
---

# Reduce sustained browser load and heat on the R36T Max

## Why it matters

The existing Pico error screen consumed 17.25 CPU-seconds in 10.21 seconds and held die sensors near 85°C with thermal throttling. After the owner approved pausing Chromium, readings fell to about 65°C in two minutes and later about 56°C. The owner has paused application work, so preserve this as a separate performance investigation rather than changing the UI during hardware testing.

## Acceptance Criteria

- [ ] Reproduce and profile the browser workload on the handheld, with fixed power, placement, panel mode and governor.
- [ ] Identify the actual hot paths rather than assuming that animation, GPU fallback or the library error is the cause.
- [ ] Demonstrate lower sustained CPU/GPU work and no sustained idle-screen thermal throttling under the same conditions.
- [ ] Preserve browser sandboxing, functional controls and hardware rendering; record memory, temperatures and restart counts before and after.

## Related

- `nix/devices/r36tmax/HARDWARE-SURVEY-2026-09-14.md`
- `clients/portal`
- `surfaces/pico`
