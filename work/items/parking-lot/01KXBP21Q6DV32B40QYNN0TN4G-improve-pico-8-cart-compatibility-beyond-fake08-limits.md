---
id: 01KXBP21Q6DV32B40QYNN0TN4G
slug: improve-pico-8-cart-compatibility-beyond-fake08-limits
title: Improve PICO-8 cart compatibility beyond fake08 limits
origin: parked
status: To Do
priority: medium
labels:
  - pico8
  - bandai
  - emulation
  - store
created: 2026-07-12
source: se-debug
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/parking-lot/01KXBP21Q6DV32B40QYNN0TN4G-improve-pico-8-cart-compatibility-beyond-fake08-limits.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep cart compatibility as a current product concern. nix/product/PUBLISHED-PLUGINS.md records Into Ruins as a known FAKE-08 failure, and plugin-selection.nix selects FAKE-08 on every product device. The legacy Dinky Kong case provides a second reproducible input to investigate.

## Scope on main

Reproduce with the current published runtime before choosing upstream fixes, compatibility reporting, or an operator-installed official runtime. No proprietary runtime is approved for bundling. Preserve the observed cart identities.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

The official runtime requires a paid license. Upstream fixes cannot guarantee every cart, and compatibility gating needs honest evidence rather than filename guesses.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Improve PICO-8 cart compatibility beyond fake08 limits

## Why it matters

Store-acquired PICO-8 carts launch through @korri:pico8/fake08 (a reimplementation with incomplete PICO-8 API coverage). Verified differential on Bandai: Celeste (cart 15133) renders fine while Dinky Kong (dinkykong-0.p8.png, a valid 160x205 cart) shows a black screen under the identical gamescope->retroarch->fake08 stack. Modern carts using newer PICO-8 APIs (tline, custom fonts, extended memory pokes) silently black-screen, making the Store->Play promise unreliable for a large share of BBS content.

## Acceptance Criteria

- [ ] Dinky Kong (BBS cart) renders and plays on Bandai
- [ ] Chosen approach documented: official Lexaloffle PICO-8 arm64 runtime as a launcher, upstream fake08 fixes, or per-cart compatibility gating
- [ ] Known-incompatible carts fail with a user-visible message instead of a silent black screen

## Related

- `product/plugins/pico8/src/plugin.ts`

## Notes

Lexaloffle ships an official PICO-8 Raspberry Pi/arm64 binary that runs BBS carts natively; it is proprietary (paid license) so it would follow the local-plugin/operator-installed path rather than bundling. fake08 black screen = cart uses unsupported API; no error surfaces through retroarch.
