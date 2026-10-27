#!/usr/bin/env bash
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACT="$HERE/contract"
SERIAL="${1:?usage: launcher-device-acceptance.sh <adb-serial> <evidence-directory>}"
EVIDENCE="${2:?usage: launcher-device-acceptance.sh <adb-serial> <evidence-directory>}"
EXPECTED_SERIAL="$(cat "$CONTRACT/device-serial.txt")"
EXPECTED_FINGERPRINT="$(cat "$CONTRACT/build-fingerprint.txt")"
EXPECTED_APK_SHA256="$(cat "$CONTRACT/korri-launcher-apk-SHA256.txt")"
EXPECTED_CERT_SHA256="$(cat "$CONTRACT/korri-release-cert-SHA256.txt")"
EXPECTED_APK_PATH='package:/product/app/Korri/Korri.apk'
EXPECTED_KORRI_HOME='com.simonwjackson.korri/com.limelight.KorriShellActivity'
EXPECTED_AYN_HOME='com.odin.odinlauncher/.activities.LauncherActivity'
ANDROID_HOME="${ANDROID_HOME:?run through a Nix Android task}"
APKSIGNER="$(find -L "$ANDROID_HOME/build-tools" -mindepth 2 -maxdepth 2 -name apksigner -type f -print | sort -V | tail -n1)"
[[ -x "$APKSIGNER" ]] || {
  echo 'Android apksigner is unavailable' >&2
  exit 1
}

[[ "$SERIAL" == "$EXPECTED_SERIAL" ]] || {
  echo "device serial does not match its contract: $SERIAL" >&2
  exit 1
}
[[ ! -e "$EVIDENCE" && ! -L "$EVIDENCE" ]] || {
  echo "evidence path already exists: $EVIDENCE" >&2
  exit 1
}

PARENT="$(dirname "$EVIDENCE")"
mkdir -p "$PARENT"
PARENT="$(cd "$PARENT" && pwd -P)"
EVIDENCE="$PARENT/$(basename "$EVIDENCE")"
STAGED="$(mktemp -d "$PARENT/.odin2portal-launcher-acceptance.XXXXXX")"
WORK="$(mktemp -d)"
cleanup() {
  rm -rf "$STAGED" "$WORK"
}
trap cleanup EXIT

adb -s "$SERIAL" get-state | grep -Fx 'device' >/dev/null
adb devices -l > "$STAGED/adb-devices.txt"
grep -E "^${SERIAL}[[:space:]]+device[[:space:]].*model:Odin2_Portal" \
  "$STAGED/adb-devices.txt" >/dev/null

read_property() {
  adb -s "$SERIAL" shell getprop "$1" | tr -d '\r'
}
FINGERPRINT="$(read_property ro.build.fingerprint)"
DEVICE_STATE="$(read_property ro.boot.vbmeta.device_state)"
VERIFIED_BOOT_STATE="$(read_property ro.boot.verifiedbootstate)"
VERITY_MODE="$(read_property ro.boot.veritymode)"
BOOT_COMPLETED="$(read_property sys.boot_completed)"
DEVICE_SERIAL="$(read_property ro.serialno)"
DEVICE_MODEL="$(read_property ro.product.model)"

[[ "$DEVICE_SERIAL" == "$EXPECTED_SERIAL" ]]
[[ "$DEVICE_MODEL" == 'Odin2 Portal' ]]
[[ "$FINGERPRINT" == "$EXPECTED_FINGERPRINT" ]]
[[ "$DEVICE_STATE" == 'unlocked' ]]
[[ "$VERIFIED_BOOT_STATE" == 'orange' ]]
[[ "$VERITY_MODE" == 'enforcing' ]]
[[ "$BOOT_COMPLETED" == '1' ]]

MARKER="$(adb -s "$SERIAL" shell "grep -F -x '# korri marker-only dry run' /product/etc/build.prop" | tr -d '\r')"
[[ "$MARKER" == '# korri marker-only dry run' ]]

PACKAGE_PATHS="$(adb -s "$SERIAL" shell pm path com.simonwjackson.korri 2>/dev/null | tr -d '\r' || true)"
[[ "$PACKAGE_PATHS" == "$EXPECTED_APK_PATH" ]] || {
  echo 'Korri is not installed at the approved product path' >&2
  printf '%s\n' "$PACKAGE_PATHS" >&2
  exit 1
}
APK_DEVICE_PATH="${PACKAGE_PATHS#package:}"
adb -s "$SERIAL" pull "$APK_DEVICE_PATH" "$WORK/Korri.apk" > "$STAGED/adb-pull.txt" 2>&1
ACTUAL_APK_SHA256="$(sha256sum "$WORK/Korri.apk" | awk '{print $1}')"
[[ "$ACTUAL_APK_SHA256" == "$EXPECTED_APK_SHA256" ]] || {
  echo 'installed Korri APK does not match the approved APK contract' >&2
  exit 1
}

