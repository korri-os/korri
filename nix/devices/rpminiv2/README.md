# Retroid Pocket Mini V2 first-boot candidate

This is a native NixOS SD image for the Retroid Pocket Mini V2, not a finished
Korri release. Hardware is not available yet. A successful build and image
check do not establish that the device boots.

The image starts a Linux console. It includes prebuilt korrid and diagnostic
programs, but does not start korrid, a compositor, a plugin host, or SSH.
The panel is the primary emergency console. Debug-priority kernel messages
remain visible because the vendor patch reports initrd failures at that level;
this makes boot noisier and can slow console output.
Physical consoles grant passwordless root access. Do not use this candidate
where other people have untrusted physical access. BlueZ supplies Bluetooth
diagnostics but does not automatically power on the controller.

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
Partition, an ext4 NixOS root, systemd-boot at the ARM64 removable-media path,
and an EFI-stub kernel with an explicit device tree and separate initrd.
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

The full image uses the x86-built kernel and firmware, as the Odin image does.
On an ARM-only build machine, provide those exact prebuilt outputs or an x86
builder. These commands do not flash media or activate a device.

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

The verifier checks the GPT table, partition bounds, filesystem labels,
selected EFI boot entry, nonempty kernel/initrd/loader files, actual V2 DTB
identity, and the referenced NixOS init file in the ext4 root. Its tests use
real temporary filesystems with deliberate corruption. This is stored-file
verification, not execution of the boot chain or proof of firmware loading.

## Verified before arrival

On 2026-09-15, the x86 development machine built the ARM64 kernel and the
ARM build host assembled the complete image. Neither was the handheld.

| Check | Observed result |
|---|---|
| Kernel | Linux 7.2 compiled with all 37 pinned patches, the V2 DTB, EFI stub, and `g_serial.ko`. |
| Firmware | All 25 configured paths were read from the actual gzip/newc initrd and compared byte-for-byte with the firmware package. All matched. |
| Modules | The strict module/firmware closure check passed, including USB serial and its selected dependencies. |
| Image | The complete GPT/FAT/ext4 image passed the same verifier exposed by `rpminiv2-image-check`, before compression. |
| Regression tests | All 27 real-image CLI tests passed, including malformed compatible-string boundaries and isolated missing/empty payload checks. Shared layout and distribution checks also passed. |
| Emergency console | The actual pinned stage-1 console parser selected `tty0` from the candidate parameters. This does not prove the panel or keyboard works. |
| Static checks | Nixfmt and Ruff passed on authored files. Vendor patches and device trees retain source bytes, including source whitespace. |

No boot, device activation, firmware write, or hardware test occurred.

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
4. Boot the candidate SD. It selects the V2 entry without a boot-menu choice.
   A black screen is a failed acceptance gate, not permission to change
   internal firmware.
5. Check the physical console. A USB keyboard can provide input if the panel
   works. The candidate also requests a USB serial gadget; cable role and
   enumeration remain unverified. The Linux gadget starts after root mounts;
   it is not an early-kernel console. If a serial port appears on the computer,
   confirm its identity before connecting to it.
6. Run the read-only checks below and save their output. Collect logs before
   testing suspend or making configuration changes.
7. Confirm that removing the SD and selecting Android returns to the original
   installation. Do this before treating the candidate as a usable device.

Commands on the candidate's physical console:

```sh
uname -a
tr -d '\0' </proc/device-tree/model
findmnt /
findmnt /boot
lsblk -o NAME,SIZE,TYPE,FSTYPE,LABEL,MOUNTPOINTS
systemctl --failed
journalctl -b -p warning --no-pager
ls -l /dev/dri /dev/input /sys/class/udc
modetest -M msm -c
cat /proc/bus/input/devices
nmcli device status
rfkill list
bluetoothctl show
aplay -l
cat /sys/power/mem_sleep
```

| Gate | Required hardware observation |
|---|---|
| Boot and storage | NixOS reaches a console, root and ESP are on the SD, and Android remains unchanged. |
| Display and touch | Correct V2 geometry and orientation, working brightness, and touch coordinates that match the panel. |
| USB console | The computer detects the gadget and receives the root console; unplug and reconnect do not lose access permanently. |
| Controls | Every button and both sticks emit the correct events without stuck or duplicated input. |
| Graphics | The msm render node works with hardware rendering, not only a visible software framebuffer. |
| Networking | Wi-Fi association and traffic work; Bluetooth pairing and reconnect work. |
| Audio | Speaker and headphone routes work. Suspend or reconnect does not leave audio broken. |
| Power and thermal | Battery reporting, charging, fan control, and temperatures remain safe under a bounded load. |
| Suspend | Repeated suspend/resume preserves display, input, audio, and networking; measure actual battery drain. |
| Shutdown and recovery | Shutdown completes and the factory path back to Android remains available. |

Video decoding, streaming performance, automatic Korri presentation, and
plugin installation are separate acceptance work. Do not infer them from
kernel configuration or the existence of a driver node.
