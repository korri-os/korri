#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPACK="$HERE/repack-core.sh"
FIXTURE_BIN="$HERE/test/fixtures/bin"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

SOURCE="$TMP/source"
CONTRACT="$TMP/contract"
EXPECTED_LOGICAL="$TMP/expected-logical"
mkdir -p "$SOURCE" "$CONTRACT" "$EXPECTED_LOGICAL"

source_images=(
  boot_a.img boot_b.img dtbo_a.img dtbo_b.img init_boot_a.img init_boot_b.img
  recovery_a.img recovery_b.img super.img vbmeta_a.img vbmeta_b.img
  vbmeta_system_a.img vbmeta_system_b.img vendor_boot_a.img vendor_boot_b.img
)
logical_partitions=(odm product system system_dlkm system_ext vendor vendor_dlkm)

for image in "${source_images[@]}"; do
  printf 'source fixture: %s\n' "$image" > "$SOURCE/$image"
done
truncate -s 4096 "$SOURCE/super.img"
printf 'fixture-build\n' > "$SOURCE/build-id.txt"
printf 'fixture/fingerprint\n' > "$SOURCE/build-fingerprint.txt"
(
  cd "$SOURCE"
  sha256sum "${source_images[@]}" > "$CONTRACT/SHA256SUMS"
)
printf 'fixture-build\n' > "$CONTRACT/build-id.txt"
printf 'fixture/fingerprint\n' > "$CONTRACT/build-fingerprint.txt"
printf 'fixture super layout\n' > "$CONTRACT/super-layout.txt"
for partition in "${logical_partitions[@]}"; do
  printf '%s-fixture\n' "$partition" > "$EXPECTED_LOGICAL/${partition}_a.img"
