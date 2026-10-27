#!/usr/bin/env bash
set -Eeuo pipefail

SOURCE="${1:?usage: repack-core.sh <stock-source-directory> <output-directory> <contract-directory>}"
OUTPUT="${2:?usage: repack-core.sh <stock-source-directory> <output-directory> <contract-directory>}"
CONTRACT="${3:?usage: repack-core.sh <stock-source-directory> <output-directory> <contract-directory>}"

SOURCE="$(cd "$SOURCE" && pwd -P)"
OUTPUT_PARENT="$(cd "$(dirname "$OUTPUT")" && pwd -P)"
OUTPUT="$OUTPUT_PARENT/$(basename "$OUTPUT")"

case "$OUTPUT/" in
  "$SOURCE/"*)
    echo 'output must be outside the source directory' >&2
    exit 1
    ;;
esac

if [[ -e "$OUTPUT" ]]; then
  echo "output already exists: $OUTPUT" >&2
  exit 1
fi

required_contract_files=(
  SHA256SUMS
  logical-SHA256SUMS
  build-id.txt
  build-fingerprint.txt
  super-layout.txt
)
for file in "${required_contract_files[@]}"; do
  if [[ ! -f "$CONTRACT/$file" ]]; then
    echo "source contract is incomplete: missing $file" >&2
    exit 1
  fi
done

STAGING="$(mktemp -d "$OUTPUT_PARENT/.odin2portal-stock-repack.XXXXXX")"
cleanup() {
  [[ -z "${STAGING:-}" ]] || rm -rf "$STAGING"
}
trap cleanup EXIT
mkdir -p "$STAGING/evidence" "$STAGING/logical/source" "$STAGING/logical/rebuilt"

if ! (
  cd "$SOURCE"
  sha256sum --check "$CONTRACT/SHA256SUMS"
) > "$STAGING/evidence/source-sha256.txt" 2>&1; then
  echo 'source checksum verification failed' >&2
  exit 1
fi

if ! cmp -s "$CONTRACT/build-id.txt" "$SOURCE/build-id.txt" ||
   ! cmp -s "$CONTRACT/build-fingerprint.txt" "$SOURCE/build-fingerprint.txt"; then
  echo 'source build identity does not match the build contract' >&2
  exit 1
fi
cp "$SOURCE/build-id.txt" "$STAGING/evidence/build-id.txt"
cp "$SOURCE/build-fingerprint.txt" "$STAGING/evidence/build-fingerprint.txt"

lpdump "$SOURCE/super.img" > "$STAGING/evidence/source-lpdump.txt"
if ! diff -u "$CONTRACT/super-layout.txt" "$STAGING/evidence/source-lpdump.txt" \
  > "$STAGING/evidence/source-layout.diff"; then
  echo 'source super layout does not match the build contract' >&2
  exit 1
fi

lpunpack "$SOURCE/super.img" "$STAGING/logical/source"

logical_partitions=(odm product system system_dlkm system_ext vendor vendor_dlkm)
verify_logical_set() {
  local directory="$1"
  local label="$2"
  local inventory="$STAGING/evidence/$label-files.txt"
  local expected_inventory="$STAGING/evidence/$label-expected-files.txt"
  : > "$expected_inventory"
  for partition in "${logical_partitions[@]}"; do
    printf '%s_a.img\n%s_b.img\n' "$partition" "$partition" >> "$expected_inventory"
    if [[ ! -s "$directory/${partition}_a.img" ]]; then
      echo "$label logical partition is absent or empty: ${partition}_a" >&2
      return 1
    fi
    if [[ ! -f "$directory/${partition}_b.img" || -s "$directory/${partition}_b.img" ]]; then
      echo "$label slot-B placeholder is not zero-length: ${partition}_b" >&2
      return 1
    fi
  done
  find "$directory" -maxdepth 1 -type f -printf '%f\n' | sort > "$inventory"
  sort -o "$expected_inventory" "$expected_inventory"
  if ! diff -u "$expected_inventory" "$inventory" > "$STAGING/evidence/$label-files.diff"; then
    echo "$label logical partition inventory differs from the build contract" >&2
    return 1
  fi
  if ! (
    cd "$directory"
    sha256sum --check "$CONTRACT/logical-SHA256SUMS"
  ) > "$STAGING/evidence/$label-logical-sha256.txt" 2>&1; then
    echo "$label logical partition checksum verification failed" >&2
    return 1
  fi
}

verify_logical_set "$STAGING/logical/source" source

