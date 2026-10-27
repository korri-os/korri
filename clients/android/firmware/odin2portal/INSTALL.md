# Odin 2 Portal controlled installation plan

This plan applies only to Odin 2 Portal `ef201f64`. It applies only to stock build `Odin2Portal_V1.0.0.130_20250312_205701_user`.

This document does not approve an installation. Do not run a flash command without separate user approval.

## Safety limits

Keep the bootloader unlocked. The device will show the orange startup warning. An unlocked bootloader gives less protection from physical access.

Do not use a data-wipe option. Do not force a flash. Do not erase a partition. Do not disable AVB verification or dm-verity. Do not run a bootloader lock or critical-lock operation.

Do not change the active slot during normal installation. Use the documented slot-A recovery branch only after specific approval.

Do not write slot B. Do not write `userdata`, `metadata`, GPT data, bootchain firmware, calibration data, or security data. Do not use QFIL for this installation.

The custom product has no Forward Error Correction data. Android Verified Boot can detect product corruption. It cannot repair that corruption.

## Required files

Use these signed files:

- `NON_FLASHABLE_ARTIFACTS/super.img.not-flashable`
- `NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable`
- `NON_FLASHABLE_ARTIFACTS/vbmeta_a.img.not-flashable`

Use these rollback files:

- `super.img`
- `vbmeta_system_a.img`
- `vbmeta_a.img`

The `.not-flashable` names are an intentional safety control. Do not rename the files before final approval.

After the signed marker image passes first-boot acceptance, use these launcher follow-up files from the separately verified launcher output:

- `NON_FLASHABLE_ARTIFACTS/super.img.not-flashable`
- `NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable`
- `NON_FLASHABLE_ARTIFACTS/vbmeta_a.img.not-flashable`

Do not substitute the marker output for the launcher output. The dedicated launcher readiness gate pins different `super` and `vbmeta_system_a` bytes.

## Host readiness gate

Run this host-only check:

```console
nix run .#odin2portal-install-readiness -- \
  /path/to/odin2portal-stock-130-signed-avb-dry-run \
  /path/to/odin2portal-stock-130-rollback-bundle
```

The final line must be `ODIN2PORTAL_INSTALL_ARTIFACTS_READY`. This result does not approve a device write.

Before a release, distribution, or installation on another person's device, the user must own a verified encrypted offline backup.

For private development on owner-operated Odin 2 Portal `ef201f64`, the user can explicitly accept the loss risk and defer that backup. Record the waiver before installation. In the same session, verify the exact stock source and rollback bundle against their SHA-256 contracts. This waiver does not apply to a bootloader lock or to deletion of the verified stock source.

Keep the signed output and rollback bundle on stable local storage. Keep them available for the complete session. Run the host readiness gate again in the same shell immediately before the first write.

## Device readiness gate

Use a direct USB cable. Do not use a USB hub. Charge the battery to at least 60 percent.

Record whether Odin 2 Portal `ef201f64` is connected at the start of each installation session. Treat every live value as unconfirmed until that session captures it again.

Before bootloader entry, confirm that ADB lists `ef201f64` as `device`. Stop if ADB is unauthorized, offline, or absent. USB authorization must remain available for first-boot acceptance.

After separate approval, enter bootloader fastboot mode. Set `SERIAL=ef201f64`. Use `fastboot -s "$SERIAL"` for every command.

Read these variables before a write:

```console
fastboot -s "$SERIAL" devices -l
fastboot -s "$SERIAL" getvar unlocked
fastboot -s "$SERIAL" getvar current-slot
fastboot -s "$SERIAL" getvar is-userspace
fastboot -s "$SERIAL" getvar partition-size:super
fastboot -s "$SERIAL" getvar partition-size:vbmeta_a
fastboot -s "$SERIAL" getvar partition-size:vbmeta_system_a
fastboot -s "$SERIAL" getvar snapshot-update-status
```

Stop unless all conditions are true:

- The serial is exactly `ef201f64`.
- `unlocked` is `yes`.
- `current-slot` is `a`.
- `is-userspace` is `no`.
- The super size is `0x152877000` bytes.
- Each vbmeta size is `0x10000` bytes.
- No snapshot update is active or merging.

Record all command output before the first write.

## Proposed installation order

This section is not approved for execution yet.

Set these paths:

