#!/usr/bin/env bash
set -Eeuo pipefail
report_error() {
  local status=$?
  printf 'launcher product dry run failed at line %s: %s\n' "$1" "$2" >&2
  exit "$status"
}
trap 'report_error "$LINENO" "$BASH_COMMAND"' ERR

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE="${1:?usage: launcher-product-dry-run.sh <stock-source> <signed-launcher-apk> <output>}"
APK="${2:?usage: launcher-product-dry-run.sh <stock-source> <signed-launcher-apk> <output>}"
OUTPUT="${3:?usage: launcher-product-dry-run.sh <stock-source> <signed-launcher-apk> <output>}"
CONTRACT="$HERE/contract"
MARKER="$HERE/marker-build-prop.txt"
NOTICE="$HERE/LAUNCHER-NOT-FLASHABLE.md"

SOURCE="$(cd "$SOURCE" && pwd -P)"
APK_PARENT="$(cd "$(dirname "$APK")" && pwd -P)"
APK="$APK_PARENT/$(basename "$APK")"
OUTPUT_PARENT="$(cd "$(dirname "$OUTPUT")" && pwd -P)"
OUTPUT="$OUTPUT_PARENT/$(basename "$OUTPUT")"
if [[ "$OUTPUT_PARENT" =~ [[:space:]] ]]; then
  echo 'output parent must not contain whitespace because debugfs cannot safely parse host paths with spaces' >&2
  exit 1
fi
case "$OUTPUT/" in
  "$SOURCE/"*)
    echo 'output must be outside the source directory' >&2
    exit 1
    ;;
esac
if [[ -e "$OUTPUT" || -L "$OUTPUT" ]]; then
  echo "output already exists: $OUTPUT" >&2
  exit 1
fi
required_free_bytes=$((28 * 1024 * 1024 * 1024))
available_bytes="$(df -PB1 "$OUTPUT_PARENT" | awk 'NR == 2 {print $4}')"
if [[ ! "$available_bytes" =~ ^[0-9]+$ || "$available_bytes" -lt "$required_free_bytes" ]]; then
  echo 'launcher product dry run requires at least 28 GiB free in the output filesystem' >&2
  exit 1
fi

required_contract_files=(
  SHA256SUMS active-slot.txt build-fingerprint.txt build-id.txt
  logical-SHA256SUMS super-layout.txt
)
for file in "${required_contract_files[@]}"; do
  [[ -f "$CONTRACT/$file" && ! -L "$CONTRACT/$file" ]] || {
    echo "source contract is incomplete: missing regular $file" >&2
    exit 1
  }
done
for file in "$MARKER" "$NOTICE" "$APK"; do
  [[ -f "$file" && ! -L "$file" ]] || {
    echo "launcher product input is missing, not regular, or symbolic: $file" >&2
    exit 1
  }
done
expected_marker_sha256=db82ee5bc6d88d479785a18698a7a4bdde1a26f5dfd998e4d9a5e786299bba08
actual_marker_sha256="$(sha256sum "$MARKER" | awk '{print $1}')"
if [[ "$actual_marker_sha256" != "$expected_marker_sha256" ]]; then
  echo 'marker does not match the exact marker-only slice contract' >&2
  exit 1
fi

if ! (
  cd "$SOURCE"
  sha256sum --check "$CONTRACT/SHA256SUMS"
) >/dev/null 2>&1; then
  echo 'source checksum verification failed' >&2
  exit 1
fi
if ! cmp -s "$CONTRACT/build-id.txt" "$SOURCE/build-id.txt" ||
   ! cmp -s "$CONTRACT/build-fingerprint.txt" "$SOURCE/build-fingerprint.txt"; then
  echo 'source build identity does not match the build contract' >&2
  exit 1
fi
if ! cmp -s "$CONTRACT/active-slot.txt" "$SOURCE/active-slot.txt" ||
   ! cmp -s "$CONTRACT/super-layout.txt" "$SOURCE/super-layout.txt"; then
  echo 'source slot or super layout does not match the build contract' >&2
  exit 1
fi

umask 077
STAGING="$(mktemp -d "$OUTPUT_PARENT/.odin2portal-launcher-product.XXXXXX")"
PUBLISHED_OUTPUT=""
cleanup() {
  [[ -z "${STAGING:-}" ]] || rm -rf "$STAGING"
  [[ -z "${PUBLISHED_OUTPUT:-}" ]] || rm -rf --one-file-system -- "$PUBLISHED_OUTPUT"
}
trap cleanup EXIT
mkdir -p "$STAGING/evidence" "$STAGING/logical/source" "$STAGING/logical/rebuilt" \
  "$STAGING/avb-verify" "$STAGING/verified-source"
