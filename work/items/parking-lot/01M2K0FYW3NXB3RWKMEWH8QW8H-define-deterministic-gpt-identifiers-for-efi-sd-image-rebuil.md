---
id: 01M2K0FYW3NXB3RWKMEWH8QW8H
slug: define-deterministic-gpt-identifiers-for-efi-sd-image-rebuil
title: Define deterministic GPT identifiers for EFI SD image rebuilds
origin: parked
status: To Do
priority: low
labels:
  - nixos
  - reproducibility
  - images
created: 2026-09-15
source: se-code-review
---

# Define deterministic GPT identifiers for EFI SD image rebuilds

## Why it matters

The EFI image producer inherited from the Odin path runs sgdisk --mbrtogpt without setting disk or partition GUIDs. The real tool generates different identifiers from identical MBR input, so rebuilds of a pinned image need not have the same checksum. Source-pinned builds and exact-artifact checks remain useful, but byte reproducibility requires a deliberately grounded identity policy and independent rebuild comparison.

## Acceptance Criteria

- [ ] Define and document a deterministic GPT identity policy grounded in existing native image inputs, including the duplicate-card cost.
- [ ] Two independent assemblies of the same source and inputs produce matching GPT metadata.
- [ ] Compare complete image bytes from independent rebuilds; record or remove any remaining non-GPT nondeterminism.
- [ ] Existing EFI image verification and safe SD-only behavior remain intact.

## Related

- `nix/devices/rpminiv2/sd-image.nix`
- `nix/devices/rpminiv2/README.md`
- `nix/devices/odin2portal/sd-image.nix`
- `nix/formats/image-dist.py`
