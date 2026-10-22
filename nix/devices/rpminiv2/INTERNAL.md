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

## System updates from Zao

Use the build-host app, not `nixos-rebuild` or `switch-to-configuration boot`.
GRUB installation is disabled. Those commands do not update this ESP's active
`boot/grub/grub.cfg`. `esp.nix` now produces the same firmware tree for image
assembly and this updater: store-hash filenames, kernel parameters, GRUB/GOP
setup, DTB, font, loader and inactive BLS files have one producer.

Coordinate exclusive use and get approval for each device update. The owner's
2026-10-22 approval covers one Mini V2 update only. The Mini V2 is a development
device; this operation does not preserve user data or copy backups to Zao.
It retains only the on-device boot rollback described below.

From the approved checkout on **Zao**:

```sh
nix run .#rpminiv2-update-check       # offline; no handheld connection
nix run .#rpminiv2-update -- --status # read-only; SSH root@10.42.3.1:2222
# Copy the exact current= path from status after checking the intended device.
nix run .#rpminiv2-update -- --expected-system /nix/store/EXACT-CURRENT-SYSTEM
```

The command builds `nixosConfigurations.rpminiv2.config.system.build.toplevel`
and `system.build.rpminiv2Firmware` on the host's Nix daemon and configured
builders (fuji/yari for ARM). It signs the system closure with
`nix store sign --recursive --key-file ~/.config/korri/cache-key.secret` on Zao.
The private key must belong to the current user, have no group/other access,
and resolve to the device-trusted `korri-cache-zao-1` identity. It is never
printed or copied. `nix copy` transfers the prebuilt closure over SSH. Signature
requirements stay enabled. No command builds or dispatches builds on the device.

The tool refuses any board other than `Retroid Pocket Mini V2`, or mounts other
than writable `/boot` = `/dev/sda21`, vfat, `RPMINIV2`, and `/` = `/dev/sda22`,
ext4, `NIXOS_RPMINIV2`. It rejects nested ESP mounts and symlink boot directories.
It checks the device's signature/build policy and compares the installed GRUB
loader and font with the shared tree. A changed booted system, profile, config,
GRUB handoff, or differing existing store-hash payload stops the update.

After a conservative ESP free-space check, it copies kernel, initrd and DTB
alongside old payloads. It saves the exact active config to
`/boot/boot/grub/grub.cfg.previous`. It roots the previous system with the native
Nix profile `/nix/var/nix/profiles/system-rpminiv2-previous`. These names extend
the existing GRUB file and system-profile paths; there is no separate manifest
or generation database. It writes a new config with exactly two entries:
new system first, previous booted system second, labelled `(previous)`. Each
file uses a same-directory temporary file, rename and `sync`. Only then does
it set `/nix/var/nix/profiles/system`. It does **not** activate or reboot.

Record the before/after store paths, profile generations, mount identities,
ESP filenames/hashes and free space in `docs/acceptance/`. Then reboot explicitly:

```sh
ssh -o BatchMode=yes -p 2222 root@10.42.3.1 reboot
# Wait for this same endpoint to return, then:
nix run .#rpminiv2-update -- --status
```

#### Select this generation's service bundle explicitly

System boot does **not** replace the persistent Korri service-bundle selection.
This is deliberate: [bundle deployment](../../../services/inputd/deploy/README.md)
and [portal deployment](../../../clients/portal/DEPLOYMENT.md) both preserve valid
selections across system changes. Keep this as a separate reviewed operation,
not an implicit updater side effect. A booted new system can still run an old
korrid until this step completes.

Before the update, record the old selector for runtime rollback:

```sh
ssh -o BatchMode=yes -p 2222 root@10.42.3.1 \
  readlink -f /nix/var/nix/gcroots/korri-bundle/active
```

After booting the new system, select its **Mini V2-specific** bundle, already
present in the copied system closure. Never select the generic `korri-bundle`:

```sh
bundle=$(nix eval --raw .#nixosConfigurations.rpminiv2.config.services.korriBundle.initialPackage.outPath)
ssh -o BatchMode=yes -p 2222 root@10.42.3.1 \
  "/run/current-system/sw/bin/korri-bundle-select switch '$bundle' /run/current-system/sw/bin/systemctl"
```

The existing selector checks the immutable bundle, keeps the previous bundle
GC root, restarts its consumers and verifies health. It performs no build.
Also check `/proc/$(systemctl show korrid -p MainPID --value)/exe` on the device
against the bundle's `bin/korrid` target. Preserve both old and new bundle roots
until full acceptance passes. The portal has its own explicit web selection;
a service-bundle switch does not replace it.

Require `current=` and `profile=` to equal the built system. Check failed units,
korrid, compositor and portal, unchanged plugin-host trust, uclamp/launch limits,
and a real korrid launch through the selected runner. Do not treat staged files
or a successful SSH copy as boot acceptance.

**Current limit:** core at `4e34b8a9e` does not implement general bubblewrap
wrapping. ADR 0015 records a decision, not shipped behavior. The 2026-10-22
acceptance therefore records the actual korrid → Gamescope → gamescopereaper →
runner chain, not a bubblewrap claim. A future wrapper implementation needs its
own acceptance.

### Rollback and update limits

If acceptance fails and SSH works, first restore the exact old service bundle
recorded above **if you changed it**. Do not blindly toggle an unrelated previous
selection:

```sh
ssh -o BatchMode=yes -p 2222 root@10.42.3.1 \
  '/run/current-system/sw/bin/korri-bundle-select switch /nix/store/EXACT-RECORDED-OLD-BUNDLE /run/current-system/sw/bin/systemctl'
```

Then restore the system boot selection:

```sh
nix run .#rpminiv2-update -- --rollback --expected-system /nix/store/EXACT-BOOTED-SYSTEM
ssh -o BatchMode=yes -p 2222 root@10.42.3.1 reboot
nix run .#rpminiv2-update -- --status
```

Rollback verifies that the backup's first entry matches the retained profile,
restores the **exact previous config bytes**, and sets the system profile back
to that store path. Nix may create a new generation number for this selection.
Rollback needs no build or signing key. Verify the old system after reboot.
If SSH never returns, **stop**. The owner must select the second GRUB entry, or
boot a rescue SD with labels different from `RPMINIV2` and `NIXOS_RPMINIV2`.
Selecting the GRUB fallback alone does not reset the Nix system profile; run
`--rollback` once SSH returns.

This is a one-update fallback, not a history manager. Each successful later
update replaces the previous config/profile. Old payload files are never
pruned automatically; a full ESP causes a refusal. Do not remove the fallback's
profile root or payloads. Filesystem rename plus `sync` does not make FAT immune
to power loss. An interruption can leave harmless extra payload/temp files or
a staged config whose profile is not yet selected. Inspect status and restore
with `--rollback`; do not rerun an update over a profile/boot mismatch.

The tool writes only ordinary files on the existing ESP and root filesystems.
It never installs GRUB, runs `bootctl`, touches EFI variables, writes a raw block
device, edits a GPT, reformats, or flashes an image. LUNs 1–5 and LUN 0 #1–#20
remain outside the update path. A failed update does not authorize writing them.

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
  only. Android also needs a `super` image and a factory reset. This is historical
  recovery information, **not** part of the system-update procedure or its
  approval. A GPT/raw write is forbidden in the 2026-10-22 update task.
- **Boot chain:** not changed. EDL (Power + Volume Up + Volume Down, Qualcomm
  9008) and the `prog_ufs_firehose_sm8250_lite_lp5.elf` programmer are reported
  for the Mini and RP5. Neither is tested on this unit.
