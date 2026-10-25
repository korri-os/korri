# Update the eMMC installation

`nix run .#rgds-update` stages a prebuilt, signed RG DS system over the existing
key-only SSH plugin, `root@10.42.1.1:2222`. It does not activate or reboot.
**No RG DS connection or update was made while implementing this path.** Get
owner approval and exclusive device access before the first real run.

## Boot layout and write boundary

**Correction to the initial update brief:** boot files belong to the ext4 root,
not the 30 MiB FAT partition. The existing producer `sd-image.nix` puts extlinux,
kernel, initrd and DTB in root `/boot`. `verify-image.py` reads
`/boot/extlinux/extlinux.conf` from partition 2 and requires that partition to be
bootable. Partition 1 must not be bootable. The evaluated NixOS configuration
mounts FAT at `/boot/firmware` with `nofail,noauto`.

| Area | Update behavior |
|---|---|
| eMMC MBR and 16 MiB raw area, including U-Boot at sector 64 | Read-only; no partition or loader installation |
| Partition 1, FAT `NIXOS_BOOT` | Untouched, whether mounted or not |
| Partition 2, ext4 `NIXOS_RGDS`, mounted at `/` | Prebuilt store import, native system profiles, `/boot/extlinux`, `/boot/nixos`, temporary staging |
| Other media and firmware | Untouched |

The target refuses the wrong board, non-MMC media, any root except writable
`/dev/mmcblk0p2` labelled `NIXOS_RGDS`, a non-MBR table, a non-bootable root or
bootable FAT, and boot/profile/store directories on another mount or through
symlinks. It requires a regular existing extlinux config and completed image
first-boot registration. It checks effective `require-sigs = true`,
`max-jobs = 0`, empty `builders`, and `fallback = false`.

Remove the SD before an update and its acceptance reboot. U-Boot also scans SD;
duplicate root labels or an inserted bootable SD can select another system.

## One-time owner steps before the first update

These are instructions for a later, approved session. Key authorization is an
owner decision; this change does not choose, generate, install or rotate a key.

1. On **Zao**, choose the owner's existing SSH public key. Record its fingerprint
   with `ssh-keygen -lf /path/to/chosen-key.pub`. Keep the private key on Zao.
   Establish the existing RG DS USB network link at `10.42.1.1`; coordinate the
   local console with its operator. Do not change the plugin's port or trust.
2. Through that trusted local console, confirm the selected key's fingerprint,
   then authorize its **public** line for root. The plugin's shipped
   `sshd_config` already accepts `/root/.ssh/authorized_keys` and
   `/etc/ssh/authorized_keys.d/root`; the published plugin VM tests exercise the
   latter. For the currently empty account, the native root-key path is:

   ```sh
   (
   set -e
   install -d -m 0700 /root/.ssh
   test ! -e /root/.ssh/authorized_keys && test ! -L /root/.ssh/authorized_keys
   # Stop and review if the check above fails.
   umask 077
   cat > /root/.ssh/authorized_keys <<'KEY'
   PASTE_THE_OWNER_CHOSEN_PUBLIC_KEY_LINE
   KEY
   chmod 0600 /root/.ssh/authorized_keys
   )
   ```

   Never paste a private key. Do not disable the plugin's key-only settings,
   signatures, publisher binding or permission checks. No sshd restart is needed
   just to add an authorized key.
3. From the same console, obtain the plugin's **existing** SSH host public key:

   ```sh
   systemctl is-active korri-ssh.service
   state=$(systemctl show korri-ssh.service -p StateDirectory --value)
   test -n "$state"
   cat "/var/lib/$state/ssh_host_ed25519_key.pub"
   ```

   This path comes from the plugin's native service `StateDirectory` and its
   shipped `korri-sshd-prepare`, not a new updater identity. On Zao, inspect any
   existing record with `ssh-keygen -F '[10.42.1.1]:2222'`. If absent, add the
   verified public key as `[10.42.1.1]:2222 ssh-ed25519 PUBLIC_KEY_DATA` to
   `~/.ssh/known_hosts`. Stop on a conflicting existing key; do not delete it or
   accept an unverified `ssh-keyscan` result.
4. Make the selected client key available through Zao's normal SSH agent or
   `~/.ssh/config` (`Host 10.42.1.1`, `IdentityFile /path/to/chosen-private-key`).
   Verify `ssh -o BatchMode=yes -o StrictHostKeyChecking=yes -p 2222
   root@10.42.1.1 id -u` returns `0`. The updater always uses strict known-host
   checking and batch authentication, including both directions of `nix copy`.
5. Verify Zao already has the Mini V2 updater's private signing key at
   `~/.config/korri/cache-key.secret`, owned by the invoking user and with no
   group/other access. Its public identity must be the existing device-trusted
   `korri-cache-zao-1:thKjQnMnPl8AqTuZWJTn+ej9BORTPqzleGue4ZfJ2u4=`.
   Do not make a replacement key or add another trusted key to work around a
   failure. ARM builds must use Zao's fuji/yari builders, never the RG DS.
6. Remove SD media. Confirm `/nix-path-registration` is absent and that the
   current system has its native system profile. Run the read-only status below
   and inspect the board, root and profile. If registration, mounts or profile
   disagree, stop for a separately reviewed repair. Do not automatically repair
   an existing installation as part of an update.

## Run from the approved checkout on Zao

```sh
nix run .#rgds-update-check
nix run .#rgds-update -- --status
# Use the exact current= path returned above, after checking the device.
nix run .#rgds-update -- --expected-system /nix/store/EXACT-CURRENT-SYSTEM
```

Both writes and rollback require `--expected-system`. Status does not create a
lock file. Writes lock the existing native profiles directory and recheck the
booted system and config checksum. A pending system profile different from the
booted system refuses another update; reboot or recover it first.

