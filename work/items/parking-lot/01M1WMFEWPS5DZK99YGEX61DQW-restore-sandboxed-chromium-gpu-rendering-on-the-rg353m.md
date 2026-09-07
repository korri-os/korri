---
id: 01M1WMFEWPS5DZK99YGEX61DQW
slug: restore-sandboxed-chromium-gpu-rendering-on-the-rg353m
title: Restore sandboxed Chromium GPU rendering on the RG353M
origin: parked
status: To Do
priority: medium
labels:
  - rg353m
  - chromium
  - performance
created: 2026-09-07
source: se-work
---

# Restore sandboxed Chromium GPU rendering on the RG353M

## Why it matters

The pinned Chromium 143 GPU subprocess repeatedly crashes on seccomp syscall 0x77 on RG353M, including with the benchmark's explicit angle/gles-egl flags. During portal update/rollback acceptance the browser reached 68.125°C and its thermal guard stopped it. The persistent fixture preview uses --disable-gpu and reduced motion instead, preserving the sandbox and hardware-encoded Sunshine streaming. Larger or more animated surfaces will need verified browser acceleration rather than assuming the GPU path works.

## Acceptance Criteria

- [ ] Reproduce and explain the GPU subprocess seccomp failure using the actual RG353M browser.
- [ ] Restore accelerated browser rendering without --no-sandbox or --disable-gpu-sandbox.
- [ ] Verify the actual renderer, zero GPU crash loops, pointer/keyboard interactions, and sustained temperatures on both Pico and Shift.
- [ ] Keep Sunshine's existing hardware H.264 and zero-copy path unchanged.

## Related

- `nix/rg353m/portal-preview.nix`
- `nix/rg353m/browser-bench.nix`
- `clients/portal/DEPLOYMENT.md`
