# RG DS first-boot candidate

This is an SD-only bring-up image for the Anbernic RG DS, not a finished
Korri handheld release. It targets a Linux console on the panels and a USB
serial root console. It includes prebuilt korrid and diagnostic tools.
It does not start korrid, a compositor, or the portal automatically.
Controller mapping, dual-screen presentation, WiFi, audio, suspend, charging,
and the factory boot sequence still need hardware verification.

## Source and configuration

| Piece | Grounding |
|---|---|
| Kernel | Linux 7.2 release tarball, hash in `kernel.nix`. The RG DS DTS and Jadard panel driver are in this source. |
| U-Boot | v2026.10-rc4 archive, hash in `uboot.nix`, upstream `anbernic-rg-ds-rk3568_defconfig`. This is a release candidate. |
| Firmware | The pinned nixpkgs `rkbin.BL31_RK3568` and `rkbin.TPL_RK3568`, as consumed by the existing RG353M U-Boot package. These are binary firmware, not fully open firmware. |
| Image layout | The existing NixOS SD builder and RG353M raw-loader placement. MBR, a 16 MiB gap, loader at sector 64, FAT `NIXOS_BOOT`, bootable ext4 `NIXOS_RGDS`. |
| Root selection | The existing SD builder derives `/dev/disk/by-label/NIXOS_RGDS` from the root label. Do not install a second disk with that label. |
| USB | Linux 7.2's RG DS DTS selects peripheral mode on `usb_host0_xhci`. `g_serial` provides ACM and `serial-getty@ttyGS0` provides the console. Hardware operation is unverified. |
| Access | The shared base disables network SSH and grants root through physical consoles. This image contains no personal keys or WiFi secrets. |
| Device builds | The shared device-cache module prohibits compilation and remote build dispatch on the handheld. |

The kernel override stays inside this device directory. It does not change
`flake.lock`, the RG353M kernel, or the Odin kernel. The pinned nixpkgs common
kernel configuration predates 7.2. `kernel.nix` removes audited obsolete
symbols and selects supported replacements while retaining strict checks.
The RG DS disables the SD installer's broad `hardware.enableAllHardware`
list. That list requests obsolete PC drivers such as `pata_qdi` and caused
CI run 34538825543 to fail after the kernel compiled. Normal NixOS initrd
defaults and the device's explicit display modules remain enabled. Missing
modules still fail the build; they are never silently skipped.

Primary source files:

- https://github.com/torvalds/linux/blob/v7.2/arch/arm64/boot/dts/rockchip/rk3568-anbernic-rg-ds.dts
- https://github.com/torvalds/linux/blob/v7.2/drivers/gpu/drm/panel/panel-jadard-jd9365da-h3.c
- https://github.com/u-boot/u-boot/blob/v2026.10-rc4/configs/anbernic-rg-ds-rk3568_defconfig
- https://github.com/u-boot/u-boot/blob/v2026.10-rc4/arch/arm/dts/rk3568-anbernic-rg-ds-u-boot.dtsi
- https://github.com/u-boot/u-boot/blob/v2026.10-rc4/include/configs/rockchip-common.h

The last file lists `mmc1` before `mmc0` for U-Boot boot scanning. This does
not prove that the factory BootROM/loader will select our SD U-Boot.
If the SD does not boot, stop and inspect. Do not erase eMMC to change boot order.

## Build and download

From a clean commit, dispatch `.github/workflows/device-images.yml` with
`device=rgds` and `publish=false`. The existing ARM runner builds
`packages.aarch64-linux.rgds-sd-image`. The artifact contains the compressed
image, SHA-256 checksum, and `korri-revision.txt`. See
[`../../formats/IMAGE-BUILDS.md`](../../formats/IMAGE-BUILDS.md).

On a development machine:

```sh
nix run .#nixos-layout-check
nix run .#device-image-dist-check
nix build --no-link .#rgds-uboot
nix build --no-link .#rgds-kernel.configfile
nix run .#rgds-initrd-check
```

On x86_64, the last three commands cross-build ARM boot components. On ARM,
they build natively. They do not write media. A full kernel preflight is
`nix build --no-link .#rgds-kernel`. The initrd check uses NixOS's real module
shrinker and the configured module names against the built ARM kernel. It
uses the pinned Linux firmware package; final image assembly uses the full
configured firmware set. On a cold development store this check includes a
full kernel build. It is separate from the fast layout checks.

Verify the downloaded distribution before decompression:

