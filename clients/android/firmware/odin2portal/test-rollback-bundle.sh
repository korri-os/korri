#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STAGE="$HERE/rollback-bundle-core.sh"
FIXTURE_BIN="$HERE/test/fixtures/bin"
ROLLBACK_TOOLS="$HERE/test/rollback-tools"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

SOURCE="$TMP/source"
CONTRACT="$TMP/contract"
PROCEDURE="$TMP/README.md"
mkdir -p "$SOURCE" "$CONTRACT"

images=(
  boot_a.img boot_b.img dtbo_a.img dtbo_b.img init_boot_a.img init_boot_b.img
  recovery_a.img recovery_b.img super.img vbmeta_a.img vbmeta_b.img
  vbmeta_system_a.img vbmeta_system_b.img vendor_boot_a.img vendor_boot_b.img
)
for image in "${images[@]}"; do
  printf 'rollback fixture: %s\n' "$image" > "$SOURCE/$image"
done
printf 'fixture-build\n' > "$SOURCE/build-id.txt"
printf 'fixture/fingerprint\n' > "$SOURCE/build-fingerprint.txt"
printf '_a\n' > "$SOURCE/active-slot.txt"
printf 'fixture super layout\n' > "$SOURCE/super-layout.txt"
(
  cd "$SOURCE"
  sha256sum "${images[@]}" > SHA256SUMS.local
)
cp "$SOURCE/SHA256SUMS.local" "$CONTRACT/SHA256SUMS"
cp "$SOURCE/build-id.txt" "$CONTRACT/build-id.txt"
cp "$SOURCE/build-fingerprint.txt" "$CONTRACT/build-fingerprint.txt"
cp "$SOURCE/active-slot.txt" "$CONTRACT/active-slot.txt"
cp "$SOURCE/super-layout.txt" "$CONTRACT/super-layout.txt"
printf '# Fixture rollback procedure\n\nNo device-write commands.\n' > "$PROCEDURE"

REAL_CP="$(command -v cp)"
REAL_SYNC="$(command -v sync)"
export REAL_CP REAL_SYNC
export PATH="$ROLLBACK_TOOLS:$FIXTURE_BIN:$PATH"
export FAKE_TOOL_LOG="$TMP/tool.log"
: > "$FAKE_TOOL_LOG"
chmod +x "$FIXTURE_BIN/adb" "$FIXTURE_BIN/fastboot" "$ROLLBACK_TOOLS/cp" "$ROLLBACK_TOOLS/sync"

run_stage() {
  "$STAGE" "$1" "$2" "$CONTRACT" "$PROCEDURE"
}

source_before="$TMP/source-before.sha256"
source_after="$TMP/source-after.sha256"
find "$SOURCE" -type f -print0 | sort -z | xargs -0 sha256sum > "$source_before"
run_stage "$SOURCE" "$TMP/output"
find "$SOURCE" -type f -print0 | sort -z | xargs -0 sha256sum > "$source_after"
cmp "$source_before" "$source_after"

expected_inventory="$TMP/expected-inventory.txt"
printf '%s\n' \
  README.md SHA256SUMS.local active-slot.txt build-fingerprint.txt build-id.txt \
  super-layout.txt "${images[@]}" | sort > "$expected_inventory"
find "$TMP/output" -maxdepth 1 -type f -printf '%f\n' | sort > "$TMP/output-inventory.txt"
diff -u "$expected_inventory" "$TMP/output-inventory.txt"
[[ -z "$(find "$TMP/output" -type l -print -quit)" ]]
(
  cd "$TMP/output"
  sha256sum --check SHA256SUMS.local
)
cmp "$PROCEDURE" "$TMP/output/README.md"
cmp "$SOURCE/build-id.txt" "$TMP/output/build-id.txt"
cmp "$SOURCE/build-fingerprint.txt" "$TMP/output/build-fingerprint.txt"
cmp "$SOURCE/active-slot.txt" "$TMP/output/active-slot.txt"
cmp "$SOURCE/super-layout.txt" "$TMP/output/super-layout.txt"
[[ "$(stat -c %a "$TMP/output")" == 700 ]]
if grep -E '(^|[[:space:]])(adb|fastboot)([[:space:]]|$)' "$FAKE_TOOL_LOG" >/dev/null; then
  echo 'rollback staging invoked a device tool' >&2
  exit 1
fi

cp -a "$SOURCE" "$TMP/corrupt-source"
printf 'corrupt\n' >> "$TMP/corrupt-source/boot_a.img"
if run_stage "$TMP/corrupt-source" "$TMP/corrupt-output" >"$TMP/corrupt.stdout" 2>"$TMP/corrupt.stderr"; then
  echo 'rollback staging accepted a source checksum mismatch' >&2
  exit 1
fi
[[ ! -e "$TMP/corrupt-output" ]]
grep -F 'source checksum verification failed' "$TMP/corrupt.stderr" >/dev/null

