---
id: 01KVDXP0A4DKSQY78KYY84XEAP
slug: build-plugin-owned-portmaster-compatibility-substrate
title: Build plugin-owned PortMaster compatibility substrate
origin: parked
status: To Do
priority: high
labels:
  - plugins
  - portmaster
  - compatibility
  - bandai
  - runtime
created: 2026-06-18
source: user
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: trunk
  commit: ff6ef741
  repo: simonwjackson/korri
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/parking-lot/01KVDXP0A4DKSQY78KYY84XEAP-build-plugin-owned-portmaster-compatibility-substrate.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep PortMaster compatibility and the Wordle SDL, Digger, and SkiFree milestones. The current published output map contains no PortMaster package. Checked boxes in the historical record refer to legacy work only.

## Scope on main

Recover only the needed compatibility behavior through current plugin declarations and native artifacts. Installation and spawning remain host effects. Prebuilt runtime packages must replace any target-side build path. Do not restore the former product tree or adopt its manifest proposals without review.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

Legacy screenshots prove earlier milestones, not current image gameplay. Runtime lanes, input isolation, and per-port licensing still need acceptance.

## Supporting legacy evidence

The staged game milestones and compatibility findings are in
`0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/parking-lot/01KVDXP0A4DKSQY78KYY84XEAP-portmaster-plugin-compatibility-substrate-brief.md`.
That brief is historical. Its install handlers and native package layout need current contract review.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Build plugin-owned PortMaster compatibility substrate

## Why it matters

PortMaster catalog search and package acquisition are now first-party, but ready-to-play ports still need manual launch adaptation on Bandai/NixOS. A plugin-owned compatibility substrate would let users install and launch PortMaster entries through generic Korri plugin seams while keeping Korri core unaware of PortMaster-specific layouts, scripts, runtime packs, architecture quirks, and per-port compatibility overrides.

## Acceptance Criteria

- [x] `@korri:portmaster` can install a selected catalog zip into writable Korri storage, verify it, and emit an installed-port manifest without Korri core knowing PortMaster layout details.
- [x] The PortMaster plugin provides a compatibility launch envelope for original PortMaster scripts, including control.txt, `/roms/ports` semantics, helper functions, shell compatibility, and safe device/helper stubs.
- [x] Native aarch64 ready-to-run ports such as Wordle SDL install and launch from catalog through plugin-owned normalization, ELF repair, and foreground presentation.
- [x] x86_64 ready-to-run ports such as Digger launch on Bandai through a plugin-owned FEX lane with bundled `libs.x86_64`, SDL backend handling, and foreground behavior.
- [x] Runtime-dependent RetroArch/libretro ports launch through plugin-declared dependencies and a plugin-owned runtime seam.
- [ ] Additional runtime-dependent ports launch through plugin-declared dependencies as FRT/Godot, Love2D, Mono, Java, GL4ES/WestonPack/GMloader runtime plugins land.
- [x] armhf lanes are supported through a plugin-owned qemu/rootfs strategy and validated with SkiFree on Bandai; 32-bit x86 remains future diagnostic/support work.
- [ ] Installed PortMaster games materialize as normal Korri library entries with launch, stop, logs, diagnostics, and compatibility metadata owned by the plugin.
- [ ] The phased plan in `work/items/parking-lot/01KVDXP0A4DKSQY78KYY84XEAP-portmaster-plugin-compatibility-substrate-brief.md` remains the handoff brief and each phase ends with a real Bandai launch milestone.

## Related

- `work/items/parking-lot/01KVDXP0A4DKSQY78KYY84XEAP-portmaster-plugin-compatibility-substrate-brief.md`
- `product/plugins/portmaster/src/plugin.ts`
- `product/plugins/portmaster/packages/portmaster/default.nix`
- `product/plugins/portmaster/nix/composition.nix`
- `product/plugins/gamescope/src/plugin.ts`
- `product/plugins/fex-runtime/packages/fex-runtime/setup-env`

## Notes

Created after live Bandai smoke tests: Wordle SDL native aarch64 required shebang/ELF/display adaptation, and Digger x86_64 launched through FEX with SDL x11. User explicitly wants a plugin-owned approach where Korri core stays unaware and each phase has a launchable milestone.

2026-06-18 Phase 1 landed locally: added plugin-owned `portmaster.install` normalizer plus ZIP extraction, catalog MD5 verification, safe PortMaster ports-layout extraction, script/binary detection, and installed manifest emission. Live validation installed `wordlesdl.zip` from the packaged catalog, transferred the manifest tree to Bandai, applied the phase-allowed manual ELF patch/wrapper, and captured Wordle rendering from the installed tree.

2026-06-18 Phase 2 landed locally: added plugin-owned `portmaster.prepare-launch` envelope generation. The envelope writes `control.txt` and `tasksetter`, exposes both `$XDG_DATA_HOME/PortMaster` and `/roms/ports/PortMaster` semantics, provides helper/device stubs, and returns a bwrap/bash launch command for unmodified PortMaster scripts. Live validation launched the original `Wordle SDL.sh` (`#!/bin/bash` unchanged) through the generated envelope on Bandai and captured Wordle rendering.

2026-06-18 Phases 3-7 landed locally and were validated on Bandai: native ELF repair rendered Wordle SDL, x86_64/FEX rendered Digger, foreground presentation focused/fullscreened Digger, input compatibility ran generated `gptokeyb`, and RetroArch/libretro runtime compatibility rendered 2048 via generated `/usr/bin/retroarch` binding.

2026-06-18 Phase 8 completed the plugin-owned armhf/qemu lane: install-time armhf executable wrappers preserve originals under `.korri-qemu-arm/`, launch env prefers `DEVICE_ARCH=armhf`, and qemu/rootfs/library env is recorded in manifests and envelopes. Added `.#portmaster-armhf-runtime`, a plugin-owned Debian bookworm armhf SDL runtime package with `nix-support` metadata for rootfs, library path, qemu-arm, and env. Bandai verified qemu-arm can execute real PortMaster armhf helper binaries (`xdelta3.armhf`, `7zzs.armhf`), then launched `skifree.zip` through package-provided aarch64 qemu-arm and the package-provided armhf runtime. Screenshot `/tmp/portmaster-phase8-skifree5-screen-focused.png` confirmed SkiFree rendered.