"$APKSIGNER" verify --verbose --print-certs "$WORK/Korri.apk" > "$STAGED/apksigner.txt"
SIGNER_COUNT="$(sed -n 's/^Number of signers: //p' "$STAGED/apksigner.txt" | head -n1 | tr -d '[:space:]')"
[[ "$SIGNER_COUNT" == 1 ]] || {
  echo 'installed Korri APK must have exactly one signer' >&2
  exit 1
}
ACTUAL_CERT_SHA256="$(sed -n -e 's/^Signer #1 certificate SHA-256 digest: //p' -e 's/^V[0-9.]* Signer: certificate SHA-256 digest: //p' "$STAGED/apksigner.txt" | tr '[:upper:]' '[:lower:]' | tr -d ':[:space:]' | sort -u)"
[[ "$ACTUAL_CERT_SHA256" == "$EXPECTED_CERT_SHA256" ]] || {
  echo 'installed Korri APK signer does not match the release certificate contract' >&2
  exit 1
}

adb -s "$SERIAL" shell cmd package query-activities --brief \
  -a android.intent.action.MAIN -c android.intent.category.HOME \
  | tr -d '\r' > "$STAGED/home-candidates.txt"
grep -F "$EXPECTED_KORRI_HOME" "$STAGED/home-candidates.txt" >/dev/null
grep -F "$EXPECTED_AYN_HOME" "$STAGED/home-candidates.txt" >/dev/null

adb -s "$SERIAL" shell cmd package resolve-activity --brief \
  -a android.intent.action.MAIN -c android.intent.category.HOME \
  | tr -d '\r' > "$STAGED/resolved-home.txt"
RESOLVED_HOME="$(tail -n1 "$STAGED/resolved-home.txt")"
[[ "$RESOLVED_HOME" == "$EXPECTED_AYN_HOME" ]] || {
  echo "preferred HOME changed before approval: $RESOLVED_HOME" >&2
  exit 1
}

SETTINGS_PATH="$(adb -s "$SERIAL" shell pm path com.android.settings | tr -d '\r')"
AYN_LAUNCHER_PATH="$(adb -s "$SERIAL" shell pm path com.odin.odinlauncher | tr -d '\r')"
[[ "$SETTINGS_PATH" == package:*Settings*.apk ]]
[[ "$AYN_LAUNCHER_PATH" == package:*OdinLauncher*.apk ]]
adb -s "$SERIAL" shell dumpsys package com.simonwjackson.korri \
  | tr -d '\r' > "$STAGED/korri-package.txt"
adb -s "$SERIAL" shell dumpsys battery \
  | tr -d '\r' > "$STAGED/battery.txt"

cat > "$STAGED/device-state.txt" <<EOF
captured: $(date --iso-8601=seconds)
device serial: $DEVICE_SERIAL
device model: $DEVICE_MODEL
ro.build.fingerprint=$FINGERPRINT
ro.boot.vbmeta.device_state=$DEVICE_STATE
ro.boot.verifiedbootstate=$VERIFIED_BOOT_STATE
ro.boot.veritymode=$VERITY_MODE
sys.boot_completed=$BOOT_COMPLETED
marker=$MARKER
Korri package path=$PACKAGE_PATHS
Korri APK SHA-256=$ACTUAL_APK_SHA256
Korri signer SHA-256=$ACTUAL_CERT_SHA256
resolved HOME=$RESOLVED_HOME
Android Settings path=$SETTINGS_PATH
AYN launcher path=$AYN_LAUNCHER_PATH
device writes: none
manual hardware acceptance: required
HOME provisioning approved: no
EOF

printf '%s  %s\n' "$ACTUAL_APK_SHA256" 'Korri.apk' > "$STAGED/apk-SHA256SUMS"
cat > "$STAGED/RESULT.txt" <<'EOF'
ODIN2PORTAL_LAUNCHER_DEVICE_HOST_GATES_PASS
device writes: none
manual hardware acceptance: required
HOME provisioning approved: no
EOF

find "$STAGED" -type f -exec chmod 0600 {} +
chmod 0700 "$STAGED"
mv "$STAGED" "$EVIDENCE"
trap 'rm -rf "$WORK"' EXIT
printf 'Evidence: %s\n' "$EVIDENCE"
printf 'ODIN2PORTAL_LAUNCHER_DEVICE_HOST_GATES_PASS\n'
