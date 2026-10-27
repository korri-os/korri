# Odin 2 Portal stock reconstruction

This pipeline reconstructs the AYN Odin 2 Portal stock `super.img` without changing its logical partitions or contacting a device. It accepts only the captured Android 13 build `Odin2Portal_V1.0.0.130_20250312_205701_user`.

The source contract comes from the verified stock producer files and observed device layout:

- `contract/SHA256SUMS` pins all 15 captured Android and boot images.
- `contract/logical-SHA256SUMS` pins the seven populated logical partitions extracted from `super.img`.
- `contract/super-layout.txt` is the device and host `lpdump` output, which matched exactly during capture.
- The build ID and fingerprint files pin the source identity.

Proprietary images remain outside Git.

## Run

```console
nix run .#odin2portal-stock-repack -- \
  /path/to/odin2portal-stock-130 \
  /path/to/output
```

The output path must not exist. The command publishes it only after all checks pass.

The output contains:

- `super.img`: the reconstructed physical dynamic-partition image
- `RESULT.txt`: the final read-only verification result
- `evidence/`: source hashes, source and rebuilt layouts, filesystem checks, AVB inspection, and the rebuilt image hash

The pipeline deletes intermediate logical images after verification because the source `super.img` remains the immutable rollback input.

## Stage a rollback bundle

```console
nix run .#odin2portal-rollback-bundle -- \
  /path/to/odin2portal-stock-130 \
  /path/to/rollback-bundle
```

This task accepts the same exact stock contract. It copies only the 15 verified Android images and the captured build, active-slot, checksum, and layout records. It adds the non-executing procedure from `ROLLBACK.md`. The task does not contain or call a device tool.

The output is a private directory with mode `0700`. It must be copied to encrypted offline storage before the first later flash. Local staging on the same disk is not a backup.

## Run the first marker-only mutation

```console
nix run .#odin2portal-marker-dry-run -- \
  /path/to/odin2portal-stock-130 \
  /path/to/marker-output
```

This host-only dry run appends one ignored comment to the existing product `build.prop`. It grows `product_a` by 16 MiB so the stock shared-block filesystem can be safely unshared, regenerates the product hashtree without FEC, creates an unsigned `vbmeta_system_a.img`, and rebuilds `super.img`.

The result is deliberately not flashable. AYN's signed root AVB chain remains unresolved. Read `MARKER-NOT-FLASHABLE.md` before using the output.

Before publication, the marker command proves that the source and staged inputs match the exact-build contract, `build.prop` equals its stock content plus the exact comment, ext4 is clean, the unsigned product and `vbmeta_system` chain verifies on the host, all other logical bytes are unchanged, slot-B placeholders remain empty, and the rebuilt container has the stock physical size. It publishes partition-shaped files only under `NON_FLASHABLE_ARTIFACTS/` with `.not-flashable` suffixes.

## Build the signed AVB dry run

```console
nix run .#odin2portal-signed-avb-dry-run -- \
  /path/to/odin2portal-stock-130 \
  /private/path/korri-odin2portal-avb.pem \
  /path/to/signed-avb-output
```

The private key must be the selected Korri RSA-4096 PEM file with mode `0600`. Its encoded public-key SHA-256 must match the tracked public identity contract. The key stays outside Git and is copied only to temporary memory during the command.

The command first rebuilds the marker image from the verified stock source. It does not trust an external marker directory. The output contains the rebuilt marker `super` image, the encoded public key, signed root and system `vbmeta` images, and verification evidence. It does not contain the private key.

The command preserves the AYN `boot` and `recovery` chain keys. It replaces the root signing key and the `vbmeta_system` chain key with the Korri key. The bootloader must stay unlocked.

This output is not flashable. Read `SIGNED-AVB-NOT-FLASHABLE.md`. The selected home and Towada key copies are private but not encrypted. The user owns the separate encrypted offline backup.

## Build the Korri launcher image dry run

Build an arm64 release APK that declares Korri as a HOME candidate and sign it with the selected Korri Android release key. The approved signer certificate SHA-256 is pinned in `contract/korri-release-cert-SHA256.txt`. The exact approved APK SHA-256 is pinned in `contract/korri-launcher-apk-SHA256.txt`; a new APK requires an explicit contract update.

```console
nix run .#odin2portal-launcher-image-dry-run -- \
  /path/to/odin2portal-stock-130 \
  /private/path/Korri-arm64-release.apk \
  /private/path/korri-odin2portal-avb.pem \
  /path/to/launcher-image-output
```

The host-only pipeline verifies the exact APK package, one release signer, non-debuggable and enabled application, enabled `MAIN+HOME+DEFAULT` activity, defined activity bytecode, bundled portal content, and arm64-only native code. It self-derives from verified stock, retains the marker, inserts only `/product/app/Korri/Korri.apk`, signs the existing Korri AVB chain, and proves every other logical partition is unchanged. It preserves Android Settings, the AYN launcher, and AYN services.

