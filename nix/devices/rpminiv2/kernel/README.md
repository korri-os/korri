# Retroid Pocket Mini V2 kernel

This is the SM8250 kernel from ROCKNIX distribution commit
`e81d1fc943458fb13cffe1646761e9452b29ddc1`. It is a first-boot candidate, not
hardware-verified support. The producer selects **Linux 7.2** with archive
SHA-256 `f9fef3d14c0df53819026f4be74459835c2a0b0dcbf5b5bbd9ea19f0829402b3`.

## Vendored sources

All paths below are relative to that distribution commit. Patches and device
trees are byte-for-byte copies. Downloaded bytes were checked against the Git
blob IDs in the pinned recursive Git tree before vendoring.

| Local path | Producer path |
| --- | --- |
| `patches/0000-mainline-*` | `projects/ROCKNIX/packages/linux/patches/mainline/*` |
| `patches/0000-version-*` | `projects/ROCKNIX/packages/linux/patches/7.2/*` |
| Remaining `patches/*` | `projects/ROCKNIX/devices/SM8250/patches/linux/*` |
| `dts/*` | `projects/ROCKNIX/devices/SM8250/linux/dts/qcom/*` |
| `config` | `projects/ROCKNIX/devices/SM8250/linux/linux.aarch64.conf` |

`projects/ROCKNIX/packages/linux/package.mk` and `scripts/unpack` define the
order: four mainline patches, three version patches, then 30 SM8250 patches,
with each queue sorted by filename. The prefixes preserve that order under
one sorted glob. The project package overrides `packages/linux`; its unrelated
`default` patches do not enter this queue.

No SM8550 queue was copied. The SM8250 producer itself contains patches whose
names mention other handhelds; those remain part of its exact queue. The
common `0010-msm-resource-cleanup.patch` remains enabled. Odin's hardware-specific
exclusion and local codec patches do not apply here.

The three DT files are the V2 DTS, the Mini DTS it includes, and their shared
DTSI. V2 overrides the panel compatible to `ch13726a,rpminiv2` and the touch
coordinates to 1080 by 1240. The common DTSI still includes the kernel's native
SM8250 and PM8150 family files.

## NixOS differences

As in the Odin `linuxManualConfig` package, `CONFIG_DEFAULT_HOSTNAME` becomes
`rpminiv2` and `CONFIG_INITRAMFS_SOURCE` becomes empty. NixOS supplies a separate
initrd. One diagnostic change enables `CONFIG_USB_G_SERIAL=m` for a physical
USB serial console. The native board already selects DWC3 dual-role USB.
The kernel's normal `oldconfig` step resolves the serial function dependencies
and toolchain-dependent values. USB cable operation still needs hardware
verification.

ROCKNIX's `pre_make_target` later embeds GPU/DSP blobs and signed regulatory
data. This package does not run that ROCKNIX build hook. NixOS must put the
companion firmware package's `firmwarePaths` into its initrd for the built-in
MSM and remoteproc drivers. The firmware package includes `regulatory.db` and
`regulatory.db.p7s` from `wireless-regdb` in that list as well. Firmware
availability at early probe remains a first-boot acceptance check.

No bootloader or device-writing tooling is included.
