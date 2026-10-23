# Odin 2 Portal update preflight — 2026-10-22

**Historical preflight refusal, later resolved by explicit owner approval.**
See [the accepted update](odin2portal-system-update-2026-10-22.md) for the new
one-run scope and result. At the preflight recorded below, no update write,
closure copy, profile change, activation, bundle switch, reboot, firmware write
or lock-state change had run on the Odin. The
proposed SD-only update does not match the live device. The parent is asking
the owner for a corrected scope. `bench` still owns the device.

## Verified live layout

Read-only SSH endpoint: `root@10.42.4.1:2222`. Observation time:
`2026-10-22T23:59:14+00:00`. Board DT model: `AYN Odin 2 Portal`.

| Item | Observed value |
|---|---|
| Booted/system profile | `/nix/store/bv325b0lc8jsjv6r85hgd73mblg2ll4h-nixos-system-odin2portal-sd-card-26.11.20260923.4975466` |
| `/boot` | `/dev/sda18`, vfat, label `ODIN2P_ESP`, 1,073,741,824 bytes (1 GiB) |
| `/` | `/dev/sda19`, ext4, label `ODIN2P_ROOT`, 105,813,880,832 bytes |
| Parent disk | `/dev/sda`, 114,907,152,384 bytes, non-removable (`RM=0`) |
| UFS model | `THGJFJT0E25BAIPC` |
| Disk sysfs device | `/sys/devices/platform/soc@0/1d84000.ufshc/host0/target0:0:0/0:0:0:0` |
| Active service bundle | `/nix/store/lwsp0ys0vyyb2ygn0dbm3d4nbcvbw0nh-korri-bundle-0.0.0` |
| SD host | `mmc0`, `sdhci_msm`, `/soc@0/mmc@8804000` |
| SD detection | `/sys/bus/mmc/devices` is empty; no `mmcblk*` block devices. No SD card is detected by Linux. Physical insertion is not independently verified. |

The UFS controller sysfs path proves that these labels refer to **internal
UFS**, not an SD. Other enumerated disks `sdb` through `sdh` have the same UFS
model. The initial instruction explicitly forbids `/dev/sda18` and
`/dev/sda19`. Labels alone cannot authorize writes to them.

## Boot path evidence

Read-only `bootctl status` reports:

- Firmware `UEFI 2.100 (Das U-Boot 8229.256)`, ARM64; Secure Boot disabled.
  This is not proof of the Android bootloader's lock state.
- Current loader `systemd-boot 261.2`, image `/EFI/BOOT/BOOTAA64.EFI`.
- Loader partition PARTUUID `16dd55a8-df9f-44a4-871c-5646430798e8`, which
  resolves to `/dev/sda18`.
- Current/default entry
  `nixos-47510337292c3a798c7ba33e26a847abb00e1d8782000c01d59b57cc4cc31ea9.conf`,
  generation 7, with `init=` pointing to the booted `bv325b0...` system.
- `BootCurrent` EFI variable bytes after the attribute header are `01 00`
  (`Boot0001`). Kernel log says `EFI v2.10 by Das U-Boot` and mounts `sda19`.

Thus the observed path is U-Boot's EFI boot manager → systemd-boot on the
internal UFS ESP → kernel/initrd → internal UFS root. The repository's
`sd-image.nix` comments place AYN U-Boot in `loader_a` and describe
`scsi scan; usb start` / `bootefi bootmgr`. That early-loader location is
source documentation, not independently read back from firmware here.

## Repository documentation search

`git grep` for `ODIN2P_ESP`, `sda18`, `sda19`, and Odin near `internal`/`UFS`
found no internal-install procedure or partition-layout record:

- `AGENTS.md:176` is the explicit exception: writes to `sda18`/`sda19` need
  owner approval **per run** and must never be an automated path.
- `nix/devices/odin2portal/sd-image.nix` and `module-check.nix` declare/check
  the labels, but describe an SD-only image. Neither proves the live medium.
- `clients/linux/chromium/README.md:105` warns against using internal Odin
  storage for build capacity.
- No README exists under `nix/devices/odin2portal/` at this revision.

The configuration enables systemd-boot, `configurationLimit = 3`, and
`efi.canTouchEfiVariables = false`. The generated boot builder explicitly
sets `CAN_TOUCH_EFI_VARIABLES = false` and passes `--variables=no`. A standard
boot-only profile/update procedure was approved in principle by the parent,
but **not run** because its SD precondition failed.

## Host-only work

The candidate source is the Mini's same core main commit
`4e34b8a9ed859560acac59c218c7af872221d3ed`. A build was requested only on Zao's
Nix daemon/configured builders. Evaluated candidate paths (not deployed):

```text
/nix/store/xqbqdwrn3ws25z9dpnhb3nwl47366m8q-nixos-system-odin2portal-sd-card-26.11.20260923.4975466
/nix/store/983r04y6kbmmygj6gwqi3f9hbniqzyaz-korri-bundle-0.0.0
```

At this point the task awaited owner scope correction and the parent's
separate bench-idle go. Both later arrived for an explicitly bounded UFS file
update; the accepted run is linked at the top. Labels alone did not grant it.
