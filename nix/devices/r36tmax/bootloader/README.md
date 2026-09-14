# RK3326 boot-chain spike — R36T Max step 7

**Correction: the earlier claim that PX30 cannot use a vendor DDR binary
with mainline U-Boot was wrong.** U-Boot 2025.10 already has a binman path
for `ROCKCHIP_EXTERNAL_TPL`. PX30's unconditional `select TPL` also built
the open TPL. One conditional Kconfig selection removes that extra stage.
This candidate contains proprietary DDR firmware; it is not fully open.

This is **build-only**, not a bootloader installation or image-loader
selection. The operational baseline supplied for step 7 is that the current
card boots only with the ROCKNIX loader. The open PX30 TPL attempt was
unsuccessful. This work does not re-test that baseline or establish where
the failed boot stopped. In particular, lack of a successful boot does not
prove a specific DRAM-training or register fault.

Keep the known-good ROCKNIX loader and card available. The parent selects
the image loader. No device commands, raw-disk writes, bootloader installs,
stock-shell changes, registry changes, flake changes, or CI changes belong
to this step.

## Inputs and provenance

| Piece | Producer used here | License |
|---|---|---|
| DDR init | `rockchip-linux/rkbin`, `bin/rk33/rk3326_ddr_333MHz_v2.11.bin` | Proprietary, redistributable firmware |
| SPL and U-Boot | nixpkgs `buildUBoot`, U-Boot 2025.10, `odroid-go2_defconfig` | GPL-2.0-or-later metadata; see upstream per-file terms |
| BL31 | `ARM-software/arm-trusted-firmware` v2.13.0, `PLAT=px30` | BSD-3-Clause core; upstream lists other included licenses |

The repository's `flake.lock` pins nixpkgs at
`a6531044f6d0bef691ea18d4d4ce44d0daa6e816`. Its
`pkgs/by-name/rk/rkbin/package.nix` pins rkbin at
`f43a462e7a1429a9d407ae52b4745033034a6cf9`, with source NAR hash
`sha256-geESfZP8ynpUz/i/thpaimYo3kzqkBX95gQhMBzNbmk=`.
There is no second firmware download or floating revision in this spike.

At that rkbin revision, `RKBOOT/RK3326MINIALL.ini` names this exact file as
both `CODE471_OPTION/Path1` and `LOADER_OPTION/FlashData`. That producer
record grounds the DDR choice; none of its miniloader or USB-plug binaries
are used here. The RG353M and RG DS `uboot.nix` files establish the local
`ROCKCHIP_TPL` input pattern. nixpkgs exports no `TPL_RK3326` convenience
attribute, but its rkbin package installs the file unchanged.

Verified DDR file:

- Size: **10,100 bytes**.
- SHA-256: `2df577824953bea3584282e7f5118c96620e1348dbc378d50604fe07c70eb3e4`.
- `px30_ddr_333MHz_v2.11.bin` in that same source has the same SHA-256.
- U-Boot's 2 KiB alignment makes it **10,240 bytes**, exactly the existing
  PX30 `0x2800` mkimage limit. The limit is not raised.

Rockchip's `LICENSE` permits use, copying, and distribution. It disclaims
warranties and prohibits reverse engineering and removing notices. nixpkgs
classifies it as `unfreeRedistributableFirmware`. The combined output is
therefore not labelled GPL-only. `default.nix` permits only rkbin and this
mixed-license U-Boot package by name. Consumers outside this spike must
permit the unfree firmware themselves; this step does not change their
license policy or select their image loader. `Licenses/` in the output carries the
U-Boot license files, `rkbin-LICENSE`, and TF-A license documentation. This
is not a legal review of a future distribution; preserve the notices and
meet the source-distribution duties before publishing binaries.

## The narrow mainline change

`px30-external-tpl.patch` changes only this PX30 selection:

```diff
- select TPL
+ select TPL if !ROCKCHIP_EXTERNAL_TPL
```

`uboot.nix` enables `CONFIG_ROCKCHIP_EXTERNAL_TPL=y`, explicitly disables
`CONFIG_TPL`, and passes the reviewed DDR file as `ROCKCHIP_TPL`. The explicit
disable removes the TPL selection already saved by nixpkgs' initial
`make odroid-go2_defconfig`; the conditional select alone is not sufficient
in that two-step configuration flow.

It also disables `CONFIG_SPL_BOOTROM_SUPPORT`. With no open TPL,
`TPL_ROCKCHIP_BACK_TO_BROM` no longer selects `ROCKCHIP_BROM_HELPER`.
The inherited SPL boot-ROM load method then fails to link with
`undefined reference to back_to_bootrom`. SPL loads the FIT itself in this
chain; it does not ask the ROM to load the next stage. Disabling this unused
method follows the existing external-DDR RG353M defconfig. No new ROM-return
implementation or hardware patch is added. ROM recovery is not verified.

