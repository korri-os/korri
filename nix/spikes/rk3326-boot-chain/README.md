# RK3326 boot-chain spike

Answers one question: what does a standalone NixOS SD image for an R36-class
RK3326 handheld need at the firmware layer, and is any of it proprietary?

**Answer: none of it is proprietary, and on PX30 none of it can be.** Both
stages build from source on the aarch64 builder. Neither has been written to
an SD card or booted.

```
armTrustedFirmwarePX30   bl31.elf            210,872 bytes
uboot                    idbloader.img       102,400 bytes
                         u-boot.itb          904,704 bytes
                         u-boot-rockchip.bin 9,260,544 bytes
```

This is a stronger position than the RG353M and the RG DS, which both consume
a prebuilt Rockchip `rkbin` blob (`BL31_RK3568`, `TPL_RK3568`). RK3326 needs
no vendor blob at any boot stage.

ROCKNIX takes the opposite route: it sets `UBOOT_FIRMWARE="rkbin"` for
RK3326 and uses Rockchip's closed `rk3326_ddr_333MHz_v2.11.bin` as its TPL.
That route is not reachable from mainline U-Boot. `ROCKCHIP_PX30` does
`select TPL`, a hard Kconfig select that no defconfig can undo, so U-Boot
always builds and links its own TPL on this SoC. Setting
`CONFIG_ROCKCHIP_EXTERNAL_TPL=y` makes binman prefer the blob but does not
stop the in-tree TPL being built, and it still trips the size assertion.
Using the vendor blob would need a patch to `arch/arm/mach-rockchip/Kconfig`.
ROCKNIX can do it because it does not build mainline U-Boot.

So on PX30 the open TPL is not a preference. It is the only thing that
builds.

## The TPL is the whole difficulty

`common/spl/Kconfig.tpl` gives `ROCKCHIP_PX30` a `TPL_MAX_SIZE` of `0x2800`
— 10,240 bytes. That is the tightest budget in the file; RK3288 and RV1126
get `0x8000`, RK3399 gets `0x2e000`. PX30 loads the TPL into boot SRAM
between `TPL_TEXT_BASE` `0xff0e1000` and `TPL_STACK` `0xff0e4fff`.

As shipped, `odroid-go2_defconfig` on U-Boot 2025.10 with GCC 14 does not
fit. Raising the ceiling and letting the stage link produced the exact
number from mkimage:

```
Error: SPL image is too large (size 0x3000 than 0x2800)
```

One Kconfig removal closes the 2 KiB gap, and it is in `uboot.nix`:

```
# CONFIG_RAM_ROCKCHIP_DEBUG is not set
```

That symbol compiles the SDRAM capacity and timing reporting in
`sdram_common.c`, whose `printascii` calls are the TPL's only reason to
carry a console formatter. The RG353M's own upstream defconfig,
`anbernic-rgxx3-rk3566_defconfig`, disables it for the same reason.

**`TPL_SERIAL` and `DEBUG_UART` both stay on.** A board that hangs during
DRAM training can still say so, which matters because the June bring-up
failed precisely from having no visibility. Only the informational capacity
dump is lost.

An earlier attempt removed `TPL_SERIAL` instead. It also fits, at 100,352
bytes, but takes the console with it: `DEBUG_UART` has to go too, because
`sdram_common.c` references `printascii` under that symbol and the link
fails with `undefined reference to printascii`. Prefer the
`RAM_ROCKCHIP_DEBUG` route; it is 2 KiB more expensive and keeps the thing
worth having.

## What was checked, and where

| Claim | Source |
|---|---|
| U-Boot has open PX30 DRAM init | `drivers/ram/rockchip/sdram_px30.c`, `sdram_pctl_px30.c`, `sdram_phy_px30.c`, and `sdram-px30-{ddr3,ddr4,lpddr2,lpddr3}-detect-333.inc` |
| U-Boot builds its own TPL here | `configs/odroid-go2_defconfig`: `CONFIG_TPL_RAM=y`, `CONFIG_ROCKCHIP_SDRAM_COMMON=y`, `CONFIG_ROCKCHIP_PX30=y` |
| SPL hands off to a TF-A BL31 | same defconfig, `CONFIG_SPL_ATF=y` |
| TF-A supports the platform | `plat/rockchip/px30/` in `ARM-software/arm-trusted-firmware` |
| nixpkgs has no PX30 attribute | `armTrustedFirmware*` stops at RK3328/3399/3568/3588 |
| …but needs no nixpkgs patch | `buildArmTrustedFirmware` is re-exported through `pkgs/top-level/all-packages.nix` |
| `rkbin` has no PX30 entries | only `BL31_RK3568`, `BL31_RK3588`, `TPL_RK3566/3568/3588` |
| ROCKNIX uses the closed blob | `projects/ROCKNIX/devices/RK3326/options`: `UBOOT_FIRMWARE="rkbin"` |
| PX30 cannot skip its own TPL | `arch/arm/mach-rockchip/Kconfig`: `config ROCKCHIP_PX30` does `select TPL` |
| the blob's exact name | `bin/rk33/rk3326_ddr_333MHz_v2.11.bin` at the rkbin revision nixpkgs pins |

## Other consequences worth carrying forward

**No LPDDR4 timing table exists for PX30 upstream.** The tables cover DDR3,
DDR4, LPDDR2, and LPDDR3. Earlier desk research guessed this device class
ships LPDDR4; hardware contradicts it. The stock device tree on the unit
examined in May identifies as `rockchip,rk3326-evb-lp3-v12-linux`, and `lp3`
is LPDDR3. A board that really is LPDDR4 is not covered by this boot chain.

**U-Boot brings its own USB recovery path.** The defconfig enables
`CONFIG_CMD_ROCKUSB`, `CONFIG_USB_FUNCTION_ROCKUSB`, and
`CONFIG_USB_GADGET_DWC2_OTG`. A board that reaches U-Boot is recoverable
over USB without opening the case.

**The kernel console is UART2 at 1500000.** Stock ROCKNIX passes
`console=ttyS2,1500000` in both its `extlinux.conf` and its RK3326
`EXTRA_CMDLINE`, so the port and baud rate are already known.

## What this spike does not do

It pins the board device tree to the upstream Odroid Go 2 default. Panel,
joystick, button, LED, and RK915 Wi-Fi nodes are per-board facts and belong
in a device directory once the target unit is identified. A DTB from this
build will not light an R36-class square panel.

It is not a device directory for the same reason. The boot chain is a
property of the SoC and is identical across every R36-class RK3326 board,
while everything above it is per-board. Which unit this targets is still
unresolved, so no device name is committed here.

`uboot-tpl-size-probe.nix` raises `TPL_MAX_SIZE` to weigh the stage. Its
output is oversized by construction and must never be written to a device.

## A trap in the build script

`build` addresses nixpkgs by the revision in `flake.lock` rather than
calling `builtins.getFlake` on the working tree. Evaluating this repo as a
local flake copies every untracked file into the Nix store, and the checkout
routinely carries multi-gigabyte Rust output under `services/korrid/target`
and sibling trees under `.worktree`. Doing that turns a two-minute build
into an unbounded store copy that prints no progress at all.