```console
SIGNED_ROOT=/path/to/odin2portal-stock-130-signed-avb-dry-run
SIGNED="$SIGNED_ROOT/NON_FLASHABLE_ARTIFACTS"
ROLLBACK=/path/to/odin2portal-stock-130-rollback-bundle
CONTRACT_DIR=/path/to/repository/clients/android/firmware/odin2portal/contract
```

Confirm that all paths are on stable local storage. Run the host readiness gate again:

```console
nix run .#odin2portal-install-readiness -- "$SIGNED_ROOT" "$ROLLBACK"
```

Enter userspace fastboot:

```console
fastboot -s "$SERIAL" reboot fastboot
```

Wait up to 60 seconds for the exact serial to reappear. Do not run the next command while the device is absent.

```console
fastboot -s "$SERIAL" devices -l
fastboot -s "$SERIAL" getvar is-userspace
fastboot -s "$SERIAL" getvar current-slot
fastboot -s "$SERIAL" getvar snapshot-update-status
```

Stop unless the serial matches, `is-userspace` is `yes`, `current-slot` is `a`, and no snapshot update is active or merging.

Verify the signed files again immediately before the write:

```console
cd "$SIGNED_ROOT" && sha256sum --check "$CONTRACT_DIR/signed-install-SHA256SUMS"
```

Write the complete dynamic-partition container:

```console
fastboot -s "$SERIAL" flash super "$SIGNED/super.img.not-flashable"
```

Require `OKAY`. If this command fails and userspace fastboot remains available, use the immediate super rollback branch. Do not change modes first.

Do not reboot Android after this command. Return to bootloader fastboot:

```console
fastboot -s "$SERIAL" reboot bootloader
```

Wait up to 60 seconds for the exact serial to reappear. Then read the state:

```console
fastboot -s "$SERIAL" devices -l
fastboot -s "$SERIAL" getvar is-userspace
fastboot -s "$SERIAL" getvar current-slot
fastboot -s "$SERIAL" getvar unlocked
```

Stop unless the serial matches, `is-userspace` is `no`, `current-slot` is `a`, and `unlocked` is `yes`.

Verify the signed files again immediately before the AVB writes:

```console
cd "$SIGNED_ROOT" && sha256sum --check "$CONTRACT_DIR/signed-install-SHA256SUMS"
```

Write the lower AVB image first. Write the root AVB image last:

```console
fastboot -s "$SERIAL" flash vbmeta_system_a "$SIGNED/vbmeta_system_a.img.not-flashable"
fastboot -s "$SERIAL" flash vbmeta_a "$SIGNED/vbmeta_a.img.not-flashable"
```

Require `OKAY` after each command. Stop after the first failed command. Use the AVB repair branch while bootloader fastboot remains available.

Restart only after all three writes succeed:

```console
fastboot -s "$SERIAL" reboot
```

## First-boot acceptance

Allow ten minutes for the first startup. The orange unlocked-bootloader warning is expected.

Rollback if one of these conditions occurs:

- Android does not start in ten minutes.
- The device restarts repeatedly.
- Android shows an AVB or corruption error.
- The controller does not work.
- Fan control does not work.
- LED control does not work.
- Display resolution or refresh control does not work.
- Performance-mode control does not work.

If Android starts, save the output from these read-only commands:

```console
adb devices -l
adb -s ef201f64 shell getprop ro.build.fingerprint
adb -s ef201f64 shell getprop ro.boot.vbmeta.device_state
adb -s ef201f64 shell getprop ro.boot.verifiedbootstate
adb -s ef201f64 shell getprop ro.boot.veritymode
adb -s ef201f64 shell grep -F -x '# korri marker-only dry run' /product/etc/build.prop
```

Require these values:

- The fingerprint is `qti/kalama/kalama:13/TKQ1.231222.001/eng.Odin2P.20250312.203450:user/release-keys`.
- The vbmeta device state is `unlocked`.
- The verified-boot state is `orange`.
- The verity mode is `enforcing`.
- The marker command prints the exact marker comment.

If ADB is unauthorized, offline, or absent for two minutes, acceptance fails. Start the general stock rollback.

Record a pass or fail result for each manual hardware test:

1. Test every controller button and both analog sticks.
2. Test each fan mode.
3. Test LED color, brightness, and off controls.
4. Test each available display resolution and refresh control.
5. Test each performance mode.

Each control must behave as it did on the captured stock build. Do not lock the bootloader after a successful test.

## Launcher follow-up installation

