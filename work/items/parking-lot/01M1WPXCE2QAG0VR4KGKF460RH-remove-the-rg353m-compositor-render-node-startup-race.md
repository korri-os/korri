---
id: 01M1WPXCE2QAG0VR4KGKF460RH
slug: remove-the-rg353m-compositor-render-node-startup-race
title: Remove the RG353M compositor render-node startup race
origin: parked
status: To Do
priority: medium
labels:
  - rg353m
  - compositor
  - boot
created: 2026-09-07
source: se-work
---

# Remove the RG353M compositor render-node startup race

## Why it matters

During persistent portal acceptance, Sway's first boot invocation could not open /dev/dri/renderD128 (Permission denied), failed its Wayland-socket readiness check, then succeeded on its next start. The portal now follows parent recovery, but the underlying compositor failure still delays boot and could affect other consumers. A permission-timing race is inferred, not yet proven; do not broaden device permissions as a workaround.

## Acceptance Criteria

- [ ] Reproduce and explain the first-start renderD128 permission failure from actual boot logs and device-node ownership.
- [ ] Verify Sway starts successfully on repeated boots without relaxing the compositor input allowlist or adding broad device access.
- [ ] Keep the working kernel, RKVENC, zero-copy Sunshine, audio and SD boot configuration intact.

## Related

- `services/inputd/nix/korri-linux-host.nix`
- `nix/rg353m/sunshine-host.nix`
- `clients/portal/nix/nixos-module.nix`