The output is quarantined under `NON_FLASHABLE_ARTIFACTS/`, contains no private key, states `flash ready: no`, and never calls ADB or fastboot. Read `LAUNCHER-NOT-FLASHABLE.md`.

The image adds Korri as a HOME candidate but does not alter the existing user's preferred HOME. `HOME-PROVISIONING.md` defines the separate, auditable package-manager operation and the AYN-launcher fallback. That operation requires separate approval.

Verify the exact launcher output and stock rollback bundle before any live device gate:

```console
nix run .#odin2portal-launcher-install-readiness -- \
  /path/to/odin2portal-korri-launcher-signed-avb-dry-run \
  /path/to/odin2portal-stock-130-rollback-bundle
```

The final line must be `ODIN2PORTAL_LAUNCHER_INSTALL_ARTIFACTS_READY`. The command pins the produced launcher image bytes, complete inventory, operator documents, and rollback artifacts. It contains no ADB or fastboot command and does not approve an installation.

`INSTALL.md` defines the launcher write and acceptance sequence after the signed marker image passes its first-boot acceptance gate. After the launcher image boots, run the read-only installed-image gate:

```console
nix run .#odin2portal-launcher-device-acceptance -- \
  ef201f64 \
  /path/to/odin2portal-launcher-acceptance-ef201f64
```

This gate verifies the installed product APK bytes, release signer, boot state, marker, HOME candidates, AYN fallback, and Settings package. `HOME-PROVISIONING.md` remains a separate user-state operation after launcher-image acceptance.

## Review installation readiness

Read `INSTALL.md`. It defines the proposed write order, stop conditions, first-boot checks, and stock rollback sequence. It does not approve or execute an installation.

Run the host-only artifact gate:

```console
nix run .#odin2portal-install-readiness -- \
  /path/to/odin2portal-stock-130-signed-avb-dry-run \
  /path/to/odin2portal-stock-130-rollback-bundle
```

This command verifies both complete artifact sets. It does not call ADB or fastboot. Live device gates remain incomplete until Odin 2 Portal `ef201f64` is connected in bootloader fastboot mode.

## Stock repack guarantees

Before publishing stock repack output, the command proves:

1. Every captured source image matches the exact-build SHA-256 contract.
2. Build identity and source `lpdump` match the captured device.
3. All seven populated ext4 filesystems pass read-only `e2fsck`.
4. Logical AVB footers and both vbmeta images parse successfully.
5. Reconstructed slot-A logical partitions are byte-identical to source.
6. All slot-B logical placeholders remain zero-length.
7. Reconstructed `lpdump` output and physical image size match source.

The script contains no ADB, fastboot, mount, or device-write path.

## Stock repack limits

The stock repack is not an installer and does not prove that a flashed image boots. It does not modify filesystems, regenerate AVB metadata, or sign an OTA. Those are separate later slices with higher risk.

The reconstructed physical `super.img` does not need to match the source file byte-for-byte. `lpmake` can regenerate metadata and unused bytes. The acceptance gate is exact `lpdump` geometry plus byte-identical logical partitions. The evidence records the rebuilt physical hash without claiming that it is the source hash.

The cost is a temporary second 5.68 GB `super.img` plus unpacked working data. A successful run retains only the reconstructed image and evidence.

## Test

```console
nix run .#odin2portal-stock-repack-check
nix run .#odin2portal-rollback-bundle-check
nix run .#odin2portal-marker-dry-run-check
nix run .#odin2portal-signed-avb-dry-run-check
nix run .#odin2portal-launcher-image-dry-run-check
nix run .#odin2portal-launcher-install-readiness-check
nix run .#odin2portal-launcher-device-acceptance-check
nix run .#odin2portal-install-readiness-check
```

The public checks use small fixtures for mutation and rejection behavior. The marker check also runs the full pipeline when the private stock capture is supplied:

```console
ODIN2PORTAL_STOCK_SOURCE=/path/to/odin2portal-stock-130 \
  nix run .#odin2portal-marker-dry-run-check

ODIN2PORTAL_STOCK_SOURCE=/path/to/odin2portal-stock-130 \
ODIN2PORTAL_AVB_PRIVATE_KEY=/private/path/korri-odin2portal-avb.pem \
  nix run .#odin2portal-signed-avb-dry-run-check

ODIN2PORTAL_STOCK_SOURCE=/path/to/odin2portal-stock-130 \
ODIN2PORTAL_LAUNCHER_APK=/private/path/Korri-arm64-release.apk \
ODIN2PORTAL_AVB_PRIVATE_KEY=/private/path/korri-odin2portal-avb.pem \
  nix run .#odin2portal-launcher-image-dry-run-check
```

The checks cover successful publication, source immutability, exact build and slot gates, checksum and layout rejection, and refusal to overwrite output.
