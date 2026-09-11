# RK3326 boot-chain spike

Answers one question: what does a standalone NixOS SD image for an R36-class
RK3326 handheld need at the firmware layer, and is any of it proprietary?

**Answer: none of it is proprietary, but only after trimming the TPL.** Both
stages now build from source on the aarch64 builder. Neither has been written
to an SD card or booted.

```
armTrustedFirmwarePX30   bl31.elf            210,872 bytes
uboot                    idbloader.img       100,352 bytes
                         u-boot.itb          904,704 bytes
                         u-boot-rockchip.bin 9,260,544 bytes
```

This is a stronger position than the RG353M and the RG DS, which both consume
a prebuilt Rockchip `rkbin` blob (`BL31_RK3568`, `TPL_RK3568`). RK3326 needs
no vendor blob at any boot stage.

It is also a different position from every shipping RK3326 distribution.
ROCKNIX sets `UBOOT_FIRMWARE="rkbin"` for RK3326 and uses Rockchip's
closed `rk3326_ddr_333MHz_*.bin` as its TPL. Going open here is a deliberate
divergence from the only configuration known to boot these boards.

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

Two Kconfig removals close the 2 KiB gap, and both are in `uboot.nix`:

- `# CONFIG_TPL_SERIAL is not set` — the cheapest 2 KiB available.
- `# CONFIG_DEBUG_UART is not set` — forced by the first. `sdram_common.c`
  calls `printascii` under that symbol, so leaving it on while `TPL_SERIAL`
  is off fails to link with `undefined reference to printascii`.

**Cost: there is no console during DRAM init.** Output starts at the SPL,
which runs from DRAM immediately after. A board that hangs inside DRAM
training will hang silently. Given that the June bring-up failed precisely
because nobody could see what the device was doing, this is the one trade in
this spike that may need revisiting — the alternative is to accept
Rockchip's DDR blob, which prints over UART2 and which ROCKNIX additionally
retunes to UART5 for K36-class clones using `ddrbin_tool.py`.

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
