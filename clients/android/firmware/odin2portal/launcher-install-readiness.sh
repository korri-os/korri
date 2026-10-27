#!/usr/bin/env bash
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LAUNCHER="${1:?usage: launcher-install-readiness.sh <launcher-output> <rollback-bundle>}"
ROLLBACK="${2:?usage: launcher-install-readiness.sh <launcher-output> <rollback-bundle>}"
CONTRACT="$HERE/contract"
VERIFY_APK_EVIDENCE="$HERE/verify-launcher-apk-evidence.sh"

[[ -x "$VERIFY_APK_EVIDENCE" && ! -L "$VERIFY_APK_EVIDENCE" ]] || {
  echo 'launcher APK evidence verifier is missing or symbolic' >&2
  exit 1
}
for directory in "$LAUNCHER" "$ROLLBACK"; do
  [[ -d "$directory" && ! -L "$directory" ]] || {
    echo "input directory is missing or symbolic: $directory" >&2
    exit 1
  }
done
LAUNCHER="$(cd "$LAUNCHER" && pwd -P)"
ROLLBACK="$(cd "$ROLLBACK" && pwd -P)"

launcher_required=(
  MANIFEST-SHA256SUMS HOME-PROVISIONING.md NOT-FLASHABLE.md RESULT.txt
  evidence/output-SHA256SUMS
  evidence/extracted-apk-verification/apk-SHA256SUMS
  korri-odin2portal-avb.avbpubkey
  NON_FLASHABLE_ARTIFACTS/super.img.not-flashable
  NON_FLASHABLE_ARTIFACTS/vbmeta_a.img.not-flashable
  NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable
)
rollback_required=(
  SHA256SUMS.local active-slot.txt build-fingerprint.txt build-id.txt
  super-layout.txt super.img vbmeta_a.img vbmeta_system_a.img
)
for file in "${launcher_required[@]}"; do
  [[ -f "$LAUNCHER/$file" && ! -L "$LAUNCHER/$file" ]] || {
    echo "launcher output is incomplete or symbolic: $file" >&2
    exit 1
  }
done
for file in "${rollback_required[@]}"; do
  [[ -f "$ROLLBACK/$file" && ! -L "$ROLLBACK/$file" ]] || {
    echo "rollback bundle is incomplete or symbolic: $file" >&2
    exit 1
  }
done

(
  cd "$LAUNCHER"
  sha256sum --check MANIFEST-SHA256SUMS
  sha256sum --check evidence/output-SHA256SUMS
  sha256sum --check "$CONTRACT/launcher-install-SHA256SUMS"
) >/dev/null
diff -u "$CONTRACT/launcher-install-SHA256SUMS" \
  "$LAUNCHER/evidence/output-SHA256SUMS" >/dev/null
"$VERIFY_APK_EVIDENCE" \
  "$LAUNCHER/evidence/extracted-apk-verification/apk-SHA256SUMS" \
  "$CONTRACT/korri-launcher-apk-SHA256.txt"
grep -Fx 'ODIN2PORTAL_LAUNCHER_IMAGE_DRY_RUN_VERIFIED' "$LAUNCHER/RESULT.txt" >/dev/null
grep -Fx 'Korri APK path: /product/app/Korri/Korri.apk' "$LAUNCHER/RESULT.txt" >/dev/null
grep -Fx 'AYN launcher: retained as fallback' "$LAUNCHER/RESULT.txt" >/dev/null
grep -Fx 'default home provisioned: no' "$LAUNCHER/RESULT.txt" >/dev/null
grep -Fx 'private key included: no' "$LAUNCHER/RESULT.txt" >/dev/null
grep -Fx 'bootloader required: unlocked' "$LAUNCHER/RESULT.txt" >/dev/null
grep -Fx 'flash ready: no' "$LAUNCHER/RESULT.txt" >/dev/null
grep -Fx 'device writes: none' "$LAUNCHER/RESULT.txt" >/dev/null

