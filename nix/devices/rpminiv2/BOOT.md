# Mini V2 boot audit

## Decision

Use a device-specific EFI SD image and do not replace internal firmware. The
installed Retroid U-Boot has now booted both official ROCKNIX and the NixOS SD
on the target Mini V2. The accepted NixOS handoff uses the exact ROCKNIX GRUB
EFI binary with a hardware-proven GCC 15.2 source kernel and byte-identical V2
DTB, while keeping Android and the installed loader intact.

## Verified source evidence

### Retroid's published loader

The official ROCKNIX Mini wiki links Retroid's `rp-v1.0.1` loader release.
The corresponding RetroidPocket/u-boot commit is
`d1c4eba0c495c3c4a32924efe0f590a4759aa6d5`.

Its [`board/qualcomm/retroidpocket.config`](https://github.com/RetroidPocket/u-boot/blob/d1c4eba0c495c3c4a32924efe0f590a4759aa6d5/board/qualcomm/retroidpocket.config)
selects `board/qualcomm/retroidpocket.env` as the default environment.
That [environment](https://github.com/RetroidPocket/u-boot/blob/d1c4eba0c495c3c4a32924efe0f590a4759aa6d5/board/qualcomm/retroidpocket.env)
runs `scsi scan; usb start` before `bootefi bootmgr`, and falls back to its
menu if EFI boot fails. It also exposes an optional USB serial console in
that menu. These are source observations, not operations executed here.

The [published V2 loader DTS](https://github.com/RetroidPocket/u-boot/blob/d1c4eba0c495c3c4a32924efe0f590a4759aa6d5/dts/upstream/src/arm64/qcom/sm8250-retroidpocket-rpminiv2.dts)
describes a 1080 by 1240 simple-framebuffer, stride 4320, and rotation 1.
This supports the EFI candidate's design. It does not identify the firmware
installed on an unopened unit. We do not ship or install this loader.

### The Imager workaround

The source inspected is ROCKNIX/ImageBurner commit
`9c88ce055b3f1f505940c89c78f087ce9b4866b9`.
[`ImageFetcher.cs`](https://github.com/ROCKNIX/ImageBurner/blob/9c88ce055b3f1f505940c89c78f087ce9b4866b9/Services/ImageFetcher.cs)
reads the image list at `https://releases.rocknix.org/imageburner`.
The Mini V2 records observed on 2026-09-15 specify `post_install=grubenv` and
`dtb=rpminiv2` for both stable 20260901 and nightly 20260914.

[`ImageWriter.cs`](https://github.com/ROCKNIX/ImageBurner/blob/9c88ce055b3f1f505940c89c78f087ce9b4866b9/Services/ImageWriter.cs)
implements that action by writing a 1024-byte `boot/grub/grubenv` with
`saved_entry=rpminiv2` on a mounted volume labelled `ROCKNIX`. That action
selects a GRUB entry. It does not reflash a Retroid loader, change panel
geometry, or repair firmware graphics initialization.

This does not certify the Imager as safe for our procedure. Its raw writer
is destructive to the selected disk, and its post-write code finds a volume
by label rather than binding it to that disk. We do not run or package it.
Our own image contains only one board's selected boot entry.

### Linux's early framebuffer remains different

ROCKNIX/distribution commit `e81d1fc943458fb13cffe1646761e9452b29ddc1` supplies
our kernel, config and device trees. Its
[V2 DTS](https://github.com/ROCKNIX/distribution/blob/e81d1fc943458fb13cffe1646761e9452b29ddc1/projects/ROCKNIX/devices/SM8250/linux/dts/qcom/sm8250-retroidpocket-rpminiv2.dts)
inherits the original Mini's 960 by 1280 simple-framebuffer and stride 3840.
The V2 overrides only model/compatible, touch coordinates, and panel
compatible. The native panel driver has the correct 1080 by 1240 mode.

We preserve these source bytes instead of inventing a framebuffer correction
without the device. The explicit V2 boot entry prevents selection of the
wrong board. It does not fix the inherited early framebuffer or prove that
native DRM takeover recovers a faulty firmware display state.
`video=efifb:off` is retained from ROCKNIX's SM8250 options. It disables efifb,
not every possible simple-framebuffer driver.

### Only the proven removable-media handoff is copied

The NixOS image extracts the checksum-pinned `EFI/BOOT/bootaa64.efi` and GRUB
font from official ROCKNIX `20260901`. Its DTB and GRUB configuration are also
retained as checksum-pinned controls, but the boot kernel and DTB are built from
the vendored Linux source, patch queue and board files. It does not copy
`rocknix_abl`, first-boot scripts, `SYSTEM`, an internal installer or
firmware-flashing tools. The GRUB configuration is generated locally with one
NixOS entry and a separate NixOS initrd/root. This reproduces the accepted SD
handoff without replacing the device's installed loader.

## Safety boundary and hardware evidence

Korri configures no internal block-device writes. GRUB starts only from the
SD's removable-media EFI path and does not update EFI variables. The native
NixOS installer is disabled, and no Retroid flashing tools or internal firmware
payloads are included. SD root selection is explicit. The normal first-boot
expansion changes only the disk backing that root.

Hardware testing established that the installed loader reaches
`EFI/BOOT/BOOTAA64.EFI`, GRUB initializes the correct rotated GOP mode, the
separate NixOS initrd mounts the SD root, native MSM DRM enables DSI at 1080 by
1240, and USB `0525:a4a7` provides `ttyGS0`. The source kernel requires GCC 15.2;
the otherwise matching GCC 14.3 build failed display takeover. Binutils 2.44 is
hardware-proven and does not need to match ROCKNIX's 2.47. A visible GRUB menu alone was not
accepted as display proof; the native framebuffer was unblanked and VT1 text
was observed directly. The factory route back to Android must remain part of
release acceptance after any future SD-image change.

The [ROCKNIX wiki](https://rocknix.org/devices/retroid/retroid-pocket-mini/)
documents a loader replacement for affected V2 units. That is an internal
firmware write and is outside this procedure. A boot failure does not authorize
it. No bootloader unlocking, relocking, flashing, or device writes occurred
during this audit.
