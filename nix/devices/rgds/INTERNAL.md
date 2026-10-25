# RG DS internal eMMC install

On 2026-10-25 the owner approved erasing the RG DS eMMC, including Android,
without a backup. The image was written from a running SD boot to `mmcblk0`.
The device now boots from eMMC with its root grown to 29.1G. This was a one-off,
owner-approved operation, not an unattended installer or continuing permission
to write internal storage.

## Installed image and evidence

| Item | Recorded value |
|---|---|
| Source commit | `ac3c4faa09752242f9f29bc7ded5fdb7b13a8677` |
| Local tag | `rgds-sd-2026-10-24-ac3c4fa` |
| Target | Internal eMMC, `mmcblk0`, while running from SD |
| Preparation | `wipefs` of both GPT headers before the image write |
| Image bytes verified by direct read-back | 11,402,866,688 |
| Read-back SHA-256 | `0d44afe4424532f9bf75bb951113080b3265a1ef85374cb7f924f8f46ecf2219` |
| Observed result | Boots from eMMC; root grown to 29.1G |
| Recovery-route observation | The SD also boots while the eMMC holds a system |
| Android backup | None, by owner choice |

The read-back covers the image bytes, not the later filesystem expansion.
The source revision records the installed image; it is not a claim that the
new settings in the current tree have been deployed.

## Layout and boot path

The installed source's `sd-image.nix` uses the existing NixOS SD format with an
MBR, not GPT. It embeds U-Boot at sector 64 and begins the firmware partition
at 16 MiB. The extlinux kernel, initrd, and DTB files live on the root
filesystem under `/boot`. The FAT partition is mounted at `/boot/firmware`;
it is not the extlinux payload source.

| Region on eMMC | Contents |
|---|---|
| Before the first partition | MBR and raw U-Boot image starting at sector 64 |
| Partition 1 (`mmcblk0p1`) | FAT, label `NIXOS_BOOT`, starts at 16 MiB |
| Partition 2 (`mmcblk0p2`) | Bootable ext4, label `NIXOS_RGDS`; root has grown to 29.1G |

Without SD, the installed bootloader reaches the eMMC system. SD boot with a
system already on eMMC was also observed. U-Boot's upstream Rockchip boot
scan lists `mmc1` before `mmc0`; that source fact alone would not prove the
whole boot sequence. The recorded hardware boots establish the usable SD
recovery route on this unit.

## Label-collision rule

**Do not insert the old normal-image SD while eMMC holds this system.** Both
use `NIXOS_BOOT` and `NIXOS_RGDS`. Stage 1 resolves root through
`/dev/disk/by-label/NIXOS_RGDS`. With two matching filesystems, udev can leave
that name pointing at either disk. `/boot/firmware` has the same ambiguity.
An SD boot does not by itself prove that the root came from SD.

| Image purpose | Root label | Firmware label |
|---|---|---|
| Installed normal image | `NIXOS_RGDS` | `NIXOS_BOOT` |
| RG DS rescue SD | `KORRI_RESCUE` | `KORRI_RBOOT` |

`nixosConfigurations.rgds-rescue` changes both labels and the image base name,
while retaining the complete product configuration. The image output is
`packages.aarch64-linux.rgds-rescue-sd-image`. These are the existing RG353M
rescue labels, reused deliberately. Do not introduce another disk with the
rescue labels while booting that card.

## Recovery by rescue SD

1. Build the rescue image on a development host with the configured ARM
   builders. See [README.md](README.md) for cache and verification commands.
2. Verify its source, checksum, and both rescue labels. Writing the removable
   card needs approval for the exact target. No command here writes media.
3. Power off, insert the rescue card, and boot with the lid open. Physical
   recovery consoles, including USB serial, grant root access.
4. Before any repair, inspect `findmnt /`, `findmnt /boot/firmware`, and
   `lsblk -o NAME,TYPE,SIZE,FSTYPE,LABEL,MOUNTPOINTS`. Require root and firmware
   to resolve to the rescue card with `KORRI_RESCUE` / `KORRI_RBOOT`, not eMMC.
5. If those mounts are wrong, or neither panels nor USB console is available,
   stop. Do not erase eMMC, edit partitions, or write boot firmware as an
   automatic fallback. Any internal repair needs separate approval.

The distinct-label rescue image still needs a hardware boot test. The observed
old SD boot proves a recovery route, not safe root selection with duplicate
labels. The USB getty starts after root mounts and cannot diagnose an initrd
failure. [USB-CONSOLE.md](USB-CONSOLE.md) records the earlier console repair.

System updates are separate from raw image recovery; see [UPDATE.md](UPDATE.md).
Android was not backed up. Removing the SD returns to the eMMC Korri system,
not stock Android. No tested Android restore procedure is recorded here.
