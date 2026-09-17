# Anbernic RG35XXSP

First-boot baseline for the Anbernic RG35XXSP clamshell handheld.

## Hardware Summary

| Component | Specification |
|---|---|
| SoC | Allwinner H700 (quad-core ARM Cortex-A53 @ 1.5 GHz) |
| GPU | ARM Mali-G31 MP2 (Panfrost supported) |
| RAM | 1 GB LPDDR4 |
| Storage | Dual microSD (TF1/OS, TF2/Games) |
| Display | 3.5-inch 640x480 IPS |
| Form Factor | Clamshell with hall sensor on pin PE7 (`SW_LID`) |
| PMIC | X-Powers AXP717 on `r_i2c` |
| Connectivity | Realtek RTL8821CS (SDIO Wi-Fi, UART Bluetooth) |

## Bootloader

Mainline U-Boot v2025.10 with `anbernic_rg35xx_h700_defconfig`:
- BL31: ARM Trusted Firmware v2.13 (`armTrustedFirmwareAllwinnerH616`)
- Device tree: `allwinner/sun50i-h700-anbernic-rg35xx-sp`
- Output binary: `u-boot-sunxi-with-spl.bin`

## Boot Sector Layout

Allwinner BROM loads SPL from sector 16 (8 KiB offset) on the SD card:
- Sector 0: MBR partition table
- Sectors 1-15: Reserved
- Sector 16: `u-boot-sunxi-with-spl.bin` (SPL + FIT containing U-Boot, BL31, DTB)
- Offset 16 MiB: Partition 1 (FAT firmware / extlinux boot)
- Partition 2: Ext4 root filesystem (`NIXOS_RG35XXSP`)
