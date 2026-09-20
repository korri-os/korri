# Decide RG35XXSP product adoption

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
Blocked by: 07

## Question

RG35XXSP is in the device set and its console-only composition is a debugging state, not a hardware limit ([Decide the shared product and hardware boundary](07-decide-product-device-boundary.md#ownership)). Today [its device module](../../../nix/devices/rg35xxsp/default.nix) imports only `nix/base`, the device cache policy, and the SD format. Which hardware facts must it supply to the product module, which of its parts (H700 kernel, mainline U-Boot, Panfrost, AXP717, RTL8821CS) are bring-up that stays, and what is unknown about the hardware?

Inspect the [README](../../../nix/devices/rg35xxsp/README.md), [kernel.nix](../../../nix/devices/rg35xxsp/kernel.nix), [uboot.nix](../../../nix/devices/rg35xxsp/uboot.nix), and [module-check.nix](../../../nix/devices/rg35xxsp/module-check.nix) for what is verified and what is assumed. Compare with the closest already-integrated device for the hardware facts the product module will require. Do not decide the product module's option list here; that is extracted from the existing host and portal modules during `/to-spec`.

Resolve with Simon what RG35XXSP must provide, which of its 1 GB RAM and 640x480 display constraints are recorded limits versus required-behavior blockers under [Define hardware limits and product acceptance](02-define-device-support.md#answer), and whether it has any delivery route before the image and cache workflows cover it.

## Comments

2026-09-20. Source rechecked at `f530c288`. No device was contacted and no image was built. The kernel version below comes from one local evaluation; the device-tree facts come from the upstream Linux sources named in each row.

### What the device supplies today

| Fact | Source |
|---|---|
| The configuration imports `nix/base`, `nix/device-cache/nixos-module.nix`, and `nix/formats/sd-card.nix` with `gpt = false`. It imports no Linux host, no portal, no plugin host, and declares no runtime account. | `nix/devices/rg35xxsp/sd-image.nix:13-18` |
| Root logs in automatically on serial and console "for first-boot diagnostic verification". | `nix/devices/rg35xxsp/sd-image.nix:63` |
| The kernel is stock `pkgs.linuxPackages`. `nix eval .#packages.x86_64-linux.rg35xxsp-kernel.version` returns `6.12.63`. | `nix/devices/rg35xxsp/kernel.nix` |
| U-Boot is mainline v2025.10 with `anbernic_rg35xx_h700_defconfig` and ATF v2.13, written to sector 16. | `nix/devices/rg35xxsp/uboot.nix` |
| `module-check.nix` asserts 13 evaluation facts: platform, DTB name, no overlays, extlinux, root label, sector offset, three initrd modules. It proves declaration only. | `nix/devices/rg35xxsp/module-check.nix` |
| `hardware.graphics.enable = true` with no seat or compositor, so the shared base check that requires graphics, seatd, and polkit together is red on `main`. | `nix/base/module-check.nix:82-97`, parked item `01M2SGDH97XZVJX1ESEV302J3N` |
| No delivery. Neither `.github/workflows/device-images.yml` nor `.github/workflows/nix-cache.yml` names `rg35xxsp`. | both workflow files |

### The screen is missing from mainline, not from the hardware

In Linux v6.12, `sun50i-h616.dtsi` describes no GPU, no display engine, no TCON, no DSI, and no audio codec. The board files `sun50i-h700-anbernic-rg35xx-sp.dts`, `-plus.dts`, and `-2024.dts` add the lid switch on PE7, an NXP PCF8563 RTC, SDIO Wi-Fi and UART Bluetooth for the RTL8821CS, the gamepad and volume `gpio-keys`, the AXP717 with battery and USB power supplies, one green LED, and `usbotg` in `dr_mode = "peripheral"`. They add no panel and no display path.

In Linux v7.2 the same dtsi gains `gpu@1800000` (`arm,mali-bifrost`) and `codec@5096000`, and the 2024 board file enables `&codec` and `mali-supply`. A display engine, a TCON, and a panel are still absent.

So on the pinned kernel the device has no display output and `panfrost` in the initrd binds nothing. This is missing upstream support, which [ticket 02](02-define-device-support.md#answer) classes as unfinished integration, not a hardware limit.

An unlanded answer to that gap already exists in the working tree at `.worktree/rg35xxsp-display` (branch `feat/rg35xxsp-display`, 38 staged and uncommitted files): a ROCKNIX-derived Linux 7.2 build with 29 patches, an 8128-line kernel config, and three `.panel` firmware blobs for the `panel-mipi-dpi-spi` driver, plus built-in `rtw88` and `rtl_bt` firmware. Named patches enable the LCD, PWM backlight, the ROCKNIX joypad driver, USB OTG host mode, and RGB LEDs. Nothing in it is verified on hardware.

### The closest integrated devices

| Question | Nearest evidence |
|---|---|
| 640x480 panel | RG DS already runs the product session at `640x480@60Hz` (`nix/devices/rgds/portal.nix`). |
| 1 GB RAM | R36T Max reports 993,980 kB stock (`nix/devices/r36tmax/HARDWARE.md:26`) and declares the full session, with a comment calling it "a full-stack memory candidate, not a claim that those costs have been removed" (`nix/devices/r36tmax/portal.nix:66-68`). |
| Hardware facts an integrated device supplies | `services.korriLinuxHost.label`, the runtime account, `validation.enable`, `audio.enable`, and the compositor block: `backend`, `drmDevice` by hardware path, `outputName`, `mode`, `renderDevice`, `renderer`, `localInput`/`remoteInput`; plus `services.korri.webSurfaceHost.surfaceId` and the kiosk toggle (`nix/devices/rgds/portal.nix`, `nix/devices/r36tmax/portal.nix`). |
| RTL8821CS | RG353M carries the same part. Its first native boots logged SDIO timeouts from `rtw88_8821cs`, and the module is "best effort until the SDIO path is understood" (`nix/devices/rg353m/wifi.nix:3-9`). |
| Kernel strategy | RG DS overrides `linux_latest` to a mainline 7.2 tarball with extra config and no patches (`nix/devices/rgds/kernel.nix`). Every other device carries a device-specific kernel. |

### What stays unknown

No RG35XXSP behavior has ever been observed by this project: no boot, no panel output, no input mapping, no Wi-Fi association, no audio, no battery reading, no lid event, no thermal or memory measurement. Whether the H700 has any hardware H.264 encoder is unresolved here; [ticket 13](13-decide-streaming-host-plugin-boundary.md#answer) already requires a recorded encoder result for each device, and R36T Max is the precedent for a device that records the absence as a limit.

### Round 1 with Simon, 2026-09-20

Three choices, made through `ask_user`.

**Kernel.** Simon selected the ROCKNIX-derived Linux 7.2 kernel as the kernel RG35XXSP supplies. A device-specific patched kernel is the existing house pattern: RP Mini V2 carries 37 patches, and Odin and R36T Max carry their own patches, configuration seed and board DTS. Mainline remains the preferred destination, but waiting for it does not gate adoption. The accepted cost is a 29-patch out-of-tree stack and an 8128-line configuration to re-base on every kernel bump, none of it verified on hardware. This records the choice only; landing the kernel is separate implementation work, and the unlanded `.worktree/rg35xxsp-display` tree is its starting point, not an approved change.

**Memory and panel size.** Neither the 1 GB of RAM nor the 640x480 panel is a recorded limit. Both are ordinary hardware facts. The panel size is a compositor mode, and RG DS already runs the product session at `640x480@60Hz`. The memory cost is measured during first physical acceptance, not declared in advance. The accepted cost is that a session which does not fit in 1 GB is discovered late, after the port.

**Delivery.** The `device-images.yml` and `nix-cache.yml` entries land with the implementation that makes RG35XXSP import the product module, not with this decision. Until then the device has no published image and no signed closure, so it cannot be a supported image, and all bring-up builds are local and off-device.