done
(
  cd "$EXPECTED_LOGICAL"
  sha256sum ./*.img | sed 's#  \./#  #' > "$CONTRACT/logical-SHA256SUMS"
)

export PATH="$FIXTURE_BIN:$PATH"
export FAKE_TOOL_LOG="$TMP/tool.log"
export FAKE_LAYOUT_FILE="$CONTRACT/super-layout.txt"
export FAKE_SUPER_SIZE
FAKE_SUPER_SIZE="$(stat -c %s "$SOURCE/super.img")"
chmod +x "$FIXTURE_BIN"/*

run_repack() {
  "$REPACK" "$1" "$2" "$CONTRACT"
}

source_before="$TMP/source-before.sha256"
source_after="$TMP/source-after.sha256"
find "$SOURCE" -type f -print0 | sort -z | xargs -0 sha256sum > "$source_before"
run_repack "$SOURCE" "$TMP/output"
find "$SOURCE" -type f -print0 | sort -z | xargs -0 sha256sum > "$source_after"
cmp "$source_before" "$source_after"
[[ -f "$TMP/output/super.img" ]]
[[ ! -e "$TMP/output/logical" ]]
grep -Fx 'READ_ONLY_NOOP_REPACK_VERIFIED' "$TMP/output/RESULT.txt" >/dev/null
grep -F 'source images: verified' "$TMP/output/RESULT.txt" >/dev/null
grep -F 'logical partitions: byte-identical' "$TMP/output/RESULT.txt" >/dev/null
grep -F 'super layout: identical' "$TMP/output/RESULT.txt" >/dev/null
grep -E '^[0-9a-f]{64}  super\.img$' "$TMP/output/evidence/rebuilt-super-SHA256SUMS" >/dev/null
if grep -E '(^|[[:space:]])(adb|fastboot)([[:space:]]|$)' "$FAKE_TOOL_LOG" >/dev/null; then
  echo 'read-only pipeline invoked a device-writing tool' >&2
  exit 1
fi
[[ "$(grep -c '^e2fsck ' "$FAKE_TOOL_LOG")" -eq 7 ]]
[[ "$(grep -c '^avbtool ' "$FAKE_TOOL_LOG")" -eq 9 ]]
[[ "$(grep -c '^lpmake ' "$FAKE_TOOL_LOG")" -eq 1 ]]

cp -a "$SOURCE" "$TMP/corrupt-source"
printf 'corrupt\n' >> "$TMP/corrupt-source/boot_a.img"
: > "$FAKE_TOOL_LOG"
if run_repack "$TMP/corrupt-source" "$TMP/corrupt-output" >"$TMP/corrupt.stdout" 2>"$TMP/corrupt.stderr"; then
  echo 'repack accepted a source checksum mismatch' >&2
  exit 1
fi
[[ ! -e "$TMP/corrupt-output" ]]
[[ ! -s "$FAKE_TOOL_LOG" ]]
grep -F 'source checksum verification failed' "$TMP/corrupt.stderr" >/dev/null

cp -a "$SOURCE" "$TMP/wrong-identity-source"
printf 'wrong-build\n' > "$TMP/wrong-identity-source/build-id.txt"
: > "$FAKE_TOOL_LOG"
if run_repack "$TMP/wrong-identity-source" "$TMP/wrong-identity-output" >"$TMP/wrong-identity.stdout" 2>"$TMP/wrong-identity.stderr"; then
  echo 'repack accepted the wrong initial build identity' >&2
  exit 1
fi
[[ ! -e "$TMP/wrong-identity-output" ]]
[[ ! -s "$FAKE_TOOL_LOG" ]]
grep -F 'source build identity does not match the build contract' "$TMP/wrong-identity.stderr" >/dev/null

printf 'different layout\n' > "$TMP/different-layout.txt"
export FAKE_LAYOUT_FILE="$TMP/different-layout.txt"
: > "$FAKE_TOOL_LOG"
if run_repack "$SOURCE" "$TMP/layout-output" >"$TMP/layout.stdout" 2>"$TMP/layout.stderr"; then
  echo 'repack accepted a mismatched source layout' >&2
  exit 1
fi
[[ ! -e "$TMP/layout-output" ]]
grep -F 'source super layout does not match the build contract' "$TMP/layout.stderr" >/dev/null

export FAKE_LAYOUT_FILE="$CONTRACT/super-layout.txt"
export FAKE_REBUILT_LAYOUT_FILE="$TMP/different-layout.txt"
if run_repack "$SOURCE" "$TMP/rebuilt-layout-output" >"$TMP/rebuilt-layout.stdout" 2>"$TMP/rebuilt-layout.stderr"; then
  echo 'repack accepted a mismatched rebuilt layout' >&2
  exit 1
fi
unset FAKE_REBUILT_LAYOUT_FILE
[[ ! -e "$TMP/rebuilt-layout-output" ]]
grep -F 'rebuilt super layout differs from the build contract' "$TMP/rebuilt-layout.stderr" >/dev/null

export FAKE_REBUILT_SIZE=8192
if run_repack "$SOURCE" "$TMP/rebuilt-size-output" >"$TMP/rebuilt-size.stdout" 2>"$TMP/rebuilt-size.stderr"; then
  echo 'repack accepted a mismatched rebuilt image size' >&2
  exit 1
fi
unset FAKE_REBUILT_SIZE
[[ ! -e "$TMP/rebuilt-size-output" ]]
grep -F 'rebuilt super image size differs from source' "$TMP/rebuilt-size.stderr" >/dev/null

export FAKE_CORRUPT_REBUILT_LOGICAL=1
if run_repack "$SOURCE" "$TMP/rebuilt-logical-output" >"$TMP/rebuilt-logical.stdout" 2>"$TMP/rebuilt-logical.stderr"; then
  echo 'repack accepted changed rebuilt logical bytes' >&2
  exit 1
fi
unset FAKE_CORRUPT_REBUILT_LOGICAL
[[ ! -e "$TMP/rebuilt-logical-output" ]]
grep -F 'rebuilt logical partition checksum verification failed' "$TMP/rebuilt-logical.stderr" >/dev/null

mkdir "$TMP/existing-output"
if run_repack "$SOURCE" "$TMP/existing-output" >"$TMP/existing.stdout" 2>"$TMP/existing.stderr"; then
  echo 'repack overwrote an existing output directory' >&2
  exit 1
fi
grep -F 'output already exists' "$TMP/existing.stderr" >/dev/null

export FAKE_LAYOUT_FILE="$CONTRACT/super-layout.txt"
if run_repack "$SOURCE" "$SOURCE/generated-output" >"$TMP/inside.stdout" 2>"$TMP/inside.stderr"; then
  echo 'repack allowed output inside the immutable source directory' >&2
  exit 1
fi
[[ ! -e "$SOURCE/generated-output" ]]
grep -F 'output must be outside the source directory' "$TMP/inside.stderr" >/dev/null

cp -a "$SOURCE" "$TMP/changing-source"
export FAKE_MUTATE_SOURCE="$TMP/changing-source/boot_a.img"
if run_repack "$TMP/changing-source" "$TMP/changing-output" >"$TMP/changing.stdout" 2>"$TMP/changing.stderr"; then
  echo 'repack accepted source mutation during reconstruction' >&2
  exit 1
fi
unset FAKE_MUTATE_SOURCE
[[ ! -e "$TMP/changing-output" ]]
grep -F 'source changed after initial verification' "$TMP/changing.stderr" >/dev/null

cp -a "$SOURCE" "$TMP/changing-identity-source"
export FAKE_MUTATE_SOURCE="$TMP/changing-identity-source/build-id.txt"
if run_repack "$TMP/changing-identity-source" "$TMP/changing-identity-output" >"$TMP/changing-identity.stdout" 2>"$TMP/changing-identity.stderr"; then
  echo 'repack accepted build identity mutation during reconstruction' >&2
  exit 1
fi
unset FAKE_MUTATE_SOURCE
[[ ! -e "$TMP/changing-identity-output" ]]
grep -F 'source build identity changed after initial verification' "$TMP/changing-identity.stderr" >/dev/null

export FAKE_CREATE_DESTINATION="$TMP/race-output"
if run_repack "$SOURCE" "$TMP/race-output" >"$TMP/race.stdout" 2>"$TMP/race.stderr"; then
  echo 'repack nested published output inside a concurrently created directory' >&2
  exit 1
fi
unset FAKE_CREATE_DESTINATION
[[ -d "$TMP/race-output" ]]
[[ ! -e "$TMP/race-output/super.img" ]]

export FAKE_E2FSCK_FAIL=1
if run_repack "$SOURCE" "$TMP/e2fsck-output" >"$TMP/e2fsck.stdout" 2>"$TMP/e2fsck.stderr"; then
  echo 'repack accepted a failed filesystem verification' >&2
  exit 1
fi
unset FAKE_E2FSCK_FAIL
[[ ! -e "$TMP/e2fsck-output" ]]
grep -F 'filesystem verification failed' "$TMP/e2fsck.stderr" >/dev/null

export FAKE_AVBTOOL_FAIL=1
if run_repack "$SOURCE" "$TMP/avb-output" >"$TMP/avb.stdout" 2>"$TMP/avb.stderr"; then
  echo 'repack accepted a failed AVB verification' >&2
  exit 1
fi
unset FAKE_AVBTOOL_FAIL
[[ ! -e "$TMP/avb-output" ]]
grep -F 'AVB verification failed' "$TMP/avb.stderr" >/dev/null

shopt -s nullglob
staging_leftovers=("$TMP"/.odin2portal-stock-repack.*)
[[ "${#staging_leftovers[@]}" -eq 0 ]]

printf 'odin2portal stock repack tests passed\n'
