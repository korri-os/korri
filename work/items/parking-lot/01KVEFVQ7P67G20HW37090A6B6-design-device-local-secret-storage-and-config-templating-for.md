---
id: 01KVEFVQ7P67G20HW37090A6B6
slug: design-device-local-secret-storage-and-config-templating-for
title: Design device-local secret storage and config templating for provider credentials
origin: parked
status: To Do
priority: high
labels:
  - credentials
  - config
  - secrets
  - itchio
  - device-ux
created: 2026-06-18
source: user
context:
  cwd: .worktrees/feat/itchio-public-provider
  branch: feat/itchio-public-provider
  commit: 7768feca
  repo: simonwjackson/korri
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/parking-lot/01KVEFVQ7P67G20HW37090A6B6-design-device-local-secret-storage-and-config-templating-for.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep provider credential UX as future design work. Current services/korrid/src/config/settings.rs implements private, write-only SteamGridDB credentials. That does not settle secret delivery for an itch.io provider or other store plugins.

## Scope on main

Start with the existing SteamGridDB producer and settings_secrets_tests.rs. Compare the options against a real store login. Do not replace the current private store or select a new secret schema during this import.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

Encryption adds key recovery work. Plain private files rely on local filesystem protection. Either approach still needs log and build-artifact leak checks.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Design device-local secret storage and config templating for provider credentials

## Why it matters

The itch.io provider currently uses environment variables for API keys, which is acceptable for validation but not the desired long-term device UX. The user wants a future discussion about config templates plus a simple Unix-style password storage tool on-device.

## Acceptance Criteria

- [ ] Evaluate config templating patterns for provider credentials without committing secrets to config files or Nix/build artifacts.
- [ ] Compare simple Unixy secret storage options suitable for Korri devices, including file permissions, pass-like stores, age/sops-style encryption, keyring availability, and handheld constraints.
- [ ] Define how provider plugins receive secrets at runtime while preserving redaction and avoiding shell-history/log leakage.
- [ ] Document a recommended credential UX for itch.io and other store providers.

## Related

- `product/platform/acquisition/plugin-runtime.ts`
- `product/platform/acquisition/plugins/itchio.ts`
- `docs/acceptance/itchio-public-provider.md`
