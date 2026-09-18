# Retroid Pocket Mini V2 first-boot candidate

This is a native NixOS SD image for the Retroid Pocket Mini V2, not a finished
Korri release. Hardware testing reached a working 1080 by 1240 built-in TTY and
a root shell over USB serial while preserving Android and the installed loader.

The image targets one result: a Linux TTY on the built-in panel. It does not
start korrid, a compositor, a plugin host, or SSH. Userspace remains minimal.
The kernel and DTB build from the audited Linux 7.2 source and ROCKNIX patch
queue. Hardware testing isolated the earlier black-screen build to GCC 14.3;
the GCC 15.2 source build works with Binutils 2.44. The primary configuration is
the hardware-proven TTY trim: an 18.4 MB kernel plus five USB serial modules.
The panel is the primary emergency console. USB serial becomes the recovery
shell after root mounts. The framebuffer console blanks
after one idle minute to protect
the OLED; keyboard input wakes it. Physical consoles grant passwordless root
access. Do not use this candidate where other people have untrusted physical
access.

The [boot audit](BOOT.md) verifies Retroid's published EFI loader source and
the Imager's GRUB-entry workaround. It does not identify your unit's shipped
loader. The inherited Linux early framebuffer still differs from the V2 panel;
a fixed board selection does not repair that display description.

## Scope and sources

The device identity `rpminiv2` comes from ROCKNIX's
`sm8250-retroidpocket-rpminiv2.dts`. It inherits the original Mini device tree
and the common SM8250 Retroid tree. The V2 selects its own `ch13726a,rpminiv2`
panel and 1080 by 1240 touch coordinates. Do not substitute an original Mini,
Pocket 5, or Odin device tree.

Kernel source, patch order, configuration changes, and firmware provenance
are recorded next to their packages in `kernel/` and `firmware/`.

The image format follows `../odin2portal/sd-image.nix`: GPT, a FAT EFI System
Partition, and an ext4 NixOS root. The ARM64 removable-media path contains the
exact ROCKNIX GRUB/GOP loader. Its one active entry loads the source-built
kernel, the explicit V2 device tree and the separate NixOS initrd. Inactive systemd-boot and
Boot Loader Specification files remain available for offline inspection.
The filesystem labels are `RPMINIV2` and `NIXOS_RPMINIV2`. The board identity
fits the FAT label limit; these labels are selected explicitly, not inferred
from internal disk numbering. Do not attach another disk with either label.

The image contains no U-Boot image, ABL replacement, Android boot image,
firmware-flashing utility, or internal-storage installer. The builder writes
only its output image file. First boot expands the root partition on the
selected SD, using the existing shared SD expansion code. Automatic systemd
GPT discovery is disabled. Android partitions are not configured as mounts
or swap.

A source audit found an existing shared first-boot defect: the expansion hook
removes the registration marker before NixOS creates the initial system
profile. Keep the original SD image for rollback. Do not rely on generation
history to retain the shipped system until that separate shared fix lands.

## Build on a development machine

Run from this repository on a Linux builder. The x86 kernel output cross-builds
ARM code. Full image assembly also needs an ARM builder; the handheld must
never build software or dispatch builds.

```sh
nix run .#rpminiv2-check
nix build --no-link .#rpminiv2-kernel
nix run .#rpminiv2-initrd-check
nix build --no-link .#packages.aarch64-linux.rpminiv2-sd-image
```

The full image uses an x86-cross-built kernel, modules and firmware. The GRUB
EFI binary and font come from the pinned 1.4 GB official ROCKNIX image; its DTB
and GRUB configuration remain checksum-pinned controls. On an ARM-only build
machine, provide those exact baseline outputs or an x86 builder. These commands
do not flash media or activate a device. The extracted third-party image files
are kept local until their redistribution terms have been reviewed.

Inputs are pinned, but the inherited GPT conversion generates fresh disk and
partition GUIDs. Clean image rebuilds are not byte-identical. Use the checksum
that accompanies the exact artifact, not a checksum from another build.

For a revision-bound download, use the existing staging tool from a clean
checkout. The destination must not exist:

```sh
nix run .#device-image-dist -- packages.aarch64-linux.rpminiv2-sd-image /tmp/rpminiv2-dist
nix run .#device-image-verify -- /tmp/rpminiv2-dist "$(git rev-parse HEAD)"
```

Image assembly runs `verify-image.py` before compression. To inspect a copy
of the uncompressed image without mounting it:

```sh
nix run .#rpminiv2-image-check -- /path/to/nixos-rpminiv2.img
```

The verifier checks the GPT table, partition bounds, filesystem labels, exact
hardware-proven loader/font and source-built kernel/DTB hashes, the ordered GRUB/GOP setup, the
active menu's agreement with retained boot metadata, the initrd, and the
referenced NixOS init file in the ext4 root. Its tests use real temporary
filesystems with deliberate corruption. This is stored-file verification, not
execution of the boot chain or proof of firmware loading.

## Verified before arrival

On 2026-09-15, the x86 development machine built the ARM64 kernel and the
ARM build host assembled the complete image. Neither was the handheld.

