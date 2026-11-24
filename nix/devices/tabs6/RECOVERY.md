# Tab S6 flashing and recovery (parent session only)

These are host-side procedures, not permission to use a tablet owned by
another session. Coordinate device ownership first. Do not run any of these
commands during repository housekeeping. Build on zao or the remote builders,
never on the tablet.

Facts below come from the old tabs6 `AGENTS.md` and flash scripts, observed
on SM-T860 / gts6lwifi on 2026-11-17. The stock version was XAR
`T860XXS5DWH1`, Android 12 / One UI 4.1. The owner approved the unlock, wipe
and Knox trip on that date. That historical approval is not approval to erase
current user data.

## Partition rules

- Write only `boot`, `vbmeta`, `cache` and `userdata`. `userdata` holds the
  installed system and user data; replacing it needs explicit approval for
  that run. Flashing remains parent-only, including Odin and fastboot.
- Never write `abl`, `xbl*`, `tz`, `hyp`, `aop`, `devcfg`, `uefi*`, `efs`,
  `sec_efs`, `persist`, `param`, `misc`, `modem*`, `keymaster`, `vbmeta_*`,
  `recovery` or any other partition. Never flash BL, CP or a PIT. Never
  repartition. Do not automate a full stock restore as a recovery shortcut.
- Never use Magisk-in-recovery. It sets the `misc` BCB to `boot-recovery`;
  Odin cannot flash `misc`, which strands the tablet. Host-side `magiskboot`
  repacking of a boot container is not Magisk-in-recovery.
- Never publish stock firmware, extracted blobs, or anything from `efs`,
  `sec_efs`, `persist` or `modemst*`. The stock inputs stay on zao, outside git
  and binary caches.
- Before every write, record the image hash. After every write, read back and
  compare. Stop if there is no verified read-back method for that operation.
  The historical flash scripts did not implement every read-back check;
  their success message alone is not verification.

The old notes mention a full signed stock BL/AP/CSC restore as a last resort
(no CP on this model). It is **outside** the allowed four-partition procedure.
It needs a separate owner-approved recovery plan. A different BL version or
binary number must never be substituted.

## Boot chain and offline gate

Samsung ABL loads a stock Android boot container patched with Project Aloha
UEFI release 2412.74 (`samsung-gts6lwifi/SM8150_EFI_NOSB.fd`). Project Mu loads
`\EFI\BOOT\BOOTAA64.EFI` from the FAT ESP on `cache`. systemd-boot loads the
EFI-stub kernel and gts6lwifi DTB. The ext4 root is on `userdata`, without an
initrd.

The accepted initial AP archive on zao is still at
`/home/simonwjackson/code/sandbox/tabs6/out/boot-uefi-v2.tar.md5`:

```text
00c6d907ee98788e9470f14959792a1f6fbcec7e76aab06aaa33155009121a53
```

It contains exactly `boot.img` and `vbmeta.img`. Its vbmeta was generated with
`avbtool make_vbmeta_image --flags 3`, unsigned. The earlier archive with a
re-flagged signed stock vbmeta was **rejected** (`FAIL! Auth`). The archived
`nix/bootimg/build_boot.py` still generates that rejected variant; do not use
its `build-boot` output for recovery. The patcher, shellcode and offline
checker sources remain there as reference, not a supported Korri build app.

Before flashing any replacement boot image, run the reference port's
`tools/avb_analyze.py` and all checks in `docs/BOOT_METHOD.md`, under
`/home/simonwjackson/code/sandbox/tabs6/upstream/gts6l-mainline-linux/`.
Require: exactly 64 MiB, `ANDROID!` header v1, `UNCOMPRESSED_IMG`, `_FVH`,
`SEANDROIDENFORCE`, `AVB0`, and self-consistent `AVBf` offsets and sizes.
Check the injected payload against the pinned FD and shellcode too. A hash
match does not replace those checks. No replacement boot image was built or
flashed as part of the archive operation.

## Prefer the existing ESP recovery path

For a running system, use `tabs6-esp rollback` or select LAST KNOWN GOOD at
boot. Volume Down at power-on is the recovery entry point. Normal updates
use the signed deploy tool in [README.md](README.md), not a reflash of
`userdata`. For an offline root repair, use the README's `fsck-once` path.

## Mu fastboot: `cache` and `userdata`

1. Hold **Volume Down at power-on**, then **replug USB**. Mu fastboot
   enumerates as **045e:0c2f**, labelled "Android". The normal Mu USB device
   **045e:066b** speaks Microsoft UFP, not fastboot. The power key does not
   work in Mu's menu.
2. Wait for a fresh enumeration. A wedged session can remain in `fastboot
   devices`; restart the tablet instead of assuming it is usable.
3. Build the current `tabs6-esp-image` and `tabs6-rootfs-image` on the host.
   Require identical `toplevel` files and reject `PLACEHOLDER-KERNEL` images.
   Record hashes before writing. The ESP is raw FAT; root is Android sparse.
4. The write commands from the proven procedure are:

   ```sh
   fastboot flash cache /absolute/path/to/tabs6-esp.img
   fastboot -S 128M flash userdata /absolute/path/to/tabs6-rootfs.simg
   ```

   `userdata` takes about three minutes with 128 MiB chunks. **Never interrupt
   fastboot, pipe its output into `head`, close its output pipe, or send extra
   getvars while a command runs.** A cut-off reply wedges Mu until a forced
   restart. Keep full output in the terminal or a file.
5. Complete read-back verification before rebooting. These two write commands
   do not implement that verification by themselves.

The Linux installer initramfs and ESP variant were a workaround for the
mistaken assumption that Mu had no fastboot. Mu fastboot was then proven to
write both partitions. Keep the installer in the archive; do not introduce
its unauthenticated TCP writer into the product image.

## Odin recovery

Use a fresh Download-mode boot for **one Odin session**. A second session
fails the handshake. The local host tool remains at
`/home/simonwjackson/code/sandbox/tabs6/tools/samloader` (samloader-rs 2.2.0).
Its `flash -a <archive>` uses an AP archive; validate that the only members
are `boot.img` and `vbmeta.img`. For CACHE, Odin requires an **Android sparse
ESP** (`img2simg`), not the raw FAT image: raw CACHE failed with LOKE status 3.
The old combined recovery session wrote the accepted AP archive, USERDATA,
then CACHE. Do not reuse its stale rootfs/ESP hashes or paths.

After **every Odin session**, stock recovery starts once
(`RebootRecoveryAfterOdinDL`). Choose **"Reboot system now"**, never a wipe.
A custom boot was initially refused with "Only official released binaries are
allowed to be flashed (boot)" until Android had been online once after unlock
(Knox Guard / VaultKeeper). Repeatedly flashing does not solve that check.
