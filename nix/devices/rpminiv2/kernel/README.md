# Retroid Pocket Mini V2 kernel

This is the SM8250 kernel from ROCKNIX distribution commit
`e81d1fc943458fb13cffe1646761e9452b29ddc1`. It is a first-boot candidate, not
hardware-verified support. The producer selects **Linux 7.2** with archive
SHA-256 `f9fef3d14c0df53819026f4be74459835c2a0b0dcbf5b5bbd9ea19f0829402b3`.

## Vendored sources

All paths below are relative to that distribution commit. Patches and device
trees are byte-for-byte copies. Downloaded bytes were checked against the Git
blob IDs in the pinned recursive Git tree before vendoring. The kernel config
uses the ROCKNIX SM8250 config as its seed, then removes hardware and services
outside the first-boot TTY target.

| Local path | Producer path |
| --- | --- |
| `patches/0000-mainline-*` | `projects/ROCKNIX/packages/linux/patches/mainline/*` |
| `patches/0000-version-*` | `projects/ROCKNIX/packages/linux/patches/7.2/*` |
| Remaining `patches/*` | `projects/ROCKNIX/devices/SM8250/patches/linux/*` |
| `dts/*` | `projects/ROCKNIX/devices/SM8250/linux/dts/qcom/*` |
| `config` seed | `projects/ROCKNIX/devices/SM8250/linux/linux.aarch64.conf` |

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

The checked-in config is a TTY-only cut of the ROCKNIX seed. It retains the
SM8250 platform, PMIC, clocks, regulators, interconnect, SD storage, EFI stub,
gzip initrd, ext4 root, VFAT boot filesystem, MSM DPU and DSI, the Mini V2
CH13726A panel, framebuffer console, USB HID and Qualcomm serial console. It
also retains the MSM DisplayPort component because the SM8250 device tree
keeps that component enabled and the DRM master waits for it before binding.
`CONFIG_USB_G_SERIAL=m` and its four dependencies are the only modules.

The cut removes 747 enabled settings from the 2,317-setting seed. It removes
ACPI, PCI, wireless, Bluetooth, sound, media, controls, unrelated filesystems,
network filtering, virtualization and the unused MSM display generations.
`module-check.nix` holds the required and excluded configuration contract.
The kernel's normal `oldconfig` step still resolves toolchain-dependent values.

`CONFIG_DEFAULT_HOSTNAME` is `rpminiv2`, and `CONFIG_INITRAMFS_SOURCE` is empty.
NixOS supplies a separate initrd. The native board already selects DWC3
dual-role USB. The first hardware boot enumerated `g_serial` as USB
`0525:a4a7` and provided a root shell through `/dev/ttyACM0` on the host.

ROCKNIX's `pre_make_target` embeds GPU and DSP blobs plus signed regulatory
data. This package instead puts the companion firmware package's selected
paths into the NixOS initrd. Firmware availability at early probe remains a
first-boot acceptance check.

No bootloader or device-writing tooling is included.