| Check | Observed result |
|---|---|
| Kernel | Linux 7.2 compiled with all 37 pinned patches, the V2 DTB, EFI stub, and `g_serial.ko`. |
| Firmware | All 25 configured paths were read from the actual gzip/newc initrd and compared byte-for-byte with the firmware package. All matched. |
| Modules | The strict module/firmware closure check passed, including USB serial and its selected dependencies. |
| Image | The complete GPT/FAT/ext4 image passed the same verifier exposed by `rpminiv2-image-check`, before compression. |
| Regression tests | All 31 real-image CLI tests passed, including malformed compatible-string boundaries and isolated missing/empty payload checks. The shared layout check remains blocked by an existing graphics-seat assertion that includes another first-boot target. |
| Emergency console | The actual pinned stage-1 console parser selected `tty0` from the candidate parameters. This does not prove the panel or keyboard works. |
| Static checks | Nixfmt and Ruff passed on authored files. Vendor patches and device trees retain source bytes, including source whitespace. |

No boot, device activation, firmware write, or hardware test occurred before
arrival.

## First hardware boot

On 2026-09-18, the shipped Retroid loader selected the SD at `mmc0`. Linux 7.2
identified the board as `Retroid Pocket Mini V2`, mounted root from
`/dev/mmcblk0p2`, and mounted the ESP from `/dev/mmcblk0p1`. The host enumerated
USB `0525:a4a7` as `Gadget Serial v2.4`, made it available as `/dev/ttyACM0`,
and received a root shell.

The first trimmed kernel left the panel black and never registered DRM or a
framebuffer. A DisplayPort-enabled follow-up and a DSI-only device-tree
experiment also stayed black; neither produced a usable USB diagnostic session.
A later Nix-built kernel restored the complete runtime configuration, embedded
firmware and byte-identical DTB, but remained black under both systemd-boot and
the working ROCKNIX GRUB handoff.

Official ROCKNIX `20260901` was then booted on the same unit. It selected
`Retroid Pocket Mini V2`, used the unmodified vendor DTB, and registered DSI,
DisplayPort, the Adreno GPU, DRM and `fb0`. Its log contains the same initial
`DSI PLL(0) lock failed` and clock warnings as NixOS, but recovers about 230 ms
later and creates the framebuffer. The warning is therefore not a sufficient
failure diagnosis. A GRUB matrix then booted the exact ROCKNIX `KERNEL` with the unchanged NixOS
initrd and root. NixOS reached multi-user userspace and USB serial. Native MSM
DRM bound DSI, DisplayPort and Adreno; DSI was connected and enabled at 1080 by
1240, and `/dev/dri/card0` plus `msmdrmfb` `/dev/fb0` appeared. The panel's
apparent black state after one minute was the intentional `consoleblank=60`
timeout. Forcing `fb0` awake displayed the VT1 message on the OLED. Nix-built
modules, including `g_serial`, loaded under that exact kernel without version or
symbol errors. A full source rebuild with GCC 15.2 and Binutils 2.44 then reached
the same working DSI mode, DRM framebuffer, VT1 and USB serial state. This
isolated GCC 14.3 as the failing build delta; matching ROCKNIX's Binutils 2.47
was unnecessary. The aggressively trimmed GCC 15.2 kernel subsequently booted
to a visible built-in bash prompt. Its earlier apparent black state was the
intentional `consoleblank=60` timeout.

No internal partition, loader or Android file was changed. The existing U-Boot
correctly identified the Mini V2 and GRUB saved the `rpminiv2` entry. All image
writes targeted only the removable SD.

## Arrival checks

Do not flash an internal partition, erase Android, unlock or relock a
bootloader, or run an automatic repair script as part of this procedure.
If the SD does not boot, stop and collect evidence. Recovery must not require
opening the case.

1. Record the device model, Android build, and bootloader version before any
   update. Confirm that the device is the Mini V2, not the original Mini.
2. Check the SD image checksum and source revision. Identify a spare SD by
   capacity and hardware identity before writing it on a separate machine.
   Writing the image destroys the previous contents of that SD.
3. Establish the factory procedure for entering Loader mode and returning to
   Android. Use the existing loader; do not follow a loader-flashing guide.
4. Boot the candidate SD. ROCKNIX GRUB displays its single NixOS entry for two
   seconds, then boots it automatically. A black screen before the 60-second
   idle blanking deadline is a failed acceptance gate, not permission to change
   internal firmware.
5. Check the physical console. A USB keyboard can provide input if the panel
   works. The Linux serial gadget and its getty start after root mounts. The
   first unit enumerated it as USB `0525:a4a7` and
   `/dev/ttyACM0`. Confirm that identity before connecting to it.
6. Run the read-only checks below and save their output.
7. Confirm that removing the SD and selecting Android returns to the original
   installation. Do this before treating the candidate as a usable TTY image.

Commands on the candidate's physical console:

```sh
uname -a
tr -d '\0' </proc/device-tree/model
findmnt /
findmnt /boot
lsblk -o NAME,SIZE,TYPE,FSTYPE,LABEL,MOUNTPOINTS
systemctl --failed
journalctl -b -p warning --no-pager
ls -l /dev/dri /sys/class/udc
modetest -M msm -c
```

| Gate | Required hardware observation |
|---|---|
| Boot and storage | NixOS reaches a console, root and ESP are on the SD, and Android remains unchanged. |
| Display | The TTY uses the correct V2 geometry and orientation, and text remains readable during boot. |
| USB console | The computer detects the gadget and receives the root console; unplug and reconnect do not lose access permanently. |
| Shutdown and recovery | Shutdown completes and the factory path back to Android remains available. |

Touch, controls, hardware rendering, networking, Bluetooth, audio, media,
suspend and Korri are outside this first-boot cut. Their kernel support must be
added and accepted separately after the TTY milestone.
