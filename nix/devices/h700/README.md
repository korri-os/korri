# Anbernic H700 handhelds

Shared kernel, U-Boot, controller profile and board facts for two Anbernic
devices with the Allwinner H700:

| Device | Directory | Device tree | Image |
|---|---|---|---|
| RG35XX SP | `../rg35xxsp` | `sun50i-h700-anbernic-rg35xx-sp` | `rg35xxsp-sd-image` |
| RG35XX SP, v2 panel | `../rg35xxsp` | `sun50i-h700-anbernic-rg35xx-sp-v2-panel` | `rg35xxsp-v2-panel-sd-image` |
| RG35XX Pro | `../rg35xxpro` | `sun50i-h700-anbernic-rg35xx-pro` | `rg35xxpro-sd-image` |

Status: development images. No H700 board has booted this kernel yet. Every
display, input, audio and Wi-Fi value comes from the device trees, the ROCKNIX
sources and the driver sources, and each file names its source. Physical
acceptance (`.scratch/korri-product-build/issues/18-*.md`) still decides
support.

## Hardware summary

| Component | Specification |
|---|---|
| SoC | Allwinner H700 (quad-core Cortex-A53 at 1.5 GHz) |
| GPU | Mali-G31 MP2, Panfrost |
| RAM | 1 GB LPDDR4 |
| Storage | Two microSD slots. TF1 boots. |
| Display | 3.5-inch 640x480, parallel RGB through `panel-mipi-dpi-spi` |
| PMIC | X-Powers AXP717 on `r_i2c` |
| Wi-Fi and Bluetooth | Realtek RTL8821CS (SDIO Wi-Fi, UART Bluetooth) |
| Controls | `rocknix-singleadc-joypad` (buttons; Pro: two sticks), `gpio-keys` volume |
| SP only | Lid switch on PE7 (`SW_LID`), NXP PCF8563 RTC |

## Kernel

`kernel/` is Linux 7.2 with ROCKNIX's H700 patch queue and ROCKNIX's H700
`config`, unchanged. Korri adds:

- `config-korri.delta`: the product's netfilter set (as on the RP Mini V2),
  the composite USB ACM console, and the joypad driver. `default.nix` applies
  it to `config` at evaluation.
- Patch 0222 and the pinned `ROCKNIX/rocknix-joypad` source (`d02ed13a`, the
  revision ROCKNIX next uses on 7.2): the board trees bind the gamepad to this
  driver, and ROCKNIX builds it out of tree. Korri builds it in tree, so it
  comes with the cross-compiled kernel.
- Patch 0223: ROCKNIX's RG35XX Pro and SP v2-panel device trees, unchanged.

The kernel and U-Boot are cross-built with GCC 15 on a development machine
(`packages.x86_64-linux.h700-kernel`, `h700-uboot`). The images take those
builds. Nothing compiles on the handheld.

## SP panel revisions

ROCKNIX ships two SP device trees and states that the panel cannot be told
apart without opening the device: if the screen shows garbage, use the other
tree. A card that already holds ROCKNIX shows the answer in its `dtb.img`
(`rocknix-dt-id`). Without that, try `rg35xxsp-sd-image` first.

## Bootloader

Mainline U-Boot 2025.10 with `anbernic_rg35xx_h700_defconfig` and its default
control tree, as ROCKNIX builds it for every H700 board, plus ROCKNIX's two
boot settings: no autoboot delay and no EFI loader. BL31 is ARM Trusted
Firmware 2.13 (`armTrustedFirmwareAllwinnerH616`).

## Boot sector layout

The Allwinner BROM loads the SPL from sector 16 (8 KiB) on the SD card:

- Sector 0: MBR partition table
- Sector 16: `u-boot-sunxi-with-spl.bin` (SPL and a FIT with U-Boot, BL31 and the DTB)
- 16 MiB: partition 1, FAT `NIXOS_BOOT`, empty
- Partition 2: ext4 root (`NIXOS_RG35XXSP` or `NIXOS_RG35XXPRO`) with `/boot/extlinux`

## First boot access

The product USB gadget gives a root console (`/dev/ttyACM*` on the host) and a
network link: the SP at `10.42.5.1`, the Pro at `10.42.6.1`. The board's UART
is `ttyS0` at 115200 baud.

## Recorded limits

- Chromium runs without GPU acceleration, as on the RG353M and RG DS.
- No sleep state is declared. The power key and the SP lid power off.
- The PCF8563 RTC driver is not built, so the SP keeps time on the SoC RTC
  until NTP corrects it.
- HDMI audio needs the ROCKNIX `sunxi_v2` ahub driver, which ROCKNIX also
  leaves disabled. Only the codec (speaker and headphone) is expected to play.
- No hardware video encoder is selected; the streaming host is not installed.
