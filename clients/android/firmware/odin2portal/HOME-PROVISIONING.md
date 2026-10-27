# Korri HOME provisioning for Odin 2 Portal

This procedure is not executed by the image build. The image keeps the AYN launcher and Android Settings. It only adds Korri as another HOME candidate. Changing user 0's preferred HOME writes package-manager state under `userdata` and requires separate approval.

Use only Odin 2 Portal `ef201f64` on build `Odin2Portal_V1.0.0.130_20250312_205701_user`. Keep the bootloader unlocked.

## Read-only gates

Run all ADB commands with the exact serial. Stop unless the device is authorized and each value is exact.

```console
SERIAL=ef201f64
adb -s "$SERIAL" get-state
adb -s "$SERIAL" shell getprop ro.build.fingerprint
adb -s "$SERIAL" shell getprop ro.boot.vbmeta.device_state
adb -s "$SERIAL" shell getprop ro.boot.verifiedbootstate
adb -s "$SERIAL" shell getprop ro.boot.veritymode
adb -s "$SERIAL" shell pm path com.simonwjackson.korri
adb -s "$SERIAL" shell dumpsys package com.simonwjackson.korri
adb -s "$SERIAL" shell cmd package query-activities --brief -a android.intent.action.MAIN -c android.intent.category.HOME
adb -s "$SERIAL" shell cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME
adb -s "$SERIAL" shell pm path com.android.settings
adb -s "$SERIAL" shell pm path com.odin.odinlauncher
```

Require:

- the captured stock fingerprint;
- `unlocked`, `orange`, and `enforcing` boot properties;
- Korri code under `/product/app/Korri/`;
- Korri signer SHA-256 `f46183b71944a33c4c3d2fde42471846ff8d41d22f33b14c1dcf2265d1c7e8ad`;
- HOME candidates include both `com.simonwjackson.korri/com.limelight.KorriShellActivity` and `com.odin.odinlauncher/.activities.LauncherActivity`;
- Android Settings, OdinSettings, fan, LED, controller, display, and performance services remain present.

Stop if a `/data/app` Korri package shadows the product package, if its signer differs, or if any stock AYN component is absent.

## Separately approved user-state write

Capture the complete terminal transcript. After separate approval, run exactly one package-manager write:

```console
adb -s "$SERIAL" shell cmd package set-home-activity --user 0 \
  com.simonwjackson.korri/com.limelight.KorriShellActivity
```

Do not disable, uninstall, suspend, or replace the AYN launcher.

Read the selected HOME back immediately:

```console
adb -s "$SERIAL" shell cmd package resolve-activity --brief \
  -a android.intent.action.MAIN -c android.intent.category.HOME
```

Require `com.simonwjackson.korri/com.limelight.KorriShellActivity`. Press HOME and require Korri to open. Confirm Android Settings and every AYN hardware control still works.

## Restore the AYN launcher

This is also a user-state write and requires approval:

```console
adb -s "$SERIAL" shell cmd package set-home-activity --user 0 \
  com.odin.odinlauncher/.activities.LauncherActivity
adb -s "$SERIAL" shell cmd package resolve-activity --brief \
  -a android.intent.action.MAIN -c android.intent.category.HOME
```

Require `com.odin.odinlauncher/.activities.LauncherActivity`. This fallback does not modify a partition and does not lock the bootloader.