The existing `arch/arm/dts/rockchip-u-boot.dtsi` binman description packs vendor DDR
followed by mainline SPL into `idbloader.img`, then places the TF-A/U-Boot
FIT in `u-boot-rockchip.bin`. BL31 still builds from source.

There are no hardware-register, DRAM-timing, SRAM-limit, or board-DTB
patches. The old `RAM_ROCKCHIP_DEBUG` size workaround is no longer needed
because no open TPL is built. `uboot-tpl-size-probe.nix` remains a historical
measurement of the open stage, **not a boot artifact**. Do not write its
output to a device.

## Build and checks

From the repository root:

```sh
nix/devices/r36tmax/bootloader/build uboot
```

This existing Nix-shebang script builds on the configured aarch64 build
machine (`fuji` by default), never on the handheld. It returns a host Nix
store output path without a result symlink. It imports the locked nixpkgs
revision rather than copying this entire checkout through a local flake.
`KORRI_AARCH64_BUILDER` and `KORRI_AARCH64_BUILDER_KEY` select a build
machine and its SSH key. They must not point at a target device.

The derivation runs these checks on real build outputs:

- Resolved Kconfig selects external DDR, SPL, TF-A, FIT, MBR, ext4, and
  extlinux. `CONFIG_TPL` and `tpl/u-boot-tpl.bin` must be absent.
- The reviewed DDR digest matches. The exact DDR bytes appear at byte 2048
  in `idbloader.img`; the exact mainline SPL follows the aligned DDR.
- Every FIT payload matches its build input: U-Boot, each BL31 ELF load
  segment and its address, and the DTB. The default FIT configuration names
  TF-A as firmware and the expected DTB.
- The combined output contains the exact idbloader and FIT at the existing
  `CONFIG_SPL_PAD_TO` offset, with no overlap.
- Five altered copies must fail: an enabled open TPL, changed DDR bytes,
  a truncated FIT, a changed FIT payload, and a changed combined loader.
- U-Boot's own `mkimage -T rksd -l` and `dumpimage -l` inspect the idbloader
  and FIT.

`check-artifacts.py` reads format facts from U-Boot's `tools/rkcommon.c`,
`tools/rkcommon.h`, and `arch/arm/dts/rockchip-u-boot.dtsi`. The tests do not
claim secure boot: this board configuration does not enable FIT signatures.
The installed `u-boot.config` records the resolved configuration.

## Verified build result — 2026-09-14

Native aarch64 build on `fuji` passed, including all five rejection tests.
The installed files were then inspected on the x86_64 host with `mkimage`,
`dumpimage`, `readelf`, byte comparisons, and SHA-256 checks.

Output:

```
/nix/store/a309qzqy72wxcr3gw61g0nydm0z3pxil-uboot-odroid-go2_defconfig-2025.10
```

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| `idbloader.img` | 100,352 | `ff4231b7af24968d5508c5f09ddaac975d9b0e88bedcc2a1d322248da73ae869` |
| `u-boot.itb` | 904,704 | `8521709c34d4e1345d6f94de1ef004799297434838057466e3486fb44ef8927b` |
| `u-boot-rockchip.bin` | 9,260,544 | `9d7d1bb637495093b39bb1685e5b573ca9a9f16b593d374b8da393099f7355c9` |

`mkimage` reports RK33 SD/MMC format, 10,240 bytes of aligned DDR init,
and 88,064 bytes of aligned SPL. The FIT contains U-Boot, three BL31 load
segments, and `rockchip/rk3326-odroid-go2`'s DTB. TF-A is ELF64/AArch64
with entry `0x40000`. Binman reports absent optional `tee-os`; this candidate
does not include OP-TEE.

The host's configured GitHub binary cache returned HTTP 504 during one
attempt. The successful build used a process-local host override:

```sh
NIX_CONFIG='substituters = https://cache.nixos.org' \\
  nix/devices/r36tmax/bootloader/build uboot
```

No target cache policy or persistent host configuration was changed.

## Remaining hardware gates

**A successful build is not proof of a successful boot.** Keep this candidate
unselected until a separately approved boot test.

1. Cold boot with a captured early-stage serial log: prove DDR return,
   mainline SPL, BL31, U-Boot, SD/extlinux loading, and kernel handoff.
2. Warm reboot with the same evidence; repeat cold and warm boots to check
   that success is not dependent on retained state.
3. Confirm board behavior with the real R36T Max. The unchanged Odroid Go 2
   bootloader DTB is a starting point, not proof of matching power rails,
   MMC wiring, controls, or display. Do not infer a memory technology solely
   from a stock DT compatible string.
4. Keep the known-good ROCKNIX loader available throughout. Card/image
   selection, recovery, and any writes require their own approval.

The inherited defconfig keeps its MMC environment backend. This step does
not exercise `saveenv`, flash commands, USB recovery, or any device effect.
No claim is made that those operations are safe on this unit.
