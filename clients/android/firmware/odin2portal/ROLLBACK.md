# AYN Odin 2 Portal stock rollback bundle

This directory is a verified copy of the stock Android images captured from Odin 2 Portal `ef201f64`. It is specific to build `Odin2Portal_V1.0.0.130_20250312_205701_user` and fingerprint `qti/kalama/kalama:13/TKQ1.231222.001/eng.Odin2P.20250312.203450:user/release-keys`.

## Verify

Run this read-only check from the bundle directory:

```console
sha256sum --check SHA256SUMS.local
```

All 15 entries must report `OK`. Stop if any entry is absent or different.

The bundle also retains the captured `_a` active slot and exact `super.img` logical-partition geometry. These records are evidence. They are not instructions to change the active slot.

## Contents

The bundle contains the captured `super.img` plus both copies of `boot`, `init_boot`, `vendor_boot`, `dtbo`, `recovery`, `vbmeta`, and `vbmeta_system`. The captured A/B standalone image pairs are byte-identical.

The bundle does not contain `userdata`, file-encryption `metadata`, GPT data, low-level bootchain firmware, calibration, provisioning, or security partitions. The separate private deep-recovery archive holds the same-device records that were deliberately excluded here.

## Restore gate

This bundle does not execute a restore. Before a later restore is approved, the operator must:

1. Verify the encrypted offline copies of both source archives.
2. Confirm that the target is the same Odin 2 Portal unit and that its bootloader remains unlocked.
3. Verify every bundle image with `SHA256SUMS.local`.
4. Confirm that the proposed restore procedure writes only the named Android partitions.
5. Review and test the restore procedure separately before connecting the device.

A normal rollback must use a narrowly reviewed fastboot or fastbootd procedure. It must not write GPT regions, private recovery records, `userdata`, or file-encryption `metadata`. QFIL and the archived Firehose programmer remain EDL-only recovery measures and require separate approval.

## Limits

A valid checksum proves that these files match the captured stock build. It does not prove that a restore command targets the correct device or partition. It does not replace the private same-device recovery archive, and it does not prove EDL recovery.
