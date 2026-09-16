---
id: 01M24NY2SBJTMTFVDKPJ65M8QK
slug: prove-owner-approved-ssh-as-a-plugin-on-ssh-disabled-images
title: Prove owner-approved SSH as a plugin on SSH-disabled images
origin: parked
status: To Do
priority: high
labels:
  - plugins
  - ssh
  - release-security
created: 2026-09-10
source: user
---

# Prove owner-approved SSH as a plugin on SSH-disabled images

## Why it matters

The user accepts initial public RG353M images with SSH disabled only if owner-controlled SSH can later be provided through the new plugin architecture. The active plugin-standard implementation already packages native services and declares firewall ports, but its hardening forces DynamicUser, permits only CAP_NET_ADMIN/CAP_NET_RAW, and offers a root-only installation CLI. These constraints do not yet support ordinary administrative OpenSSH or owner installation without prior administrator access. Shipping the older host cannot promise plugin-only SSH enablement without a later host update.

## Acceptance Criteria

- [ ] An SSH-disabled image contains no personal authorized key, passwordless root console, or shared default administrator password.
- [ ] A local owner can inspect, approve, install, and enable the SSH plugin without needing an existing SSH session or passwordless root console.
- [ ] The native-service and permission contracts support the chosen SSH implementation through explicit owner approval without bypassing host policy for arbitrary plugins.
- [ ] Each device creates its own SSH host keys and accepts only keys supplied by its owner; no private key or personal credential is baked into the plugin or image.
- [ ] Enablement opens only the declared SSH port; disablement, removal, failed activation, and reboot recovery preserve the approved access state.
- [ ] An integration test proves remote login, refusal of an unapproved key, disablement, and reboot behavior without a NixOS rebuild after a compatible plugin host is installed.

## Related

- `docs/briefs/2026-09-09-plugin-authoring-standard-brief.md`
- `services/korrid/plugin-host/README.md`
- `services/korrid/plugin-host/src/unit.rs`
- `services/korrid/plugin-host/src/main.rs`
- `nix/base/default.nix`
- `session:01a07ca2-ba3e-704f-8d63-aa55f9cba2e1`

## Notes

Read-only inspection of the active .worktree/plugin-standard code found services/korrid/plugin-host/src/native_unit.rs restricts capabilities to two network capabilities and unit.rs forces DynamicUser=yes, NoNewPrivileges=yes, ProtectHome=yes and closed device access. Native systemd files and ports are build-side supported, but an administrative SSH service is not verified. Do not edit the concurrent session's worktree or invent an SSH schema. Ground any permission extension in the actual SSH package and native unit. Existing Wi-Fi config already uses placeholders; this requirement concerns administrator access, not restoring Wi-Fi credentials.
