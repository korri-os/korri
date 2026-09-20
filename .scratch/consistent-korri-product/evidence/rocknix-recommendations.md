# ROCKNIX-based emulator defaults

Decision: [Choose per-device emulator defaults from ROCKNIX](../issues/10-validate-device-emulator-defaults.md).

## What was checked

Retrieved on 2026-09-20 UTC. The initial delegated search located the official pages. The parent then fetched their full article text and followed their **Platform Documentation** links to ROCKNIX's generated emulator tables. These findings supersede the initial search-snippet-only report.

The tables were read at ROCKNIX distribution commit [`7f1b3abece2c7d263cebd16b5a2ba4268d6ddaa5`](https://github.com/ROCKNIX/distribution/commit/7f1b3abece2c7d263cebd16b5a2ba4268d6ddaa5), the observed `next` tip. This is a pinned documentation/source snapshot, not verification of a stable release artifact. The [official FAQ](https://rocknix.org/faqs/) says emulator configuration is per device and documented at build time.

**Verified upstream facts** below come from explicit `(default)` markers or named alternatives in those tables, not their display order. **Korri selections** are the delegated judgment requested by Simon. They choose coverage and a few explicitly identified alternatives. They do not claim measured performance, published Korri packages, or working installed devices.

No benchmark, image build, game download, device connection, or firmware operation was performed. This document is a selection record, not a new configuration schema.

## Device-to-source mapping

| Korri device | ROCKNIX source | Applicability |
|---|---|---|
| RG353M | [Exact RG353 family page](https://rocknix.org/devices/anbernic/rg353pmvvs/) links [RK3566 table][rk3566]. | Direct documented family mapping. |
| Odin 2 Portal | [Exact device page](https://rocknix.org/devices/ayn/odin2portal/) links [SM8550 table][sm8550]. | Direct documented mapping. |
| RG DS | Korri's [device record](../../../nix/devices/rgds/README.md) identifies RK3568. Use [RK3566 table][rk3566] as the nearby Rockchip baseline. | **Korri inference**, not an official RG DS default list. The pinned [table index][index] contains no RK3568 directory. Dual-screen input and presentation remain Korri integration work. |
| R36T Max | Korri's [hardware record](../../../nix/devices/r36tmax/HARDWARE.md) identifies RK3326. The official [R35S/R36S page](https://rocknix.org/devices/unbranded/game-console-r35s-r36s/) links [RK3326 table][rk3326]. | **SoC-family selection**, not a claim that the R36T Max is the R36S or has identical board support. |
| RP Mini V2 | [Pocket Mini page](https://rocknix.org/devices/retroid/retroid-pocket-mini/) discusses Mini V2 and links [SM8250 table][sm8250]. | Documented family mapping. Display and boot differences are not erased by choosing the same emulator family. |
| RG35XXSP | [Exact device page](https://rocknix.org/devices/anbernic/rg35xx-sp/) links [H700 table][h700]. | Direct documented mapping. |

## Chosen shared retro set

All six device lists include the following set. The common table avoids repeating the same rows six times; it does not replace per-device selection.

`korri-plugin-*` names below are actual outputs of the separate `korri-plugins` repository inspected at `7f4eae0c27750118ec6c39663f8644145cc7bdd8`. Package definitions and platform metadata are evidence of a producer, not a successful build or a published binary.

| Content | Chosen runner | Basis | Existing producer |
|---|---|---|---|
| NES / Famicom | Nestopia, libretro. | Explicit default in all five family tables. | `korri-plugin-nestopia` |
| SNES / Super Famicom | Snes9x, libretro. | Explicit default; not an automatic choice of an older Snes9x variant. | `korri-plugin-snes9x` |
| Game Boy / Game Boy Color | Gambatte, libretro. | Explicit default. | `korri-plugin-gambatte` |
| Game Boy Advance | mGBA, libretro. | Explicit default. | `korri-plugin-mgba` |
| Mega Drive / Genesis | Genesis Plus GX, libretro. | Explicit default. | `korri-plugin-genesis-plus-gx` |
| Master System / Game Gear / SG-1000 | Genesis Plus GX, libretro. | **Korri choice of a listed alternative.** ROCKNIX defaults to Gearsystem; the existing Genesis Plus GX plugin covers these systems too. | `korri-plugin-genesis-plus-gx` |
| Sega 32X | PicoDrive, libretro. | Explicit default. | `korri-plugin-picodrive` |
| PC Engine / TurboGrafx-16 | Beetle PCE Fast, libretro. | Explicit default. CD-system discovery is not implied by the current Korri declaration. | `korri-plugin-beetle-pce-fast` |
| Atari 2600 / 7800 | Stella / ProSystem, libretro. | Explicit defaults for the respective systems. | `korri-plugin-stella`, `korri-plugin-prosystem` |
| Atari Lynx | Handy, libretro. | Explicit default. | `korri-plugin-handy` |
| Neo Geo Pocket / Color | Beetle NeoPop, libretro, named `beetle_ngp` upstream. | Explicit default. | `korri-plugin-beetle-ngp` |
| WonderSwan / Color | Beetle WonderSwan, libretro. | Explicit default. | `korri-plugin-beetle-wswan` |
| Game & Watch | GW, libretro. | Explicit default. | `korri-plugin-gw` |
| FinalBurn-compatible arcade / Neo Geo | FinalBurn Neo, libretro. | Explicit default for FBN, Neo Geo, and CPS entries. **Not a universal MAME substitute.** | `korri-plugin-fbneo`; archive discovery still needs integration. |
| PlayStation | PCSX-ReARMed, libretro, using the existing target-architecture build. | **Korri choice of a listed alternative.** ROCKNIX defaults to `pcsx_rearmed32`; its separate `pcsx_rearmed` alternative is also listed. Do not claim Korri supplies the 32-bit variant. | `korri-plugin-pcsx-rearmed` |

Include the existing `korri-plugin-retroarch` settings-family contribution with the libretro selection. Each core still owns its native frontend dependency; the family plugin is not a second runner or permission to bake RetroArch into the base system.

## Chosen per-device additions

These are intended default selections, not certification. Missing native plugin packages are named explicitly below. Do not silently replace them merely to make an image build succeed.

| Device | Add to the shared retro set | Leave optional in this initial selection |
|---|---|---|
| RG353M | N64: Mupen64Plus-Next, libretro. Dreamcast: Flycast, libretro. PSP: PPSSPP standalone. DS: DraStic standalone. | Saturn, GameCube/Wii, 3DS, PS2, and other heavier or specialist systems. |
| RG DS | N64: Mupen64Plus-Next, libretro. Dreamcast: Flycast, libretro. PSP: PPSSPP standalone. DS: DraStic standalone. | Saturn and newer home-console/3DS systems. RK3566-based choice is an inference; no automatic dual-screen support claim. |
| R36T Max | DS: DraStic standalone. | N64, Dreamcast, PSP, Saturn, and newer systems. This is conservative Korri curation, **not a claim that ROCKNIX omits those entries or that the hardware can never run them**. |
| RG35XXSP | DS: DraStic standalone. | N64, Dreamcast, PSP, Saturn, and newer systems, under the same conservative selection rule. |
| RP Mini V2 | N64: Mupen64Plus-Next, libretro. Dreamcast: Flycast, libretro. PSP: PPSSPP standalone. DS: melonDS, libretro. Saturn: YabaSanshiro standalone. GameCube/Wii: Dolphin standalone. 3DS: Azahar standalone. PS2: AetherSX2 standalone, subject to native-package and distribution-rights checks. | Alternative cores, Vita, Windows compatibility stacks, and specialist engines/computers. |
| Odin 2 Portal | The same named additions as RP Mini V2, with its own device configuration and later acceptance. | The same optional categories. A faster SoC does not itself verify games or package availability. |

The low-power coverage cutoff and the decision not to preinstall every listed system are **Korri judgments** implementing the curated-defaults instruction. ROCKNIX's tables even list some newer consoles on RK3566; a configured entry is not a dependable-performance promise. Classic computers and additional engines remain installable candidates, not part of this initial default set.

### Explicit source choices

- **N64:** all five tables name `retroarch: mupen64plus_next` as default. Map it to existing `korri-plugin-mupen64plus`. Do not enable the ARM-blocked ParaLLEl N64 package as an implicit substitute.
- **Dreamcast:** RK3326/H700/RK3566 default to Flycast 2021 libretro; SM8250/SM8550 default to standalone Flycast. All list current Flycast libretro as an alternative. **Choose that documented alternative for the selected Korri devices**, using existing `korri-plugin-flycast`. This is a declared frontend/version choice, not a runtime fallback.
- **PSP:** the family tables explicitly default to standalone PPSSPP, and the [PSP page](https://rocknix.org/systems/psp/) distinguishes standalone and libretro. Choose core's existing [standalone PPSSPP producer](../../../plugins/ppsspp/plugin.ts), exported by [plugin-host composition](../../../services/korrid/plugin-host/default.nix) as `korri-plugin-ppsspp`. Do not also bundle the separate repository's generated libretro package with the same plugin/runner identity. Typed settings and session-control gaps in the standalone declaration remain implementation work.
- **DS:** lower/middle family tables default to standalone DraStic; the Qualcomm tables default to standalone melonDS. Choose DraStic on the four weaker models. On the two Qualcomm models, choose the listed libretro melonDS alternative, available as `korri-plugin-melonds`. The [generic DS page](https://rocknix.org/systems/nds/) instead marks libretro melonDS as default for all platforms. This document uses the more specific pinned family tables to explain the difference, then records Korri's deliberate alternative.
- **Higher systems:** Qualcomm family tables mark standalone YabaSanshiro, Dolphin, Azahar, and AetherSX2 as defaults. [GameCube documentation](https://rocknix.org/systems/gamecube/) distinguishes Dolphin frontends; [3DS documentation](https://rocknix.org/systems/3ds/) names Azahar. Choose those native defaults, not libretro Dolphin, Yabause, old Citra, or Play! solely because another package exists. The [generic PS2 page](https://rocknix.org/systems/ps2/) lists SD865 but not SM8550; the pinned SM8550 family table does include AetherSX2. This is documentation scope drift, not measured Korri performance.

## Packaging and release limits

The native DraStic, YabaSanshiro, Dolphin, Azahar, and AetherSX2 selections do not currently have corresponding plugin producers in the inspected core/independent-repository output sets. They are named implementation gaps, not fabricated package IDs. Native binaries must arrive prebuilt. Confirm redistribution rights and required notices before including third-party binaries, especially proprietary runtimes; source selection alone is not permission to redistribute them.

The existing FBNeo declaration claims `cue` and `ccd`, while ROCKNIX documents matching arcade archives. Selecting its package does not establish usable archive discovery or correct ROM-set matching. Likewise, selecting a core does not invent missing system declarations, content locations, BIOS handling, settings, or gameplay controls.

Required BIOS and game data remain user-supplied. Commercial native [PICO-8](https://rocknix.org/systems/pico-8/) requires an owner's purchase and supplied runtime files according to ROCKNIX, so it is not included in the preinstalled set. This selection does not add PortMaster, content-acquisition providers, Steam, or compatibility-runtime ports to the current effort. The earlier Moonlight, SSH, and Tailscale bundle decisions remain unchanged.

The website's device pages contain installation and bootloader procedures outside this task. None were executed or adopted. In particular, the Pocket Mini page carries a Mini V2 OTA/bootloader warning; it is not a diagnosis of the user's current device. Korri's firmware-write prohibitions and recovery rules remain authoritative.

**Cost:** these choices avoid a benchmarking detour but retain package/port work and performance uncertainty. The separately agreed hardware acceptance gates still apply before calling any Korri image supported. No image sizing, image-variant choice, or first-use readiness promise was made.

[rk3566]: https://github.com/ROCKNIX/distribution/blob/7f1b3abece2c7d263cebd16b5a2ba4268d6ddaa5/documentation/PER_DEVICE_DOCUMENTATION/RK3566/SUPPORTED_EMULATORS_AND_CORES.md
[rk3326]: https://github.com/ROCKNIX/distribution/blob/7f1b3abece2c7d263cebd16b5a2ba4268d6ddaa5/documentation/PER_DEVICE_DOCUMENTATION/RK3326/SUPPORTED_EMULATORS_AND_CORES.md
[h700]: https://github.com/ROCKNIX/distribution/blob/7f1b3abece2c7d263cebd16b5a2ba4268d6ddaa5/documentation/PER_DEVICE_DOCUMENTATION/H700/SUPPORTED_EMULATORS_AND_CORES.md
[sm8250]: https://github.com/ROCKNIX/distribution/blob/7f1b3abece2c7d263cebd16b5a2ba4268d6ddaa5/documentation/PER_DEVICE_DOCUMENTATION/SM8250/SUPPORTED_EMULATORS_AND_CORES.md
[sm8550]: https://github.com/ROCKNIX/distribution/blob/7f1b3abece2c7d263cebd16b5a2ba4268d6ddaa5/documentation/PER_DEVICE_DOCUMENTATION/SM8550/SUPPORTED_EMULATORS_AND_CORES.md
[index]: https://github.com/ROCKNIX/distribution/tree/7f1b3abece2c7d263cebd16b5a2ba4268d6ddaa5/documentation/PER_DEVICE_DOCUMENTATION