```sh
nix run .#device-image-verify -- /path/to/download FULL_COMMIT_SHA
```

Every image build runs `verify-image.py` before compression. It checks the
partition table, labels, embedded U-Boot bytes, and the selected extlinux
entry's kernel, initrd, and board DTB files. `nixos-layout-check` tests this
verifier against real temporary filesystems, including corrupt images.

To repeat the structural check after decompression, use the exact U-Boot
binary from the ARM image build, not an independently cross-built binary:

```sh
nix run .#rgds-image-check -- /path/to/image.img /path/to/u-boot-rockchip.bin
```

The verifier accepts regular files only. It never mounts a filesystem or opens
a block device. It copies the root partition to temporary storage, so provide
free disk space for that copy. It checks stored bytes and references, not
kernel execution or complete filesystem integrity.

A build, checksum, or source inspection cannot prove a successful hardware boot.

## Verified build candidate

[Actions run 34554771999](https://github.com/korri-os/korri/actions/runs/34554771999)
passed for source `b26a379b4b7df630b9cece04116a30640dff0a83`.
On 2026-09-11, the downloaded checksum, source revision, compression,
partition layout, filesystem labels and geometry, and boot-file references
passed local checks. CI checked the embedded U-Boot against its native build.

The prepared image on the build machine is
`~/Downloads/korri-rgds/nixos-rgds-b26a379b4b7d.img`, 4,704,157,696 bytes.
Use an 8 GB or larger card. The compressed distribution and its metadata
remain in `~/Downloads/korri-rgds/b26a379b4b7d/`.

The code was then rebased onto newer `main` work. The artifact retains its
original source revision above; it is not an image of the later `main` tip.
The owner-authorized 63,908,610,048-byte microSD was written and verified
by reading all 4,704,157,696 image bytes back after flushing the block cache.
The read-back SHA-256 matched
`2dbc81ec130fe9944e45bfa319d0a0ac6958a290ecb2d89d4e805f94b900277b`.
Both `NIXOS_BOOT` and `NIXOS_RGDS` partitions were unmounted after verification.
The write report is `~/Downloads/korri-rgds/sdh-write-b26a379b4b7d.txt`.
No internal device storage was written. Hardware boot remains unverified.

## Arrival checklist

1. Boot stock Android without the new SD. Check both displays, both touchscreens,
   the hinge, charging, speakers, and buttons before changing the boot media.
2. Keep Android on eMMC. Use a separate microSD and a data-capable USB cable.
   Identify and approve the exact card reader before writing the image.
   There is deliberately no unattended flashing command in this directory.
3. Verify the image checksum and source revision. Write only the confirmed
   removable card with a tool that verifies the write, then safely eject it.
4. Power the RG DS off, insert the card, and power it on with the lid open.
   Record which panel lights, orientation, boot text, and any error.
5. Connect USB-C to the laptop. Look for a new ACM serial device. Open that
   device with a serial terminal. USB ACM does not use the board UART's
   1500000 baud electrical connection. Do not attach UART wires or open the case.
6. From the root console, collect the read-only evidence below. Do not run
   `nixos-rebuild` on the handheld. Future generations must arrive prebuilt.
7. To return to stock, power off and remove the SD. If Android does not return,
   stop and investigate without writing firmware or internal storage.

The USB login starts after the root filesystem mounts. It cannot diagnose a
failure that stops in the initrd. If neither a panel nor USB login appears,
keep the device unchanged and revise the SD image from the build machine.

Physical console access grants root. Do not leave the powered device connected
to an untrusted USB host. Network SSH stays disabled.

## First-boot evidence

Run these separately from the device console and save the terminal transcript
on the laptop:

```sh
uname -a
findmnt /
lsblk -o NAME,TYPE,SIZE,FSTYPE,LABEL,MOUNTPOINTS
systemctl --failed
journalctl -b -k --no-pager
journalctl -b -u serial-getty@ttyGS0 --no-pager
modetest -M rockchip -c
cat /proc/bus/input/devices
nmcli device status
```

Stop if `/` does not resolve to the SD partition labeled `NIXOS_RGDS`.
Do not format, mount writable, or install onto eMMC. The first-boot expansion
comes from the shared image format and acts on the mounted root disk.

The next hardware-backed slice is to identify connector names and orientation,
check both touch IRQs, then select one known display for the existing Korri
session. Do not infer a controller map from RG353M product names. A working
console does not establish suspend safety, battery accuracy, or audio routing.