cp "$APK" "$STAGING/verified-source/Korri.apk"
STAGED_APK="$STAGING/verified-source/Korri.apk"
"$HERE/verify-korri-launcher-apk.sh" "$STAGED_APK" "$STAGING/evidence/apk" \
  > "$STAGING/evidence/apk-verifier.stdout.txt"
apk_sha256="$(sha256sum "$STAGED_APK" | awk '{print $1}')"
# Copy every source artifact used to derive output, then verify the copies. This
# closes the interval between the initial source gate and later tool reads.
cp --reflink=auto --sparse=always "$SOURCE/super.img" "$STAGING/verified-source/super.img"
cp "$SOURCE/vbmeta_system_a.img" "$STAGING/verified-source/vbmeta_system_a.img"
grep -E '  (super|vbmeta_system_a)\.img$' "$CONTRACT/SHA256SUMS" \
  > "$STAGING/evidence/staged-source-SHA256SUMS"
(
  cd "$STAGING/verified-source"
  sha256sum --check "$STAGING/evidence/staged-source-SHA256SUMS"
) > "$STAGING/evidence/staged-source-sha256.txt"
verified_super="$STAGING/verified-source/super.img"
verified_vbmeta_system="$STAGING/verified-source/vbmeta_system_a.img"

lpdump "$verified_super" > "$STAGING/evidence/source-lpdump.txt"
diff -u "$CONTRACT/super-layout.txt" "$STAGING/evidence/source-lpdump.txt" \
  > "$STAGING/evidence/source-layout.diff"
lpunpack "$verified_super" "$STAGING/logical/source"

logical_partitions=(odm product system system_dlkm system_ext vendor vendor_dlkm)
for partition in "${logical_partitions[@]}"; do
  [[ -s "$STAGING/logical/source/${partition}_a.img" ]]
  [[ -f "$STAGING/logical/source/${partition}_b.img" ]]
  [[ ! -s "$STAGING/logical/source/${partition}_b.img" ]]
done
(
  cd "$STAGING/logical/source"
  sha256sum --check "$CONTRACT/logical-SHA256SUMS"
) > "$STAGING/evidence/source-logical-sha256.txt"

product="$STAGING/logical/source/product_a.img"
custom_product="$STAGING/product_a.img"
cp --reflink=auto --sparse=always "$product" "$custom_product"
debugfs -R 'cat /etc/build.prop' "$product" \
  > "$STAGING/evidence/build.prop.before" 2>/dev/null
avbtool info_image --image "$product" > "$STAGING/evidence/product-avb.before.txt"

# AVB values are extracted from captured producers. Growth includes the exact
# APK size plus 32 MiB for ext4 metadata, unsharing, and AVB hashtree overhead.
stock_product_size="$(stat -c %s "$product")"
apk_size="$(stat -c %s "$STAGED_APK")"
mebibyte=$((1024 * 1024))
product_growth_bytes=$((((apk_size + 32 * mebibyte + mebibyte - 1) / mebibyte) * mebibyte))
product_partition_size=$((stock_product_size + product_growth_bytes))
product_salt="$(awk '/Salt:/ {print $2; exit}' "$STAGING/evidence/product-avb.before.txt")"
product_os_version="$(awk -F"'" '/Prop: com.android.build.product.os_version/ {print $2; exit}' "$STAGING/evidence/product-avb.before.txt")"
product_fingerprint="$(awk -F"'" '/Prop: com.android.build.product.fingerprint/ {print $2; exit}' "$STAGING/evidence/product-avb.before.txt")"
product_security_patch="$(awk -F"'" '/Prop: com.android.build.product.security_patch/ {print $2; exit}' "$STAGING/evidence/product-avb.before.txt")"
product_security_patch_count="$(grep -Fc 'Prop: com.android.build.product.security_patch' "$STAGING/evidence/product-avb.before.txt")"
[[ "$product_salt" =~ ^[0-9a-f]{64}$ ]]
[[ -n "$product_os_version" && -n "$product_fingerprint" && -n "$product_security_patch" ]]
# The stock descriptor intentionally contains the same security-patch property twice.
[[ "$product_security_patch_count" -eq 2 ]]
product_filesystem_size="$(avbtool add_hashtree_footer \
  --partition_size "$product_partition_size" \
  --partition_name product \
  --hash_algorithm sha256 \
  --salt "$product_salt" \
  --block_size 4096 \
  --do_not_generate_fec \
  --algorithm NONE \
  --calc_max_image_size)"
