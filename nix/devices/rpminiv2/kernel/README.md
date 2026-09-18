# Retroid Pocket Mini V2 kernel

This is the SM8250 kernel from ROCKNIX distribution commit
`e81d1fc943458fb13cffe1646761e9452b29ddc1`. The producer selects **Linux 7.2**
with archive SHA-256
`f9fef3d14c0df53819026f4be74459835c2a0b0dcbf5b5bbd9ea19f0829402b3`.
The module build configuration is anchored to a working Mini V2 boot of
official ROCKNIX release `20260901`, rather than the earlier speculative
TTY-only cut. The boot `Image` and DTB are now exact files from that release's
hardware-proven image; the source build remains responsible for compatible
modules while its binary delta is investigated.

## Vendored sources

All source paths below are relative to that distribution commit. Patches and
device trees are byte-for-byte copies. Downloaded bytes were checked against
the Git blob IDs in the pinned recursive Git tree before vendoring. The stable
`20260901` release uses the same Linux archive and Mini V2 device-tree sources.

| Local path | Producer path |
| --- | --- |
| `patches/0000-mainline-*` | `projects/ROCKNIX/packages/linux/patches/mainline/*` |
| `patches/0000-version-*` | `projects/ROCKNIX/packages/linux/patches/7.2/*` |
| Remaining `patches/*` | `projects/ROCKNIX/devices/SM8250/patches/linux/*` |
| `dts/sm8250-retroidpocket-common.dtsi` and board DTS files | `projects/ROCKNIX/devices/SM8250/linux/dts/qcom/*` |
| `config` baseline | `/proc/config.gz` from official ROCKNIX `20260901` running on the target Mini V2 |

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

The checked-in config starts from the complete configuration exported by the
working ROCKNIX `20260901` kernel. That boot bound DSI, DisplayPort and the
Adreno GPU before registering DRM and `fb0`. It also showed the same initial
`DSI PLL(0) lock failed` warning as the failed NixOS image, proving that warning
is recoverable and is not by itself the display failure.

The Nix source build changes `CONFIG_DEFAULT_HOSTNAME`, uses an external initrd,
and builds `CONFIG_USB_G_SERIAL=m` plus its four selected function modules for
the recovery console. Toolchain-generated values also differ: the accepted
ROCKNIX binary reports GCC 15.2 and Binutils 2.47, while the failed Nix binary
used GCC 14.3 and Binutils 2.44. Hardware testing showed that restoring the full
config, embedded firmware and byte-identical DTB was insufficient: the
source-built binary remained black under the same GRUB handoff.

Booting the exact ROCKNIX `KERNEL` with the unchanged NixOS initrd/root produced
a working native TTY, DRM and `fb0`. The Nix-built modules loaded cleanly under
it, including `g_serial`, with matching `7.2.0` vermagic and no unknown-symbol
errors. `default.nix` therefore replaces only the installed boot `Image` and
DTB with checksum-verified release artifacts while retaining the audited source
build for modules. `module-check.nix` holds that temporary boundary while the
binary/toolchain delta is investigated.

This full baseline temporarily restores hardware unrelated to the final TTY
scope. That is deliberate: first establish parity with the working display,
then remove feature groups in measured, hardware-tested steps. The kernel's
normal `oldconfig` step still resolves toolchain-dependent values.

Like ROCKNIX's `pre_make_target`, this package exposes the pinned firmware
subset as `external-firmware` so the configured GPU and DSP blobs plus signed
regulatory data are embedded in the kernel. The companion firmware is also
available to the separate NixOS initrd and root system for symlinks, service
manifests and module-time requests.

The companion `rocknix-baseline` package also extracts the exact GRUB EFI
binary and font used by the accepted handoff. It includes no device-writing
tooling and does not modify the installed Retroid loader or internal storage.
