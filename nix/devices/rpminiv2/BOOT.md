# Mini V2 boot audit

## Decision

Prepare a device-specific EFI SD image. Do not replace firmware. The evidence
supports a first-boot candidate, not a claim that the arriving unit's loader
will boot it. If boot fails, stop and preserve Android.

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

### Current ROCKNIX boot packaging is not copied wholesale

The pinned distribution's `projects/ROCKNIX/config.xml` selects the `abl`
image action for SM8250. `projects/ROCKNIX/bootloader/mkimage` implements it by
copying a `rocknix_abl` payload onto the image. Our NixOS image does not contain
that payload or ROCKNIX's first-boot scripts. Kernel support and boot packaging
are separate choices. The local Odin EFI producer is the format reference.

## Safety boundary and unresolved tests

Korri configures no internal block-device writes. Bootloader installation
and updates use bootctl's `--no-variables` option. That setting does not
disable systemd-boot's runtime EFI-variable operations. Their persistence
depends on the shipped firmware and remains unverified.
The native NixOS installer is disabled, and no Retroid flashing tools or
firmware payloads are included. SD root selection is explicit. The normal
first-boot expansion changes only the disk backing that root.

The following still need the actual unit:

- Establish its Android build and loader version before changing anything.
- Confirm that its existing loader reaches the SD's `EFI/BOOT/BOOTAA64.EFI`.
- Confirm that systemd-boot can pass the V2 DTB and separate initrd to Linux.
- Observe firmware graphics, Linux early framebuffer, and native DRM takeover
  separately. A visible boot menu is not proof of a working native display.
- Check USB role switching and the Linux serial gadget independently of the
  panel. The optional loader USB console is also unverified on this unit.
- Confirm the factory route back to Android before further hardware tests.

The [ROCKNIX wiki](https://rocknix.org/devices/retroid/retroid-pocket-mini/)
documents a loader replacement for affected V2 units. That is an internal
firmware write and is outside this procedure. A boot failure does not authorize
it. No bootloader unlocking, relocking, flashing, or device writes occurred
during this audit.
