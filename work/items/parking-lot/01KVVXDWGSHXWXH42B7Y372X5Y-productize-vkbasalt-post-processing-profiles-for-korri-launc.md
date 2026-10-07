---
id: 01KVVXDWGSHXWXH42B7Y372X5Y
slug: productize-vkbasalt-post-processing-profiles-for-korri-launc
title: Productize vkBasalt post-processing profiles for Korri launches
origin: parked
status: To Do
priority: medium
labels:
  - korri
  - retroarch
  - vkbasalt
  - post-processing
  - sobo
created: 2026-06-24
source: user
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/parking-lot/01KVVXDWGSHXWXH42B7Y372X5Y-productize-vkbasalt-post-processing-profiles-for-korri-launc.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep the demonstrated Vulkan post-processing case. The current published output map contains no vkBasalt package. The original Sobo profiles are historical proof, not installed main defaults.

## Scope on main

Own the Vulkan layer and its native configuration in a plugin. Use prebuilt dependencies and preserve launch permission checks. The old suggested durable paths and profile shape remain historical until grounded in an actual producer.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

vkBasalt does not cover OpenGL games. Shaders add GPU cost, and a layer can conflict with a runtime or compositor.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Productize vkBasalt post-processing profiles for Korri launches

## Why it matters

The Sobo proof showed vkBasalt can layer obvious Vulkan post-processing and ReShade-style VHS effects over RetroArch launches, but it currently relies on ad-hoc store paths, hand-edited YAML profiles, and manually installed shader/config files. Productizing it would make whole-window effects reusable for RetroArch, native, Steam/Proton, and Gamescope-backed launches without fragile device-local setup.

## Acceptance Criteria

- [ ] A first-class launch/plugin setting enables vkBasalt without hand-editing environment variables.
- [ ] NixOS product modules can install `pkgs.vkbasalt` and optional curated shader/config assets into stable `/etc/korri` or `/var/lib/korri` paths.
- [ ] At least one curated profile exists for an obvious effect, e.g. active/corroded VHS, with documented expected visuals and performance caveats.
- [ ] Launch dry-runs expose the vkBasalt environment/config path so users can verify the selected profile before launch.
- [ ] Sobo smoke test proves a RetroArch Vulkan launch uses the productized vkBasalt profile; non-Vulkan/OpenGL limitations are documented.

## Related

- `product/plugins/retroarch/src/launch-spec.ts`
- `product/plugins/retroarch/src/policy.ts`
- `product/plugins/retroarch/nix/nixos-module.nix`
- `product/systems/nixos/flake/products.nix`
- `/var/lib/korri/vkbasalt/vkBasalt-active-vhs.conf on Sobo`
- `/var/lib/korri/reshade-shaders/Shaders/KorriActiveCorrodedVHS.fx on Sobo`

## Notes

Prototype profiles created on Sobo: `vkbasalt-neon-proof`, `vkbasalt-corroded-vhs`, and `vkbasalt-active-vhs`. Current demo uses nix store vkbasalt path `/nix/store/g7h3yqghmgfwrqbf0ql591yibalsvwz9-vkbasalt-0.3.2.10` in `XDG_DATA_DIRS`; productized version should avoid hardcoded store paths and support curated ReShade FX assets.
