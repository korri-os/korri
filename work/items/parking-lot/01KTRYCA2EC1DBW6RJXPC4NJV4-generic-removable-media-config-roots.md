---
id: 01KTRYCA2EC1DBW6RJXPC4NJV4
slug: design-generic-removable-media-korri-config-roots
title: Design generic removable-media Korri config roots
origin: parked
status: To Do
priority: medium
labels:
  - config
  - removable-media
  - nixos
  - follow-up
created: 2026-06-10
source: se-challenge-plan
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/active/01KTRYCA2EC1DBW6RJXPC4NJV4-generic-removable-media-config-roots/item.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep USB and SD config portability as future work. The current snapshot producer reads three fixed documents from one root in services/korrid/src/config/snapshot.rs. Game-folder selection is not removable config-root support.

## Scope on main

First resolve trust and authoring destinations against real current records. Do not restore the old config graph, root names, or permission escalation by copying this ticket.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

Mounted media must not gain launch authority automatically. Hotplug and read-only media need separate acceptance.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Design generic removable-media Korri config roots

## Why it matters

Korri config should not be tied to SM8550 SD-card paths; future devices need USB drives and other removable media to expose config fragments through a shared, device-neutral convention without UUID/label assumptions.

## Acceptance Criteria

- [ ] Define a device-neutral removable media exposure contract under Korri-owned paths.
- [ ] Support multiple removable devices and media types, including USB drives and SD cards.
- [ ] Document how mounted media contributes Korri config roots without device-specific hardcoding.
- [ ] Add Nix/module checks for at least one non-SM8550 provider shape or a generic provider interface.
- [ ] Validate hotplug add/remove behavior and config event broadcasts on a real device or representative VM.

## Related

- `product/systems/nixos/images/platforms/rocknix-sm8550.nix`
- `product/systems/nixos/modules`