cp -a "$SOURCE" "$TMP/wrong-manifest-source"
printf 'changed manifest\n' >> "$TMP/wrong-manifest-source/SHA256SUMS.local"
if run_stage "$TMP/wrong-manifest-source" "$TMP/wrong-manifest-output" >"$TMP/wrong-manifest.stdout" 2>"$TMP/wrong-manifest.stderr"; then
  echo 'rollback staging accepted a changed local checksum manifest' >&2
  exit 1
fi
[[ ! -e "$TMP/wrong-manifest-output" ]]
grep -F 'source local checksum manifest does not match the build contract' "$TMP/wrong-manifest.stderr" >/dev/null

BAD_CONTRACT="$TMP/bad-contract"
cp -a "$CONTRACT" "$BAD_CONTRACT"
head -n 1 "$CONTRACT/SHA256SUMS" >> "$BAD_CONTRACT/SHA256SUMS"
if "$STAGE" "$SOURCE" "$TMP/bad-contract-output" "$BAD_CONTRACT" "$PROCEDURE" >"$TMP/bad-contract.stdout" 2>"$TMP/bad-contract.stderr"; then
  echo 'rollback staging accepted a duplicate contract image entry' >&2
  exit 1
fi
[[ ! -e "$TMP/bad-contract-output" ]]
grep -F 'source checksum contract has an invalid image entry' "$TMP/bad-contract.stderr" >/dev/null

SHORT_CONTRACT="$TMP/short-contract"
cp -a "$CONTRACT" "$SHORT_CONTRACT"
head -n 14 "$CONTRACT/SHA256SUMS" > "$SHORT_CONTRACT/SHA256SUMS"
if "$STAGE" "$SOURCE" "$TMP/short-contract-output" "$SHORT_CONTRACT" "$PROCEDURE" >"$TMP/short-contract.stdout" 2>"$TMP/short-contract.stderr"; then
  echo 'rollback staging accepted a 14-image contract' >&2
  exit 1
fi
[[ ! -e "$TMP/short-contract-output" ]]
grep -F 'source checksum contract must name exactly 15 Android images' "$TMP/short-contract.stderr" >/dev/null

cp -a "$SOURCE" "$TMP/wrong-build-source"
printf 'wrong-build\n' > "$TMP/wrong-build-source/build-id.txt"
if run_stage "$TMP/wrong-build-source" "$TMP/wrong-build-output" >"$TMP/wrong-build.stdout" 2>"$TMP/wrong-build.stderr"; then
  echo 'rollback staging accepted the wrong build identity' >&2
  exit 1
fi
[[ ! -e "$TMP/wrong-build-output" ]]
grep -F 'source build identity does not match the build contract' "$TMP/wrong-build.stderr" >/dev/null

cp -a "$SOURCE" "$TMP/wrong-slot-source"
printf '_b\n' > "$TMP/wrong-slot-source/active-slot.txt"
if run_stage "$TMP/wrong-slot-source" "$TMP/wrong-slot-output" >"$TMP/wrong-slot.stdout" 2>"$TMP/wrong-slot.stderr"; then
  echo 'rollback staging accepted the wrong captured active slot' >&2
  exit 1
fi
[[ ! -e "$TMP/wrong-slot-output" ]]
grep -F 'source active slot does not match the build contract' "$TMP/wrong-slot.stderr" >/dev/null

cp -a "$SOURCE" "$TMP/wrong-layout-source"
printf 'wrong layout\n' > "$TMP/wrong-layout-source/super-layout.txt"
if run_stage "$TMP/wrong-layout-source" "$TMP/wrong-layout-output" >"$TMP/wrong-layout.stdout" 2>"$TMP/wrong-layout.stderr"; then
  echo 'rollback staging accepted the wrong super layout' >&2
  exit 1
fi
[[ ! -e "$TMP/wrong-layout-output" ]]
grep -F 'source super layout does not match the build contract' "$TMP/wrong-layout.stderr" >/dev/null

cp -a "$SOURCE" "$TMP/symlink-source"
rm "$TMP/symlink-source/boot_b.img"
ln -s "$SOURCE/boot_b.img" "$TMP/symlink-source/boot_b.img"
if run_stage "$TMP/symlink-source" "$TMP/symlink-output" >"$TMP/symlink.stdout" 2>"$TMP/symlink.stderr"; then
  echo 'rollback staging accepted a symbolic source image' >&2
  exit 1
fi
[[ ! -e "$TMP/symlink-output" ]]
grep -F 'source file is missing, not regular, or symbolic: boot_b.img' "$TMP/symlink.stderr" >/dev/null

