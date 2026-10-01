# Mini V2 internal UFS install

On 2026-10-01 the owner's Mini V2 moved from the SD card to internal UFS. Android
was removed by owner choice. The Qualcomm boot chain and the Retroid loader were
not changed. This file records what was written, what was kept, and how to
recover. The install was a one-off operation on one unit. The repository has no
automated internal installer.

## Boot path after the install

Retroid U-Boot (`loader_a`/`loader_b`, LUN 4) runs `bootefi bootmgr`. It lists
`mmc 0` (SD) first and `scsi 0` to `scsi 5` (UFS LUNs) after it. With no SD,
the firmware selects `Boot0001` (`scsi 0`, LUN 0), finds the ESP, and runs the
same ROCKNIX GRUB removable-media binary as the SD image. GRUB boots the
product kernel. Stage 1 mounts the root by label `NIXOS_RPMINIV2`. Verified on
the device: `BootCurrent=Boot0001`, root `/dev/sda22`, `/boot` `/dev/sda21`, no
SD present, no failed units.

The product kernel builds the UFS host driver in (`config-korri.delta`). The
UFS BSG node stays off, so userspace cannot send UFS query requests, such as a
boot-LUN change. A udev rule in `portal.nix` marks every block device on LUNs 1
to 5 read-only when it appears. This prevents accidental writes. It does not
stop root from clearing the flag.

## UFS layout

Phison eUFS3.1 128 GB. The device is 4096-byte-sector, boot LUN B
(`bBootLunEn=2`).

| LUN | Contents | Install action |
|---|---|---|
| 0 | `ssd` ... `vbmeta_system_b` (#1-#20): persist, metadata, misc, keystore, frp, nvdata | kept, hashed before and after |
| 0 | #21 `super`, #22 `rawdump`, #23 `userdata` (Android) | deleted |
| 0 | new #21 ESP `RPMINIV2`, FAT32, 512 MiB, blocks 113152+131072 | created |
| 0 | new #22 root `NIXOS_RPMINIV2`, ext4, 116 GiB, blocks 244224+30469627 | created |
| 1, 2 | `xbl`, `xbl_config` (A and B) | never written |
| 3 | `cdt`, `ddr`, `mdmddr` | never written |
| 4 | `abl`, `loader`, `tz`, `hyp`, `aop`, `devcfg`, `uefi*`, `boot`, `recovery`, `devinfo`, ... | never written |
| 5 | `modemst1`, `modemst2`, `fsg`, `fsc` | never written |

The GPT keeps its factory label ID, entry count (32) and first and last usable
blocks. `sfdisk` rewrites protective-MBR fields, so the install wrote the
shipped block 0 back. `sgdisk -v` reports the same gap and table-size warnings
on the factory table and on the new table.

`uefivarstore` (LUN 4) changes on every boot. The firmware owns it.

## Evidence and backup

The backup is on Zao at `~/rpminiv2-ufs-backup-20261001/`. Each region was
hashed on the device and again on Zao.

- `lun0-head.img`: LUN 0 blocks 0 to 112935 (GPT and partitions #1-#20).
- `lun0-tail-gpt.img`: LUN 0 backup GPT.
- `lun1.img` to `lun5.img` (LUN 4 compressed): whole LUNs.
- `partition-map.txt`, `lun0-after-install.sfdisk`, `SHA256SUMS`.
- `scripts/`: the probe, backup, rehearsal, install and verification scripts.
  They pin this unit's UFS serial and its factory GPT. Do not run them on
  another unit.

`super`, `rawdump` and Android `userdata` were not backed up, by owner choice.
No published Mini V2 stock firmware package was found. TheGammaSqueeze's
repository has packages for the original Mini, RP5, Flip 2 and Classic only.

## What happened during the install

1. The first write run changed the GPT, then stopped. `partx -u` cannot add a
   partition over a stale kernel entry. The script now removes the old kernel
   entries and adds the new ones. The resumed run checked that the GPT was
   exactly the planned result and that every protected hash still matched.
2. The closure check found that the SD card returned unstable reads for one
   store file, the kernel `Image`. About 200 of its 13,975 blocks differed on
   each read. The internal copy was replaced with Zao's verified build, and
   the full system closure then verified. Treat that SD card as failing.

## Recovery

- **Normal fallback:** U-Boot tries the SD before UFS. An SD product or
  recovery image boots even with the internal install present. Do not insert
  an SD that uses the `RPMINIV2` and `NIXOS_RPMINIV2` labels. Stage 1 would
  find two roots with one label. A rescue card needs distinct labels.
- **Restore the factory LUN 0 table:** write `lun0-head.img` blocks 0 to 5 and
  `lun0-tail-gpt.img` back from an SD-booted system. This restores the table
  only. Android also needs a `super` image and a factory reset.
- **Boot chain:** not changed. EDL (Power + Volume Up + Volume Down, Qualcomm
  9008) and the `prog_ufs_firehose_sm8250_lite_lp5.elf` programmer are reported
  for the Mini and RP5. Neither is tested on this unit.