: > "$STAGING/evidence/ext4-checks.txt"
: > "$STAGING/evidence/avb-checks.txt"
for partition in "${logical_partitions[@]}"; do
  image="$STAGING/logical/source/${partition}_a.img"
  printf '\n== %s ==\n' "${partition}_a.img" >> "$STAGING/evidence/ext4-checks.txt"
  if ! e2fsck -fn "$image" >> "$STAGING/evidence/ext4-checks.txt" 2>&1; then
    echo "filesystem verification failed: ${partition}_a.img" >&2
    tail -n 20 "$STAGING/evidence/ext4-checks.txt" >&2
    exit 1
  fi
  printf '\n== %s ==\n' "${partition}_a.img" >> "$STAGING/evidence/avb-checks.txt"
  if ! avbtool info_image --image "$image" >> "$STAGING/evidence/avb-checks.txt" 2>&1; then
    echo "AVB verification failed: ${partition}_a.img" >&2
    tail -n 20 "$STAGING/evidence/avb-checks.txt" >&2
    exit 1
  fi
done
for image in vbmeta_a.img vbmeta_system_a.img; do
  printf '\n== %s ==\n' "$image" >> "$STAGING/evidence/avb-checks.txt"
  if ! avbtool info_image --image "$SOURCE/$image" >> "$STAGING/evidence/avb-checks.txt" 2>&1; then
    echo "AVB verification failed: $image" >&2
    tail -n 20 "$STAGING/evidence/avb-checks.txt" >&2
    exit 1
  fi
done

super_size="$(stat -c %s "$SOURCE/super.img")"
# The captured layout reserves 4 MiB outside each slot's dynamic partition group.
group_size=$((super_size - 4194304))
lpmake_args=(
  --metadata-size 65536
  --metadata-slots 3
  --super-name super
  --device "super:$super_size:1048576:0"
  --group "qti_dynamic_partitions_a:$group_size"
  --group "qti_dynamic_partitions_b:$group_size"
  --virtual-ab
)
for partition in "${logical_partitions[@]}"; do
  partition_size="$(stat -c %s "$STAGING/logical/source/${partition}_a.img")"
  lpmake_args+=(
    --partition "${partition}_a:readonly:$partition_size:qti_dynamic_partitions_a"
    --image "${partition}_a=$STAGING/logical/source/${partition}_a.img"
    --partition "${partition}_b:readonly:0:qti_dynamic_partitions_b"
  )
done
lpmake_args+=(--output "$STAGING/super.img")
if ! lpmake "${lpmake_args[@]}" > "$STAGING/evidence/lpmake.txt" 2>&1; then
  echo 'super image reconstruction failed' >&2
  tail -n 20 "$STAGING/evidence/lpmake.txt" >&2
  exit 1
fi

if [[ "$(stat -c %s "$STAGING/super.img")" -ne "$super_size" ]]; then
  echo 'rebuilt super image size differs from source' >&2
  exit 1
fi

lpdump "$STAGING/super.img" > "$STAGING/evidence/rebuilt-lpdump.txt"
if ! diff -u "$CONTRACT/super-layout.txt" "$STAGING/evidence/rebuilt-lpdump.txt" \
  > "$STAGING/evidence/rebuilt-layout.diff"; then
  echo 'rebuilt super layout differs from the build contract' >&2
  exit 1
fi

rm -rf "$STAGING/logical/source"
lpunpack "$STAGING/super.img" "$STAGING/logical/rebuilt"
verify_logical_set "$STAGING/logical/rebuilt" rebuilt

(
  cd "$STAGING"
  sha256sum super.img > evidence/rebuilt-super-SHA256SUMS
)
cp "$CONTRACT/SHA256SUMS" "$STAGING/evidence/source-contract-SHA256SUMS"
cp "$CONTRACT/logical-SHA256SUMS" "$STAGING/evidence/logical-contract-SHA256SUMS"
if ! (
  cd "$SOURCE"
  sha256sum --check "$CONTRACT/SHA256SUMS"
) > "$STAGING/evidence/source-sha256-final.txt" 2>&1; then
  echo 'source changed after initial verification' >&2
  exit 1
fi
if ! cmp -s "$CONTRACT/build-id.txt" "$SOURCE/build-id.txt" ||
   ! cmp -s "$CONTRACT/build-fingerprint.txt" "$SOURCE/build-fingerprint.txt"; then
  echo 'source build identity changed after initial verification' >&2
  exit 1
fi
printf '%s\n' \
  'READ_ONLY_NOOP_REPACK_VERIFIED' \
  'source images: verified' \
  'source filesystems and AVB: verified' \
  'logical partitions: byte-identical' \
  'super layout: identical' \
  'device writes: none' > "$STAGING/RESULT.txt"

rm -rf "$STAGING/logical"
mv -Tn "$STAGING" "$OUTPUT"
if [[ -e "$STAGING" ]]; then
  echo 'output appeared during reconstruction; refusing to publish' >&2
  exit 1
fi
STAGING=""
printf 'READ_ONLY_NOOP_REPACK_VERIFIED output=%s\n' "$OUTPUT"
