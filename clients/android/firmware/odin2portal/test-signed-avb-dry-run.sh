#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRY_RUN="$HERE/signed-avb-dry-run.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

python3 "$HERE/test-extract-vbmeta-public-key.py"
python3 "$HERE/test-redact-evidence-paths.py"
[[ -x "$DRY_RUN" ]]
if grep -E '(^|[[:space:]])(adb|fastboot)([[:space:]]|$)' "$DRY_RUN" >/dev/null; then
  echo 'signed AVB dry run contains a device command' >&2
  exit 1
fi

mkdir "$TMP/source"
printf 'not-a-key\n' > "$TMP/key.pem"
chmod 0644 "$TMP/key.pem"
if "$DRY_RUN" "$TMP/source" "$TMP/key.pem" "$TMP/source/output" >"$TMP/inside.stdout" 2>"$TMP/inside.stderr"; then
  echo 'signed AVB dry run accepted output inside source' >&2
  exit 1
fi
grep -F 'output must be outside the source directory' "$TMP/inside.stderr" >/dev/null

mkdir "$TMP/existing-output"
if "$DRY_RUN" "$TMP/source" "$TMP/key.pem" "$TMP/existing-output" >"$TMP/existing.stdout" 2>"$TMP/existing.stderr"; then
  echo 'signed AVB dry run accepted existing output' >&2
  exit 1
fi
grep -F 'output already exists' "$TMP/existing.stderr" >/dev/null

if "$DRY_RUN" "$TMP/source" "$TMP/key.pem" "$TMP/key-mode-output" >"$TMP/key.stdout" 2>"$TMP/key.stderr"; then
  echo 'signed AVB dry run accepted an unsafe private-key mode' >&2
  exit 1
fi
grep -F 'private key must be a regular non-symbolic file with mode 0600' "$TMP/key.stderr" >/dev/null

openssl genpkey -quiet -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$TMP/rsa-2048.pem"
chmod 0600 "$TMP/rsa-2048.pem"
if "$DRY_RUN" "$TMP/source" "$TMP/rsa-2048.pem" "$TMP/key-size-output" >"$TMP/key-size.stdout" 2>"$TMP/key-size.stderr"; then
  echo 'signed AVB dry run accepted an RSA-2048 private key' >&2
  exit 1
fi
grep -F 'private key must be RSA-4096' "$TMP/key-size.stderr" >/dev/null

if [[ -z "${ODIN2PORTAL_STOCK_SOURCE:-}" ||
      -z "${ODIN2PORTAL_AVB_PRIVATE_KEY:-}" ]]; then
  printf 'odin2portal signed AVB integration skipped: set both private input variables\n'
  exit 0
fi

mkdir "$TMP/device-tools"
cp "$HERE/test/fixtures/bin/adb" "$HERE/test/fixtures/bin/fastboot" "$TMP/device-tools/"
chmod +x "$TMP/device-tools/adb" "$TMP/device-tools/fastboot"
export FAKE_TOOL_LOG="$TMP/device-tools.log"
: > "$FAKE_TOOL_LOG"
export PATH="$TMP/device-tools:$PATH"
openssl genpkey -quiet -algorithm RSA -pkeyopt rsa_keygen_bits:4096 -out "$TMP/wrong-rsa-4096.pem"
chmod 0600 "$TMP/wrong-rsa-4096.pem"
if "$DRY_RUN" "$ODIN2PORTAL_STOCK_SOURCE" "$TMP/wrong-rsa-4096.pem" "$TMP/wrong-key-output" >"$TMP/wrong-key.stdout" 2>"$TMP/wrong-key.stderr"; then
  echo 'signed AVB dry run accepted an unintended RSA-4096 key' >&2
  exit 1
fi
[[ ! -e "$TMP/wrong-key-output" ]]
grep -F 'private key does not match the selected Korri AVB key' "$TMP/wrong-key.stderr" >/dev/null

"$DRY_RUN" \
  "$ODIN2PORTAL_STOCK_SOURCE" \
  "$ODIN2PORTAL_AVB_PRIVATE_KEY" \
  "$TMP/output"
grep -Fx 'ODIN2PORTAL_SIGNED_AVB_DRY_RUN_VERIFIED' "$TMP/output/RESULT.txt" >/dev/null
grep -Fx 'private key included: no' "$TMP/output/RESULT.txt" >/dev/null
grep -Fx 'bootloader required: unlocked' "$TMP/output/RESULT.txt" >/dev/null
grep -Fx 'flash ready: no' "$TMP/output/RESULT.txt" >/dev/null
[[ -f "$TMP/output/NON_FLASHABLE_ARTIFACTS/super.img.not-flashable" ]]
[[ -f "$TMP/output/NON_FLASHABLE_ARTIFACTS/vbmeta_a.img.not-flashable" ]]
[[ -f "$TMP/output/NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable" ]]
[[ ! -e "$TMP/output/signing.pem" ]]
(
  cd "$TMP/output"
  sha256sum --check evidence/output-SHA256SUMS
  sha256sum --check MANIFEST-SHA256SUMS
)
avbtool info_image --image "$ODIN2PORTAL_STOCK_SOURCE/vbmeta_a.img" > "$TMP/stock-vbmeta.txt"
avbtool info_image --image "$TMP/output/NON_FLASHABLE_ARTIFACTS/vbmeta_a.img.not-flashable" > "$TMP/custom-vbmeta.txt"
stock_root_key="$(awk '/^Public key \(sha1\):/ {print $4; exit}' "$TMP/stock-vbmeta.txt")"
boot_chain_key="$(awk '/Partition Name:/ {partition=$3} /Public key \(sha1\):/ && partition=="boot" {print $4; exit}' "$TMP/custom-vbmeta.txt")"
recovery_chain_key="$(awk '/Partition Name:/ {partition=$3} /Public key \(sha1\):/ && partition=="recovery" {print $4; exit}' "$TMP/custom-vbmeta.txt")"
system_chain_key="$(awk '/Partition Name:/ {partition=$3} /Public key \(sha1\):/ && partition=="vbmeta_system" {print $4; exit}' "$TMP/custom-vbmeta.txt")"
custom_public_key="$(sha1sum "$TMP/output/korri-odin2portal-avb.avbpubkey" | awk '{print $1}')"
[[ "$boot_chain_key" == "$stock_root_key" ]]
[[ "$recovery_chain_key" == "$stock_root_key" ]]
[[ "$system_chain_key" == "$custom_public_key" ]]
[[ ! -s "$FAKE_TOOL_LOG" ]]
printf 'odin2portal signed AVB integration passed\n'