[[ "$product_filesystem_size" =~ ^[0-9]+$ ]]

avbtool erase_footer --image "$custom_product"
truncate -s "$product_filesystem_size" "$custom_product"
if ! resize2fs "$custom_product" $((product_filesystem_size / 4096)) \
  > "$STAGING/evidence/product-resize.txt" 2>&1; then
  echo 'product filesystem resize failed' >&2
  tail -n 20 "$STAGING/evidence/product-resize.txt" >&2
  exit 1
fi
if ! e2fsck -fy -E unshare_blocks "$custom_product" \
  > "$STAGING/evidence/product-unshare.txt" 2>&1; then
  echo 'product shared-block unshare failed' >&2
  tail -n 20 "$STAGING/evidence/product-unshare.txt" >&2
  exit 1
fi
"$HERE/append-build-prop-marker.sh" \
  "$custom_product" /etc/build.prop "$MARKER" \
  > "$STAGING/evidence/marker-append.txt"
if debugfs -R 'ls -l /app' "$custom_product" 2>/dev/null | awk '{print $NF}' | grep -Fx Korri >/dev/null; then
  echo 'stock product already contains /app/Korri' >&2
  exit 1
fi
selinux_xattr="$STAGING/system-file.selinux-xattr"
printf 'u:object_r:system_file:s0\0' > "$selinux_xattr"
{
  debugfs -w -R 'mkdir /app/Korri' "$custom_product"
  debugfs -w -R "write $STAGED_APK /app/Korri/Korri.apk" "$custom_product"
  debugfs -w -R 'set_inode_field /app/Korri mode 040755' "$custom_product"
  debugfs -w -R 'set_inode_field /app/Korri/Korri.apk mode 0100644' "$custom_product"
  debugfs -w -R "ea_set -f $selinux_xattr /app/Korri security.selinux" "$custom_product"
  debugfs -w -R "ea_set -f $selinux_xattr /app/Korri/Korri.apk security.selinux" "$custom_product"
} > "$STAGING/evidence/apk-install.txt" 2>&1
rm "$selinux_xattr"
debugfs -R 'stat /app/Korri' "$custom_product" > "$STAGING/evidence/korri-directory-stat.txt" 2>&1
debugfs -R 'stat /app/Korri/Korri.apk' "$custom_product" > "$STAGING/evidence/korri-apk-stat.txt" 2>&1
debugfs -R 'ea_list /app/Korri' "$custom_product" > "$STAGING/evidence/korri-directory-xattr.txt" 2>&1
debugfs -R 'ea_list /app/Korri/Korri.apk' "$custom_product" > "$STAGING/evidence/korri-apk-xattr.txt" 2>&1
grep -F 'Mode:  0755' "$STAGING/evidence/korri-directory-stat.txt" >/dev/null
grep -F 'User:     0   Group:     0' "$STAGING/evidence/korri-directory-stat.txt" >/dev/null
grep -F 'Mode:  0644' "$STAGING/evidence/korri-apk-stat.txt" >/dev/null
grep -F 'User:     0   Group:     0' "$STAGING/evidence/korri-apk-stat.txt" >/dev/null
grep -F 'u:object_r:system_file:s0' "$STAGING/evidence/korri-directory-xattr.txt" >/dev/null
grep -F 'u:object_r:system_file:s0' "$STAGING/evidence/korri-apk-xattr.txt" >/dev/null
debugfs -R "dump -p /app/Korri/Korri.apk $STAGING/evidence/Korri.extracted.apk" "$custom_product" >/dev/null 2>&1
cmp "$STAGED_APK" "$STAGING/evidence/Korri.extracted.apk"
[[ "$(sha256sum "$STAGING/evidence/Korri.extracted.apk" | awk '{print $1}')" == "$apk_sha256" ]]

e2fsck -fn "$custom_product" > "$STAGING/evidence/product-ext4.txt" 2>&1
debugfs -R 'cat /etc/build.prop' "$custom_product" \
  > "$STAGING/evidence/build.prop.after" 2>/dev/null