Zao builds `nixosConfigurations.rgds.config.system.build.toplevel`. Build
commands explicitly add the Pages cache and the existing CI public key from
`nix/cache/identity.nix`. Local/remote build scheduling belongs only to Zao's
Nix daemon. The target receives a signed closure through `nix copy`; it never
evaluates a flake, realises a derivation, builds, or dispatches a build.
A failed cache/signature/copy operation stops before the boot/profile commit.
Repair missing prebuilt delivery on a build machine. Never enable fallback or
use `--no-check-sigs`. See [device-cache policy](../../device-cache/README.md).

For rendering, Zao also copies the old prebuilt system closure **from** the
handheld, with normal signature checks, and temporarily GC-roots it locally.
An old closure without an acceptable trusted signature blocks this step; arrange
its signed prebuilt delivery separately, not a signature bypass. This can copy
a substantial closure back to Zao on the first update.

The native host renderer comes from the pinned NixOS
`generic-extlinux-compatible/extlinux-conf-builder.nix`, the same producer as
`sd-image.nix`'s `populateCmd`. Only its hard-coded host profiles directory is
redirected to an isolated native `system-1-link` pointing to the old system.
The unmodified rendering logic emits the new default and one previous entry,
including native filenames, kernel parameters, timestamp and DTB path. Rendering
uses UTC and reads the timeout and DTB from the existing RG DS configuration
(currently three seconds and `rockchip/rk3568-anbernic-rg-ds.dtb`). Changed installed
header settings refuse the update rather than silently changing the handoff.

## Commit, retention and rollback

The shared Mini V2 core owns SSH, signing, policy-independent file copies and
the system-profile transaction. Its Mini V2 defaults and GRUB behavior remain
unchanged. RG DS owns root/eMMC identity, native extlinux rendering, DTB directory
copies and boot-payload retention. It does not use an ESP or GRUB assets.

The target checks available root space conservatively, including the full
rendered payload even if some files already exist. It copies payloads first;
an existing hash-named payload must match byte-for-byte. Each file uses a
same-directory temporary file, GNU `sync FILE` (`fsync`), rename, then directory
`fsync`. Only complete payloads can become referenced by the boot config.

The existing `/boot/extlinux/extlinux.conf` grounds the exact backup filename
`extlinux.conf.previous`. The native `/nix/var/nix/profiles/system` grounds
`system-rgds-previous`, using the same previous-profile rule as Mini V2. There
is no updater manifest, version database or runtime migration. The updater roots
the previous closure, saves the exact old config, commits the generated config,
then sets the native system profile. A profile-setting failure restores both.

After a successful commit, RG DS retains the union of payload trees referenced
by the **active config and exact backup config** and removes only unreferenced
hash-named children of `/boot/nixos`. This keeps every backup boot payload
valid without unbounded stale payload growth. The active menu has new/default
and previous; the first backup can contain additional pre-existing entries.
Native system-profile history is not garbage-collected by this tool. Keep its
older generations until acceptance if the first backup still names them;
`system-rgds-previous` independently roots the previous default closure even
when normal system generations are removed. Rollback leaves payloads in place;
the next successful update bounds them again.

If acceptance fails and SSH works:

```sh
nix run .#rgds-update -- --rollback --expected-system /nix/store/EXACT-BOOTED-SYSTEM
# Reboot separately only after inspecting the restored selection.
```

Rollback checks that the backup's first `init=` matches its GC-rooted previous
profile. It restores exact config bytes and the profile. It does not reboot or
reverse plugin/bundle selections. Only one exact previous selection is kept.
If the system cannot boot or SSH is unavailable, use the separately documented
SD/local-console recovery procedure; this app is not a flasher.

**Do not run `switch-to-configuration boot`, `nixos-rebuild boot/switch`, or the
extlinux generator on the handheld.** They can regenerate the config and prune
payloads outside this retention transaction. Source inspection and evaluation of
the RG DS activation scripts, boot commands and systemd service scripts found
no automatic extlinux regeneration. The inherited first-boot path can expand the
root partition and set the system profile when `/nix-path-registration` exists;
the updater refuses that unfinished state. This is a source/evaluation check,
not a claim about unseen changes on the running device.

## Acceptance still required

Record before/after current and profile paths, config bytes/hashes, mount/table
identity, payload hashes, root free space and profile roots. Reboot explicitly,
then require `current=` and `profile=` to equal the staged system. Verify U-Boot
still scans eMMC ext4, both screens/input, USB SSH, failed units, korrid, portal,
and unchanged plugin trust. Check rollback and another forward update too.

Persistent service-bundle and portal selections are deliberately not changed.
A new system can still run old application selections. Record them before the
update and use the existing selectors only as separate approved operations; use
the RG DS-specific `services.korriBundle.initialPackage`, not a generic bundle.

`rgds-update-check` uses tiny real Nix store systems and the actual pinned
extlinux generator. It compares every rendered and staged pathname/byte against
an independently rendered two-system fixture, verifies real Nix profile roots
and exact rollback, and tests mount/media/MBR/policy refusals, low space,
conflicting payloads, symlinks, retention, profile failure and CLI guards.
`rpminiv2-update-check` must also pass after changes to the shared core.

Unproven until hardware acceptance: actual SSH authorization and host-key state,
installed boot-config dialect and signatures, full ARM closure delivery, reboot
and boot-menu selection, and rollback after a failed boot. The multi-file
config/profile transaction is not power-loss atomic as a whole; a power cut can
leave a pending profile or backup/profile disagreement. It preserves complete
boot files, but may need manual inspection/recovery. This path adds no user-data
backup and offers no automatic health-triggered reboot or rollback.
