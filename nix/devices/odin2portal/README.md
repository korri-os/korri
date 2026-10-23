# AYN Odin 2 Portal

`sd-image.nix` produces a removable-media NixOS image. It does not install
internal storage or change the AYN boot chain. The owner's running device is
now a separate internal-UFS installation, despite using the image's labels.
[Read-only preflight](../../../docs/acceptance/odin2portal-update-preflight-2026-10-22.md)
and [update acceptance](../../../docs/acceptance/odin2portal-system-update-2026-10-22.md)
record the observed layout and the owner's one-run approval. Do not infer the
medium from `ODIN2P_ESP` / `ODIN2P_ROOT` alone.

## Owner's observed internal layout

| Mount | Device | Type / label | Size |
|---|---|---|---:|
| `/boot` | `/dev/sda18` | vfat / `ODIN2P_ESP` | 1 GiB |
| `/` | `/dev/sda19` | ext4 / `ODIN2P_ROOT` | 105,813,880,832 bytes |

Parent disk `sda` is internal UFS model `THGJFJT0E25BAIPC`, reached through
`1d84000.ufshc`. The loader is AYN U-Boot in `loader_a`; its EFI boot manager
runs `/EFI/BOOT/BOOTAA64.EFI` from the ESP, then systemd-boot loads a BLS entry.
The running system reports `Boot0001` (`scsi 0`) and that ESP's PARTUUID.
The firmware includes its MMC boot-device driver and the board has an SDHCI
host. No SD card was detected during acceptance. The firmware was unchanged;
physical rescue-SD boot was not tested in this operation.

## Manually update an approved system

**Every internal-UFS update needs explicit owner approval for that run. Never
turn this into an automated internal-storage update path.** Coordinate with
other workers before any write. The 2026-10-22 approval applied to that one
update only. These are the existing Nix operations, not an internal installer.

Build and sign on Zao/configured builders, never the handheld. From the chosen
checkout, build the exact `nixosConfigurations.odin2portal.config.system.build.toplevel`.
Keep a host GC root until delivery completes:

```sh
nix build --out-link /tmp/odin2portal-system .#nixosConfigurations.odin2portal.config.system.build.toplevel
system=$(readlink -f /tmp/odin2portal-system)
bundle=$(nix eval --raw .#nixosConfigurations.odin2portal.config.services.korriBundle.initialPackage.outPath)
nix store sign --recursive --key-file ~/.config/korri/cache-key.secret "$system"
```

**Before copying or changing anything**, establish the approved current-system
path and check the board model, actual mounts, filesystem types, labels, UFS
controller identity, available space, native profile generations, and current
bundle. Save metadata and ESP file hashes to a dated acceptance file. Query the
missing closure paths and their sizes; keep at least 1 GiB root headroom. Keep
signatures required, builds disabled, and the private key on Zao only.

Read-only checks on the approved device:

```sh
tr -d '\0' </proc/device-tree/model
readlink -f /run/current-system /nix/var/nix/profiles/system
findmnt --mountpoint /boot
findmnt --mountpoint /
blkid -p -s LABEL -o value /dev/sda18
blkid -p -s LABEL -o value /dev/sda19
readlink -f /sys/class/block/sda/device
readlink -f /nix/var/nix/gcroots/korri-bundle/active
bootctl --no-pager status
df -B1 / /boot
```

Refuse a wrong board or any device/type/label mismatch with the approved table.
If the device is SD-booted instead, stop and identify that layout; this internal
approval does not apply. Labels alone are not sufficient. Do not reformat,
repair a partition table or install a new loader to make a check pass.

The configuration uses systemd-boot with `efi.canTouchEfiVariables = false` and
`configurationLimit = 3`. Verify the generated builder also has
`CAN_TOUCH_EFI_VARIABLES = false`: normal update uses `touchVariables: false`
over Varlink. Record an EFI-variable inventory hash before and after staging.
Do not use `--install-bootloader`, `NIXOS_INSTALL_BOOTLOADER=1`, an EFI variable
setter, or a firmware-flashing command.

After the exact current system and mount checks pass, copy the signed closure
from the host:

```sh
NIX_SSHOPTS='-o BatchMode=yes -p 2222' nix copy --to ssh-ng://root@10.42.4.1 "$system"
```

Then run these native operations individually on the device. `OLD` and `NEW` mean the reviewed exact
store paths, not a flake or derivation. Retain the old bundle path as well:

```sh
nix-env -p /nix/var/nix/profiles/system-odin2portal-previous --set OLD
nix-env -p /nix/var/nix/profiles/system --set NEW
env -u NIXOS_INSTALL_BOOTLOADER -u NIXOS_INSTALL_GRUB NEW/bin/switch-to-configuration boot
```

The boot action installs files only; it does not evaluate a flake, compile,
activate the new system, change firmware, or reboot. Check the resulting
`loader.conf` and every retained BLS entry. The three-generation limit can
remove older ESP entries/payloads, but does not delete Nix profile history.
Current, development and Linux 7.0.2 rescue entries share that generation limit.
Keep the old generation's boot entries and its explicit profile root.

Separately select this generation's **Odin-specific** bundle already in the
closure, using the existing native selector:

```sh
NEW/sw/bin/korri-bundle-select switch EXACT-NEW-BUNDLE /run/current-system/sw/bin/systemctl
```

The selector retains its previous bundle GC root, restarts its consumers and
checks health. A NixOS profile change or reboot alone intentionally preserves
the old bundle. The portal also preserves its separately selected web profile;
this is not a web-app deployment. See the existing
[bundle](../../../services/inputd/deploy/README.md) and
[portal](../../../clients/portal/DEPLOYMENT.md) procedures.

Reboot explicitly. Wait for the same SSH endpoint to return, then verify the
booted system/profile, korrid executable from the new bundle, no failed units,
portal HTTP/selected-index agreement, unchanged identity and plugin trust,
limits/uclamp, and a real korrid launch. Record the actual process chain. Core at
`4e34b8a9e` does not implement ADR 0015's general bubblewrap decision; do not claim
that an ordinary launch uses it. Stop the exact acceptance launch through korrid.

### Rollback and recovery limits

If acceptance fails and SSH works, stop the acceptance launch, restore the exact
recorded old bundle with the native selector, set the system profile to `OLD`,
then run `OLD/bin/switch-to-configuration boot` with the install-bootloader
variables unset. Check the same mount and no-EFI-variable policy first. Reboot
and verify the old booted system and bundle. Keep old/new GC roots until
acceptance passes. Never restore only an old BLS file whose payload was pruned;
reinstall boot files from the retained old closure.

If SSH does not return, stop. The owner can use the retained systemd-boot entry
or the existing SD route. A rescue SD must have distinct filesystem labels and
matching root selection, so it cannot conflict with the internal labels. This
procedure does not provision or flash a rescue card. It leaves the AYN loader,
firmware partitions, bootloader lock state and SD hardware path unchanged.

No step writes a raw block device, GPT or firmware partition (`abl_*`,
`loader_*`, `xbl*`, `tz*`, `hyp*`, `aop*`, `devcfg*`, `uefi*`), changes the
bootloader state, or writes EFI variables. Only the explicitly approved ESP/root
files are in scope. The orange unlocked-bootloader warning and reduced physical
access protection remain standing device-policy limits; UEFI Secure Boot status
is not proof of the Android bootloader's lock state.