cat "$STAGING/evidence/build.prop.before" "$MARKER" \
  > "$STAGING/evidence/build.prop.expected"
cmp "$STAGING/evidence/build.prop.expected" "$STAGING/evidence/build.prop.after"

avbtool add_hashtree_footer \
  --image "$custom_product" \
  --partition_size "$product_partition_size" \
  --partition_name product \
  --hash_algorithm sha256 \
  --salt "$product_salt" \
  --block_size 4096 \
  --do_not_generate_fec \
  --algorithm NONE \
  --prop "com.android.build.product.os_version:$product_os_version" \
  --prop "com.android.build.product.fingerprint:$product_fingerprint" \
  --prop "com.android.build.product.security_patch:$product_security_patch" \
  --prop "com.android.build.product.security_patch:$product_security_patch" \
  --output_vbmeta_image "$STAGING/product-descriptor.img"
[[ "$(stat -c %s "$custom_product")" -eq "$product_partition_size" ]]
avbtool info_image --image "$custom_product" > "$STAGING/evidence/product-avb.after.txt"
ln -s "$custom_product" "$STAGING/avb-verify/product.img"
cp "$STAGING/product-descriptor.img" "$STAGING/avb-verify/product-vbmeta.img"
avbtool verify_image --image "$STAGING/avb-verify/product-vbmeta.img" \
  > "$STAGING/evidence/product-avb-verify.txt"

custom_vbmeta_system="$STAGING/vbmeta_system_a.img"
avbtool info_image --image "$verified_vbmeta_system" \
  > "$STAGING/evidence/vbmeta-system.before.txt"
vbmeta_system_size="$(stat -c %s "$verified_vbmeta_system")"
vbmeta_system_rollback_index="$(awk '/Rollback Index:/ {print $3; exit}' "$STAGING/evidence/vbmeta-system.before.txt")"
[[ "$vbmeta_system_rollback_index" =~ ^[0-9]+$ ]]
avbtool make_vbmeta_image \
  --output "$custom_vbmeta_system" \
  --padding_size "$vbmeta_system_size" \
  --algorithm NONE \
  --rollback_index "$vbmeta_system_rollback_index" \
  --include_descriptors_from_image "$STAGING/logical/source/system_a.img" \
  --include_descriptors_from_image "$custom_product" \
  --include_descriptors_from_image "$STAGING/logical/source/system_ext_a.img"
[[ "$(stat -c %s "$custom_vbmeta_system")" -eq "$vbmeta_system_size" ]]
avbtool info_image --image "$custom_vbmeta_system" \
  > "$STAGING/evidence/vbmeta-system.after.txt"
ln -s "$STAGING/logical/source/system_a.img" "$STAGING/avb-verify/system.img"
ln -s "$STAGING/logical/source/system_ext_a.img" "$STAGING/avb-verify/system_ext.img"
cp "$custom_vbmeta_system" "$STAGING/avb-verify/vbmeta_system.img"
avbtool verify_image --image "$STAGING/avb-verify/vbmeta_system.img" \
  > "$STAGING/evidence/vbmeta-system-verify.txt"

grep -F 'Algorithm:                NONE' "$STAGING/evidence/vbmeta-system.after.txt" >/dev/null
grep -F 'FEC num roots:         0' "$STAGING/evidence/product-avb.after.txt" >/dev/null

super_size="$(stat -c %s "$verified_super")"
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
  image="$STAGING/logical/source/${partition}_a.img"
  [[ "$partition" == product ]] && image="$custom_product"
  partition_size="$(stat -c %s "$image")"
  lpmake_args+=(
    --partition "${partition}_a:readonly:$partition_size:qti_dynamic_partitions_a"
    --image "${partition}_a=$image"
    --partition "${partition}_b:readonly:0:qti_dynamic_partitions_b"
  )
done
lpmake_args+=(--output "$STAGING/super.img")
if ! lpmake "${lpmake_args[@]}" > "$STAGING/evidence/lpmake.txt" 2>&1; then
  echo 'launcher super image reconstruction failed' >&2
  tail -n 20 "$STAGING/evidence/lpmake.txt" >&2
  exit 1
fi
[[ "$(stat -c %s "$STAGING/super.img")" -eq "$super_size" ]]
lpdump "$STAGING/super.img" > "$STAGING/evidence/rebuilt-lpdump.txt"
lpunpack "$STAGING/super.img" "$STAGING/logical/rebuilt"