PUBLIC_FIXTURE="$TMP/public-entrypoint"
mkdir -p "$PUBLIC_FIXTURE/contract"
cp "$HERE/rollback-bundle.sh" "$HERE/rollback-bundle-core.sh" "$PUBLIC_FIXTURE/"
cp "$CONTRACT"/* "$PUBLIC_FIXTURE/contract/"
cp "$PROCEDURE" "$PUBLIC_FIXTURE/ROLLBACK.md"
chmod +x "$PUBLIC_FIXTURE/rollback-bundle.sh" "$PUBLIC_FIXTURE/rollback-bundle-core.sh"
"$PUBLIC_FIXTURE/rollback-bundle.sh" "$SOURCE" "$TMP/public-output"
[[ -f "$TMP/public-output/super.img" ]]
cmp "$PROCEDURE" "$TMP/public-output/README.md"

mkdir "$TMP/existing-output"
if run_stage "$SOURCE" "$TMP/existing-output" >"$TMP/existing.stdout" 2>"$TMP/existing.stderr"; then
  echo 'rollback staging overwrote an existing output directory' >&2
  exit 1
fi
grep -F 'output already exists' "$TMP/existing.stderr" >/dev/null

if run_stage "$SOURCE" "$SOURCE/generated-output" >"$TMP/inside.stdout" 2>"$TMP/inside.stderr"; then
  echo 'rollback staging allowed output inside the immutable source directory' >&2
  exit 1
fi
[[ ! -e "$SOURCE/generated-output" ]]
grep -F 'output must be outside the source directory' "$TMP/inside.stderr" >/dev/null

mkdir "$TMP/unsafe-parent"
chmod 0777 "$TMP/unsafe-parent"
if run_stage "$SOURCE" "$TMP/unsafe-parent/output" >"$TMP/unsafe-parent.stdout" 2>"$TMP/unsafe-parent.stderr"; then
  echo 'rollback staging accepted a group/world-writable output parent' >&2
  exit 1
fi
chmod 0700 "$TMP/unsafe-parent"
[[ ! -e "$TMP/unsafe-parent/output" ]]
grep -F 'output parent must be owned by the current user and not group/world-writable' "$TMP/unsafe-parent.stderr" >/dev/null

cp -a "$SOURCE" "$TMP/changing-image-source"
export FAKE_TRIGGER_COPY_SOURCE="$TMP/changing-image-source/boot_a.img"
export FAKE_MUTATE_SOURCE="$TMP/changing-image-source/boot_a.img"
if run_stage "$TMP/changing-image-source" "$TMP/changing-image-output" >"$TMP/changing-image.stdout" 2>"$TMP/changing-image.stderr"; then
  echo 'rollback staging accepted source image mutation during copy' >&2
  exit 1
fi
unset FAKE_TRIGGER_COPY_SOURCE FAKE_MUTATE_SOURCE
[[ ! -e "$TMP/changing-image-output" ]]
grep -F 'source changed during rollback staging' "$TMP/changing-image.stderr" >/dev/null

cp -a "$SOURCE" "$TMP/changing-metadata-source"
export FAKE_TRIGGER_COPY_SOURCE="$TMP/changing-metadata-source/build-id.txt"
export FAKE_MUTATE_SOURCE="$TMP/changing-metadata-source/build-id.txt"
if run_stage "$TMP/changing-metadata-source" "$TMP/changing-metadata-output" >"$TMP/changing-metadata.stdout" 2>"$TMP/changing-metadata.stderr"; then
  echo 'rollback staging accepted source metadata mutation during copy' >&2
  exit 1
fi
unset FAKE_TRIGGER_COPY_SOURCE FAKE_MUTATE_SOURCE
[[ ! -e "$TMP/changing-metadata-output" ]]
grep -F 'source metadata changed during rollback staging' "$TMP/changing-metadata.stderr" >/dev/null

export FAKE_PREPUBLISH_SYNC_FAIL=1
if run_stage "$SOURCE" "$TMP/pre-sync-output" >"$TMP/pre-sync.stdout" 2>"$TMP/pre-sync.stderr"; then
  echo 'rollback staging accepted a failed pre-publish durability sync' >&2
  exit 1
fi
unset FAKE_PREPUBLISH_SYNC_FAIL
[[ ! -e "$TMP/pre-sync-output" ]]
grep -F 'fixture pre-publish sync failure' "$TMP/pre-sync.stderr" >/dev/null

export FAKE_FINAL_SYNC_FAIL=1
export FAKE_FINAL_SYNC_PARENT="$TMP"
if run_stage "$SOURCE" "$TMP/final-sync-output" >"$TMP/final-sync.stdout" 2>"$TMP/final-sync.stderr"; then
  echo 'rollback staging accepted a failed final durability sync' >&2
  exit 1
fi
unset FAKE_FINAL_SYNC_FAIL FAKE_FINAL_SYNC_PARENT
[[ ! -e "$TMP/final-sync-output" ]]
grep -F 'fixture final sync failure' "$TMP/final-sync.stderr" >/dev/null

shopt -s nullglob
staging_leftovers=("$TMP"/.odin2portal-rollback-bundle.*)
[[ "${#staging_leftovers[@]}" -eq 0 ]]

printf 'odin2portal rollback bundle tests passed\n'