Use this section only after the signed marker image has the recorded final result `ODIN2PORTAL_CONTROLLED_INSTALL_PASS`. This follow-up installs Korri under `/product/app/Korri/Korri.apk`. It retains the marker, AYN launcher, Android Settings, and AYN hardware services. It does not select Korri as HOME.

Set these paths:

```console
LAUNCHER_ROOT=/path/to/odin2portal-korri-launcher-signed-avb-dry-run
LAUNCHER="$LAUNCHER_ROOT/NON_FLASHABLE_ARTIFACTS"
ROLLBACK=/path/to/odin2portal-stock-130-rollback-bundle
CONTRACT_DIR=/path/to/repository/clients/android/firmware/odin2portal/contract
```

Run the dedicated host-only gate:

```console
nix run .#odin2portal-launcher-install-readiness -- \
  "$LAUNCHER_ROOT" "$ROLLBACK"
```

Require the final line `ODIN2PORTAL_LAUNCHER_INSTALL_ARTIFACTS_READY`. This result does not approve a device write.

Before bootloader entry, repeat the ADB identity, fingerprint, Verified Boot, verity, marker, and battery gates. Record all output. After separate approval, enter bootloader fastboot. Repeat every condition in the device readiness gate, including the exact serial, unlocked state, slot A, partition sizes, and inactive snapshot update.

Enter userspace fastboot:

```console
fastboot -s "$SERIAL" reboot fastboot
```

Wait up to 60 seconds for the exact serial. Require userspace fastboot, slot A, and no active or merging snapshot. Verify the launcher files immediately before the write:

```console
cd "$LAUNCHER_ROOT" && \
  sha256sum --check "$CONTRACT_DIR/launcher-install-SHA256SUMS"
```

Write the complete launcher dynamic-partition container:

```console
fastboot -s "$SERIAL" flash super "$LAUNCHER/super.img.not-flashable"
```

Require `OKAY`. If this command fails and userspace fastboot remains available, use the immediate super rollback branch. Do not change modes first.

Do not reboot Android. Return to bootloader fastboot:

```console
fastboot -s "$SERIAL" reboot bootloader
```

Wait up to 60 seconds for the exact serial. Require bootloader fastboot, slot A, and the unlocked state. Verify the launcher files again:

```console
cd "$LAUNCHER_ROOT" && \
  sha256sum --check "$CONTRACT_DIR/launcher-install-SHA256SUMS"
```

Write the lower AVB image first. Write the root AVB image last:

```console
fastboot -s "$SERIAL" flash vbmeta_system_a "$LAUNCHER/vbmeta_system_a.img.not-flashable"
fastboot -s "$SERIAL" flash vbmeta_a "$LAUNCHER/vbmeta_a.img.not-flashable"
```

Require `OKAY` after each command. Stop after the first failure and use the AVB repair branch while bootloader fastboot remains available. Restart only after all three writes succeed:

```console
fastboot -s "$SERIAL" reboot
```

Allow ten minutes for startup. Run the read-only host acceptance gate:

```console
nix run .#odin2portal-launcher-device-acceptance -- \
  ef201f64 \
  /path/to/odin2portal-launcher-acceptance-ef201f64
```

The evidence path must not exist. Require the final line `ODIN2PORTAL_LAUNCHER_DEVICE_HOST_GATES_PASS`. The command pulls the installed product APK to temporary host storage, verifies its exact approved APK SHA-256 and signer certificate, then deletes the temporary APK. It does not write to the device.

Repeat every manual hardware acceptance gate. Also require all of these launcher-specific results before HOME provisioning:

- Korri has exactly one package path, `/product/app/Korri/Korri.apk`, and no path under `/data/app`.
- The installed Korri APK and signer match `contract/korri-launcher-apk-SHA256.txt` and `contract/korri-release-cert-SHA256.txt`.
- HOME candidates include `com.simonwjackson.korri/com.limelight.KorriShellActivity` and `com.odin.odinlauncher/.activities.LauncherActivity`.
- The resolved HOME remains `com.odin.odinlauncher/.activities.LauncherActivity` before provisioning.
- Android Settings and every AYN hardware control pass again.

Rollback the complete stock `super`, `vbmeta_system_a`, and `vbmeta_a` set if any launcher-image boot, package, signer, HOME-candidate, or hardware gate fails. Do not provision HOME after a failed gate.

After launcher-image acceptance, read `HOME-PROVISIONING.md`. Its one package-manager write requires a new approval and keeps the AYN launcher installed as fallback.