for partition in "${logical_partitions[@]}"; do
  rebuilt="$STAGING/logical/rebuilt/${partition}_a.img"
  expected="$STAGING/logical/source/${partition}_a.img"
  [[ "$partition" == product ]] && expected="$custom_product"
  cmp "$expected" "$rebuilt"
  [[ -f "$STAGING/logical/rebuilt/${partition}_b.img" ]]
  [[ ! -s "$STAGING/logical/rebuilt/${partition}_b.img" ]]
done

artifact_dir="$STAGING/NON_FLASHABLE_ARTIFACTS"
mkdir "$artifact_dir"
mv "$STAGING/super.img" "$artifact_dir/super.img.not-flashable"
mv "$custom_product" "$artifact_dir/product_a.img.not-flashable"
mv "$custom_vbmeta_system" "$artifact_dir/vbmeta_system_a.img.not-flashable"
(
  cd "$STAGING"
  sha256sum \
    NON_FLASHABLE_ARTIFACTS/super.img.not-flashable \
    NON_FLASHABLE_ARTIFACTS/product_a.img.not-flashable \
    NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable \
    > evidence/output-SHA256SUMS
)
cp "$NOTICE" "$STAGING/NOT-FLASHABLE.md"
printf '%s\n' \
  'ODIN2PORTAL_LAUNCHER_PRODUCT_DRY_RUN_VERIFIED' \
  'source build: verified' \
  'intended changes: one build.prop marker and one verified Korri APK' \
  'Korri APK path: /product/app/Korri/Korri.apk' \
  "Korri APK SHA-256: $apk_sha256" \
  "product partition growth: $product_growth_bytes bytes" \
  'product hashtree: regenerated unsigned without FEC' \
  'vbmeta_system: regenerated with Algorithm NONE' \
  'stock root vbmeta chain: unresolved' \
  'flash ready: no' \
  'device writes: none' > "$STAGING/RESULT.txt"

if ! (
  cd "$SOURCE"
  sha256sum --check "$CONTRACT/SHA256SUMS"
) > "$STAGING/evidence/source-sha256-final.txt" 2>&1; then
  echo 'source changed during marker dry run' >&2
  exit 1
fi
if ! cmp -s "$CONTRACT/build-id.txt" "$SOURCE/build-id.txt" ||
   ! cmp -s "$CONTRACT/build-fingerprint.txt" "$SOURCE/build-fingerprint.txt" ||
   ! cmp -s "$CONTRACT/active-slot.txt" "$SOURCE/active-slot.txt" ||
   ! cmp -s "$CONTRACT/super-layout.txt" "$SOURCE/super-layout.txt"; then
  echo 'source metadata changed during launcher product dry run' >&2
  exit 1
fi
[[ "$(sha256sum "$APK" | awk '{print $1}')" == "$apk_sha256" ]] || {
  echo 'Korri APK changed during launcher product dry run' >&2
  exit 1
}

rm -rf "$STAGING/logical" "$STAGING/avb-verify" "$STAGING/verified-source" \
  "$STAGING/product-descriptor.img"
sync -d "$STAGING/RESULT.txt" "$STAGING/NOT-FLASHABLE.md" \
  "$STAGING/evidence/output-SHA256SUMS" "$artifact_dir"/*
sync -f "$STAGING"
mv -Tn "$STAGING" "$OUTPUT"
if [[ -e "$STAGING" ]]; then
  echo 'output appeared during marker dry run; refusing to publish' >&2
  exit 1
fi
STAGING=""
PUBLISHED_OUTPUT="$OUTPUT"
[[ -f "$OUTPUT/NON_FLASHABLE_ARTIFACTS/super.img.not-flashable" ]]
[[ -f "$OUTPUT/NON_FLASHABLE_ARTIFACTS/product_a.img.not-flashable" ]]
[[ -f "$OUTPUT/NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable" ]]
grep -Fx 'ODIN2PORTAL_LAUNCHER_PRODUCT_DRY_RUN_VERIFIED' "$OUTPUT/RESULT.txt" >/dev/null
(
  cd "$OUTPUT"
  sha256sum --check evidence/output-SHA256SUMS
) >/dev/null
sync -f "$OUTPUT_PARENT"
PUBLISHED_OUTPUT=""
printf 'ODIN2PORTAL_LAUNCHER_PRODUCT_DRY_RUN_VERIFIED output=%s\n' "$OUTPUT"