check_contract_hash() {
  local contract_name="$1"
  local file="$2"
  local expected actual
  expected="$(awk -v name="$contract_name" '$2 == name {print $1}' "$CONTRACT/launcher-install-operator-files-SHA256SUMS")"
  actual="$(sha256sum "$file" | awk '{print $1}')"
  if [[ ! "$expected" =~ ^[0-9a-f]{64}$ || "$actual" != "$expected" ]]; then
    echo "operator file does not match its contract: $contract_name" >&2
    exit 1
  fi
}
check_contract_hash launcher/MANIFEST-SHA256SUMS "$LAUNCHER/MANIFEST-SHA256SUMS"
check_contract_hash launcher/RESULT.txt "$LAUNCHER/RESULT.txt"
check_contract_hash launcher/NOT-FLASHABLE.md "$LAUNCHER/NOT-FLASHABLE.md"
check_contract_hash launcher/HOME-PROVISIONING.md "$LAUNCHER/HOME-PROVISIONING.md"
check_contract_hash rollback/README.md "$ROLLBACK/README.md"

(
  cd "$ROLLBACK"
  sha256sum --check SHA256SUMS.local
) >/dev/null
diff -u "$CONTRACT/SHA256SUMS" "$ROLLBACK/SHA256SUMS.local" >/dev/null
cmp "$CONTRACT/active-slot.txt" "$ROLLBACK/active-slot.txt"
cmp "$CONTRACT/build-id.txt" "$ROLLBACK/build-id.txt"
cmp "$CONTRACT/build-fingerprint.txt" "$ROLLBACK/build-fingerprint.txt"
cmp "$CONTRACT/super-layout.txt" "$ROLLBACK/super-layout.txt"

if find "$LAUNCHER" "$ROLLBACK" -type l -print -quit | grep . >/dev/null; then
  echo 'artifact directories contain a symbolic link' >&2
  exit 1
fi
diff -u \
  <(find "$LAUNCHER" -type f -printf '%P\n' | sort) \
  <(sort "$CONTRACT/launcher-install-inventory.txt") \
  >/dev/null || {
    echo 'launcher output contains an unmanifested or missing file' >&2
    exit 1
  }
diff -u \
  <(find "$ROLLBACK" -type f -printf '%P\n' | sort) \
  <({ awk '{print $2}' "$CONTRACT/SHA256SUMS"; printf '%s\n' active-slot.txt build-fingerprint.txt build-id.txt README.md SHA256SUMS.local super-layout.txt; } | sort) \
  >/dev/null || {
    echo 'rollback bundle contains an unexpected or missing file' >&2
    exit 1
  }

check_size() {
  local file="$1"
  local expected="$2"
  local actual
  actual="$(stat -c %s "$file")"
  if [[ "$actual" -ne "$expected" ]]; then
    echo "unexpected image size: $file has $actual bytes, expected $expected" >&2
    exit 1
  fi
}
check_size "$LAUNCHER/NON_FLASHABLE_ARTIFACTS/super.img.not-flashable" 5679575040
check_size "$LAUNCHER/NON_FLASHABLE_ARTIFACTS/vbmeta_a.img.not-flashable" 65536
check_size "$LAUNCHER/NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable" 65536
check_size "$ROLLBACK/super.img" 5679575040
check_size "$ROLLBACK/vbmeta_a.img" 65536
check_size "$ROLLBACK/vbmeta_system_a.img" 65536

if find "$LAUNCHER" -type f \( -iname '*.pem' -o -iname '*private*' \) -print -quit | grep . >/dev/null; then
  echo 'launcher output contains a private-key-shaped path' >&2
  exit 1
fi
if find "$LAUNCHER" -type f -size -16777216c -exec \
  grep -IlE -- '-----BEGIN (ENCRYPTED |RSA )?PRIVATE KEY-----' {} + | grep . >/dev/null; then
  echo 'launcher output contains private-key PEM data' >&2
  exit 1
fi

printf '%s\n' \
  "device serial contract: $(cat "$CONTRACT/device-serial.txt")" \
  'launcher artifacts: verified' \
  'stock rollback artifacts: verified' \
  'active slot contract: a' \
  'bootloader requirement: unlocked' \
  'required installed baseline: accepted signed marker image' \
  'default HOME change: separate approval required' \
  'device connection: not checked' \
  'device writes: none' \
  'installation approved: no' \
  'ODIN2PORTAL_LAUNCHER_INSTALL_ARTIFACTS_READY'
