---
id: 01M2HHPQXA8HKDRBQQB3XR5PXF
slug: deduplicate-r36t-max-ffmpeg-through-the-alsa-plugin-dependen
title: Deduplicate R36T Max FFmpeg through the ALSA plugin dependency
origin: parked
status: To Do
priority: low
labels:
  - r36tmax
  - image-size
  - audio
created: 2026-09-15
source: se-work
---

# Deduplicate R36T Max FFmpeg through the ALSA plugin dependency

## Why it matters

The measured R36T Max baseline closure includes both ffmpeg and ffmpeg-headless. A read-only dependency audit identified alsa-plugins as the external consumer of ordinary FFmpeg; PipeWire already uses headless. A package-scoped ALSA dependency override could remove about 28.9 MB of duplicate FFmpeg outputs, but savings and codec/audio parity are not yet proven. Keep this separate from the behavior-neutral registry/compression pass because speaker acceptance is still pending.

## Acceptance Criteria

- [ ] Verify the actual closure referrers and compute net removal rather than counting duplicate names.
- [ ] Use the existing alsa-plugins dependency seam without a global FFmpeg override.
- [ ] Verify ALSA plugin behavior, retained codec capabilities and owner-approved audio acceptance on the candidate.
- [ ] Measure actual closure and compressed image differences, preserving kernel, UCM, recovery and rollback policy.

## Related

- `nix/devices/r36tmax/audio`
- `nix/devices/r36tmax/IMAGE-REDUCTION.md`
- `nix/devices/r36tmax/default.nix`
- `flake.lock`