## Immediate super rollback

Use this branch only if the custom super write fails and userspace fastboot remains available.

Read the state again:

```console
fastboot -s "$SERIAL" devices -l
fastboot -s "$SERIAL" getvar is-userspace
fastboot -s "$SERIAL" getvar current-slot
fastboot -s "$SERIAL" getvar snapshot-update-status
```

Stop unless the serial matches, `is-userspace` is `yes`, `current-slot` is `a`, and no snapshot update is active or merging.

Verify the stock bundle again immediately before the write:

```console
cd "$ROLLBACK" && sha256sum --check SHA256SUMS.local
```

```console
fastboot -s "$SERIAL" flash super "$ROLLBACK/super.img"
```

Require `OKAY`. Do not reboot Android. Return to bootloader fastboot and confirm the stock AVB images before a restart.

## AVB repair branch

Use this branch if an AVB write fails and bootloader fastboot remains available.

Confirm the exact serial, bootloader fastboot mode, slot A, and unlocked state. Stop if one value is different.

Verify the stock bundle again:

```console
cd "$ROLLBACK" && sha256sum --check SHA256SUMS.local
```

Write the stock lower AVB image first. Write the stock root image last:

```console
fastboot -s "$SERIAL" flash vbmeta_system_a "$ROLLBACK/vbmeta_system_a.img"
fastboot -s "$SERIAL" flash vbmeta_a "$ROLLBACK/vbmeta_a.img"
```

Require `OKAY` after each command. Then continue with the general stock rollback. Do not reboot Android first.

## Slot-A recovery branch

The captured super image has empty slot-B logical partitions. A failed boot can select slot B.

If `current-slot` is `b`, stop the normal procedure. A separate recovery approval must permit this command:

```console
fastboot -s "$SERIAL" set_active a
```

After approval, require `OKAY`. Read `current-slot` again. Stop unless it is `a`. Then continue with stock rollback.

Do not write slot B.

## General stock rollback

Use this rollback after a failed boot. Also use it after AVB repair or immediate super rollback.

Enter bootloader fastboot mode. Confirm the exact serial and the unlocked state. If slot B is active, use the Slot-A recovery branch first.

Enter userspace fastboot:

```console
fastboot -s "$SERIAL" reboot fastboot
```

Wait up to 60 seconds for the exact serial to reappear. Then read the state:

```console
fastboot -s "$SERIAL" devices -l
fastboot -s "$SERIAL" getvar is-userspace
fastboot -s "$SERIAL" getvar current-slot
fastboot -s "$SERIAL" getvar snapshot-update-status
```

Stop unless the serial matches, `is-userspace` is `yes`, `current-slot` is `a`, and no snapshot update is active or merging.

Verify the stock bundle again immediately before the write:

```console
cd "$ROLLBACK" && sha256sum --check SHA256SUMS.local
```

```console
fastboot -s "$SERIAL" flash super "$ROLLBACK/super.img"
```

Require `OKAY`. Stop after a failed command.

Return to bootloader fastboot:

```console
fastboot -s "$SERIAL" reboot bootloader
```

Wait up to 60 seconds for the exact serial to reappear. Then read the state:

```console
fastboot -s "$SERIAL" devices -l
fastboot -s "$SERIAL" getvar is-userspace
fastboot -s "$SERIAL" getvar current-slot
fastboot -s "$SERIAL" getvar unlocked
```

Stop unless the serial matches, `is-userspace` is `no`, `current-slot` is `a`, and `unlocked` is `yes`.

Verify the stock bundle again immediately before the AVB writes:

```console
cd "$ROLLBACK" && sha256sum --check SHA256SUMS.local
```

Write the stock AVB images. Write the stock root image last:

```console
fastboot -s "$SERIAL" flash vbmeta_system_a "$ROLLBACK/vbmeta_system_a.img"
```

Require `OKAY`. Then run:

```console
fastboot -s "$SERIAL" flash vbmeta_a "$ROLLBACK/vbmeta_a.img"
```

Require `OKAY`. Restart only after all rollback writes succeed:

```console
fastboot -s "$SERIAL" reboot
```

After rollback, confirm the stock fingerprint and all AYN hardware controls.

## Escalation limit

Stop if bootloader fastboot or userspace fastboot is not available. Do not use QFIL or EDL without separate approval. Do not change GPT data or low-level bootchain firmware.
