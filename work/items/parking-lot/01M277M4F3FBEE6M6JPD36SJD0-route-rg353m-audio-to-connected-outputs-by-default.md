---
id: 01M277M4F3FBEE6M6JPD36SJD0
slug: route-rg353m-audio-to-connected-outputs-by-default
title: Route RG353M audio to connected outputs by default
origin: parked
status: In Progress
priority: medium
labels:
  - rg353m
  - audio
  - boot-validation
created: 2026-09-11
source: user
---

# Route RG353M audio to connected outputs by default

## Why it matters

The phase-1 candidate selects HDMI as its default PipeWire sink while the HDMI connector reports disconnected. The built-in speaker sink is available. Applications that follow the default can therefore produce no audible sound on the handheld. Direct speaker test playback completed, with listening confirmation still pending.

## Acceptance Criteria

- [ ] Reproduce the disconnected-HDMI default on the RG353M using native PipeWire and DRM state.
- [ ] Use native audio policy to select the built-in speakers when no external output is connected, while preserving headphones and HDMI routing.
- [ ] Verify audible playback, reconnect behavior, and routing after suspend/resume.

## Related

- `services/inputd/nix/korri-linux-host.nix`
- `nix/devices/rg353m/sunshine-host.nix`

## Notes

Observed default alsa_output.platform-hdmi-sound.stereo-fallback; /sys/class/drm/card1-HDMI-A-1/status=disconnected. Available speaker sink alsa_output.platform-sound.HiFi__Speaker__sink. No default-route change made during validation.
