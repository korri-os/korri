#!/usr/bin/env bash
set -Eeuo pipefail
report_error() {
  local status=$?
  printf 'launcher image dry run failed at line %s: %s\n' "$1" "$2" >&2
  exit "$status"
}
trap 'report_error "$LINENO" "$BASH_COMMAND"' ERR

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE="${1:?usage: launcher-image-dry-run.sh <stock-source> <signed-launcher-apk> <private-key> <output>}"
APK="${2:?usage: launcher-image-dry-run.sh <stock-source> <signed-launcher-apk> <private-key> <output>}"
PRIVATE_KEY="${3:?usage: launcher-image-dry-run.sh <stock-source> <signed-launcher-apk> <private-key> <output>}"
OUTPUT="${4:?usage: launcher-image-dry-run.sh <stock-source> <signed-launcher-apk> <private-key> <output>}"
CONTRACT="$HERE/contract"
MARKER="$HERE/marker-build-prop.txt"
NOTICE="$HERE/LAUNCHER-NOT-FLASHABLE.md"
PROVISIONING="$HERE/HOME-PROVISIONING.md"

SOURCE="$(cd "$SOURCE" && pwd -P)"
APK_PARENT="$(cd "$(dirname "$APK")" && pwd -P)"
APK="$APK_PARENT/$(basename "$APK")"
PRIVATE_KEY_PARENT="$(cd "$(dirname "$PRIVATE_KEY")" && pwd -P)"
PRIVATE_KEY="$PRIVATE_KEY_PARENT/$(basename "$PRIVATE_KEY")"
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
required_free_bytes=$((32 * 1024 * 1024 * 1024))
available_bytes="$(df -PB1 "$OUTPUT_PARENT" | awk 'NR == 2 {print $4}')"
if [[ ! "$available_bytes" =~ ^[0-9]+$ || "$available_bytes" -lt "$required_free_bytes" ]]; then
  echo 'launcher image dry run requires at least 32 GiB free in the output filesystem' >&2
  exit 1
fi

required_contract_files=(
  SHA256SUMS active-slot.txt build-fingerprint.txt build-id.txt
  korri-avb-public-key-SHA256.txt korri-release-cert-SHA256.txt
  logical-SHA256SUMS super-layout.txt
)
for file in "${required_contract_files[@]}"; do
  [[ -f "$CONTRACT/$file" && ! -L "$CONTRACT/$file" ]] || {
    echo "source contract is incomplete: missing regular $file" >&2
    exit 1
  }
done
for file in "$MARKER" "$NOTICE" "$PROVISIONING" "$APK"; do
  [[ -f "$file" && ! -L "$file" ]] || {
    echo "launcher image input is missing, not regular, or symbolic: $file" >&2
    exit 1
  }
done
expected_marker_sha256=db82ee5bc6d88d479785a18698a7a4bdde1a26f5dfd998e4d9a5e786299bba08
[[ "$(sha256sum "$MARKER" | awk '{print $1}')" == "$expected_marker_sha256" ]] || {
  echo 'marker does not match the exact marker-only slice contract' >&2
  exit 1
}
if [[ ! -f "$PRIVATE_KEY" || -L "$PRIVATE_KEY" || "$(stat -c %a "$PRIVATE_KEY")" != 600 ]]; then
  echo 'private key must be a regular non-symbolic file with mode 0600' >&2
  exit 1
fi
openssl pkey -in "$PRIVATE_KEY" -check -noout >/dev/null
openssl pkey -in "$PRIVATE_KEY" -pubout -text_pub -noout | grep -F 'Public-Key: (4096 bit' >/dev/null || {
  echo 'private key must be RSA-4096' >&2
  exit 1
}
private_key_sha256="$(sha256sum "$PRIVATE_KEY" | awk '{print $1}')"
apk_sha256="$(sha256sum "$APK" | awk '{print $1}')"

if ! (
  cd "$SOURCE"
  sha256sum --check "$CONTRACT/SHA256SUMS"
) >/dev/null 2>&1; then
  echo 'source checksum verification failed' >&2
  exit 1
fi
if ! cmp -s "$CONTRACT/build-id.txt" "$SOURCE/build-id.txt" ||
   ! cmp -s "$CONTRACT/build-fingerprint.txt" "$SOURCE/build-fingerprint.txt" ||
   ! cmp -s "$CONTRACT/active-slot.txt" "$SOURCE/active-slot.txt" ||
   ! cmp -s "$CONTRACT/super-layout.txt" "$SOURCE/super-layout.txt"; then
  echo 'source identity, slot, or super layout does not match the contract' >&2
  exit 1
fi
umask 077
STAGING=""
SECURE_KEY_DIR=""
PUBLISHED_OUTPUT=""
cleanup() {
  [[ -z "${SECURE_KEY_DIR:-}" ]] || rm -rf "$SECURE_KEY_DIR"
  [[ -z "${STAGING:-}" ]] || rm -rf "$STAGING"
  [[ -z "${PUBLISHED_OUTPUT:-}" ]] || rm -rf --one-file-system -- "$PUBLISHED_OUTPUT"
}
trap cleanup EXIT
STAGING="$(mktemp -d "$OUTPUT_PARENT/.odin2portal-launcher-image.XXXXXX")"
SECURE_KEY_DIR="$(mktemp -d /dev/shm/.odin2portal-avb-key.XXXXXX)"
mkdir -p "$STAGING/evidence" "$STAGING/source" "$STAGING/logical" \
  "$STAGING/descriptor-inputs" "$STAGING/verify" "$STAGING/NON_FLASHABLE_ARTIFACTS"
cp "$PRIVATE_KEY" "$SECURE_KEY_DIR/signing.pem"
chmod 0600 "$SECURE_KEY_DIR/signing.pem"
[[ "$(sha256sum "$SECURE_KEY_DIR/signing.pem" | awk '{print $1}')" == "$private_key_sha256" ]]
SIGNING_KEY="$SECURE_KEY_DIR/signing.pem"
CUSTOM_PUBLIC_KEY="$STAGING/korri-odin2portal-avb.avbpubkey"
avbtool extract_public_key --key "$SIGNING_KEY" --output "$CUSTOM_PUBLIC_KEY"
(
  cd "$STAGING"
  sha256sum --check "$CONTRACT/korri-avb-public-key-SHA256.txt"
) > "$STAGING/evidence/korri-public-key-sha256.txt" || {
  echo 'private key does not match the selected Korri AVB key' >&2
  exit 1
}

LAUNCHER_OUTPUT="$STAGING/generated-launcher-output"
"$HERE/launcher-product-dry-run.sh" "$SOURCE" "$APK" "$LAUNCHER_OUTPUT" \
  > "$STAGING/evidence/launcher-generation.txt"
launcher_required=(
  RESULT.txt evidence/output-SHA256SUMS
  NON_FLASHABLE_ARTIFACTS/product_a.img.not-flashable
  NON_FLASHABLE_ARTIFACTS/super.img.not-flashable
  NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable
)
for file in "${launcher_required[@]}"; do
  [[ -f "$LAUNCHER_OUTPUT/$file" && ! -L "$LAUNCHER_OUTPUT/$file" ]] || {
    echo "generated launcher product output is incomplete or symbolic: $file" >&2
    exit 1
  }
done
grep -Fx 'ODIN2PORTAL_LAUNCHER_PRODUCT_DRY_RUN_VERIFIED' "$LAUNCHER_OUTPUT/RESULT.txt" >/dev/null
grep -Fx 'flash ready: no' "$LAUNCHER_OUTPUT/RESULT.txt" >/dev/null
grep -Fx 'device writes: none' "$LAUNCHER_OUTPUT/RESULT.txt" >/dev/null
(
  cd "$LAUNCHER_OUTPUT"
  sha256sum --check evidence/output-SHA256SUMS
) > "$STAGING/evidence/generated-launcher-sha256.txt"

standalone_inputs=(
  boot_a.img dtbo_a.img init_boot_a.img recovery_a.img super.img
  vbmeta_a.img vbmeta_system_a.img vendor_boot_a.img
)
for image in "${standalone_inputs[@]}"; do
  cp --reflink=auto --sparse=always "$SOURCE/$image" "$STAGING/source/$image"
done
grep -E '  (boot_a|dtbo_a|init_boot_a|recovery_a|super|vbmeta_a|vbmeta_system_a|vendor_boot_a)\.img$' \
  "$CONTRACT/SHA256SUMS" > "$STAGING/evidence/staged-source-SHA256SUMS"
(
  cd "$STAGING/source"
  sha256sum --check "$STAGING/evidence/staged-source-SHA256SUMS"
) > "$STAGING/evidence/staged-source-sha256.txt"
lpdump "$STAGING/source/super.img" > "$STAGING/evidence/source-lpdump.txt"
diff -u "$CONTRACT/super-layout.txt" "$STAGING/evidence/source-lpdump.txt" \
  > "$STAGING/evidence/source-layout.diff"
lpunpack "$STAGING/source/super.img" "$STAGING/logical"
(
  cd "$STAGING/logical"
  sha256sum --check "$CONTRACT/logical-SHA256SUMS"
) > "$STAGING/evidence/source-logical-sha256.txt"

CUSTOM_PRODUCT="$STAGING/product_a.img"
cp --reflink=auto --sparse=always \
  "$LAUNCHER_OUTPUT/NON_FLASHABLE_ARTIFACTS/product_a.img.not-flashable" "$CUSTOM_PRODUCT"
e2fsck -fn "$CUSTOM_PRODUCT" > "$STAGING/evidence/product-ext4.txt" 2>&1
debugfs -R 'cat /etc/build.prop' "$STAGING/logical/product_a.img" \
  > "$STAGING/evidence/build.prop.stock" 2>/dev/null
debugfs -R 'cat /etc/build.prop' "$CUSTOM_PRODUCT" \
  > "$STAGING/evidence/build.prop.custom" 2>/dev/null
cat "$STAGING/evidence/build.prop.stock" "$MARKER" \
  > "$STAGING/evidence/build.prop.expected"
cmp "$STAGING/evidence/build.prop.expected" "$STAGING/evidence/build.prop.custom"
debugfs -R "dump -p /app/Korri/Korri.apk $STAGING/evidence/Korri.extracted.apk" "$CUSTOM_PRODUCT" >/dev/null 2>&1
cmp "$APK" "$STAGING/evidence/Korri.extracted.apk"
[[ "$(sha256sum "$STAGING/evidence/Korri.extracted.apk" | awk '{print $1}')" == "$apk_sha256" ]]
"$HERE/verify-korri-launcher-apk.sh" "$STAGING/evidence/Korri.extracted.apk" "$STAGING/evidence/extracted-apk-verification" \
  > "$STAGING/evidence/extracted-apk-verifier.stdout.txt"
rm "$STAGING/evidence/Korri.extracted.apk"

"$HERE/extract-vbmeta-public-key.py" \
  "$STAGING/source/vbmeta_a.img" "$STAGING/ayn-root.avbpubkey" \
  > "$STAGING/evidence/ayn-root-key.txt"
avbtool info_image --image "$STAGING/source/vbmeta_a.img" \
  > "$STAGING/evidence/vbmeta.stock.txt"
stock_root_key_sha1="$(awk '/^Public key \(sha1\):/ {print $4; exit}' "$STAGING/evidence/vbmeta.stock.txt")"
boot_chain_key_sha1="$(awk '/Partition Name:/ {partition=$3} /Public key \(sha1\):/ && partition=="boot" {print $4; exit}' "$STAGING/evidence/vbmeta.stock.txt")"
recovery_chain_key_sha1="$(awk '/Partition Name:/ {partition=$3} /Public key \(sha1\):/ && partition=="recovery" {print $4; exit}' "$STAGING/evidence/vbmeta.stock.txt")"
[[ "$(sha1sum "$STAGING/ayn-root.avbpubkey" | awk '{print $1}')" == "$stock_root_key_sha1" ]]
[[ "$boot_chain_key_sha1" == "$stock_root_key_sha1" ]]
[[ "$recovery_chain_key_sha1" == "$stock_root_key_sha1" ]]
printf '%s\n' \
  "stock root key sha1: $stock_root_key_sha1" \
  "stock boot chain key sha1: $boot_chain_key_sha1" \
  "stock recovery chain key sha1: $recovery_chain_key_sha1" \
  > "$STAGING/evidence/stock-chain-key-sha1.txt"

make_hash_descriptor() {
  local partition="$1"
  local image_size="$2"
  local salt="$3"
  local source_image="$4"
  local output="$5"
  shift 5
  local raw="$STAGING/descriptor-inputs/$partition.raw"
  head -c "$image_size" "$source_image" > "$raw"
  avbtool add_hash_footer \
    --image "$raw" \
    --partition_size "$(stat -c %s "$source_image")" \
    --partition_name "$partition" \
    --hash_algorithm sha256 \
    --salt "$salt" \
    --do_not_append_vbmeta_image \
    --algorithm NONE \
    --output_vbmeta_image "$output" \
    "$@"
}

make_hash_descriptor \
  dtbo 12672324 553e7a7119c65a1e3559053e299966e7107110054988f37b43b85080f7fb6ad0 \
  "$STAGING/source/dtbo_a.img" "$STAGING/descriptor-inputs/dtbo.img" \
  --prop com.android.build.dtbo.fingerprint:qti/kalama/kalama:13/TKQ1.231222.001/Odin2Portal03122128:user/release-keys
make_hash_descriptor \
  init_boot 2023424 a1dcf8ef3a8f4efcfb976b16b9d93953504278fefb70e83ebaab8f9909ba8a1f \
  "$STAGING/source/init_boot_a.img" "$STAGING/descriptor-inputs/init_boot.img" \
  --prop com.android.build.init_boot.os_version:13 \
  --prop com.android.build.init_boot.fingerprint:qti/kalama/kalama:13/TKQ1.231222.001/Odin2Portal03122128:user/release-keys \
  --prop com.android.build.init_boot.security_patch:2024-01-01
make_hash_descriptor \
  vendor_boot 14782464 206dc8b2498ce5e9cb10f1109dbd0500ee2b4f190fa4ac06f43260a19b65c594 \
  "$STAGING/source/vendor_boot_a.img" "$STAGING/descriptor-inputs/vendor_boot.img" \
  --prop com.android.build.vendor_boot.fingerprint:qti/kalama/kalama:13/TKQ1.231222.001/Odin2Portal03122128:user/release-keys

CUSTOM_VBMETA_SYSTEM="$STAGING/NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable"
avbtool make_vbmeta_image \
  --output "$CUSTOM_VBMETA_SYSTEM" \
  --padding_size "$(stat -c %s "$STAGING/source/vbmeta_system_a.img")" \
  --algorithm SHA256_RSA4096 \
  --key "$SIGNING_KEY" \
  --rollback_index 1704067200 \
  --include_descriptors_from_image "$STAGING/logical/system_a.img" \
  --include_descriptors_from_image "$CUSTOM_PRODUCT" \
  --include_descriptors_from_image "$STAGING/logical/system_ext_a.img"

CUSTOM_VBMETA="$STAGING/NON_FLASHABLE_ARTIFACTS/vbmeta_a.img.not-flashable"
avbtool make_vbmeta_image \
  --output "$CUSTOM_VBMETA" \
  --padding_size "$(stat -c %s "$STAGING/source/vbmeta_a.img")" \
  --algorithm SHA256_RSA4096 \
  --key "$SIGNING_KEY" \
  --rollback_index 0 \
  --rollback_index_location 0 \
  --include_descriptors_from_image "$STAGING/descriptor-inputs/init_boot.img" \
  --include_descriptors_from_image "$STAGING/descriptor-inputs/vendor_boot.img" \
  --include_descriptors_from_image "$STAGING/logical/vendor_a.img" \
  --include_descriptors_from_image "$STAGING/logical/odm_a.img" \
  --include_descriptors_from_image "$STAGING/logical/vendor_dlkm_a.img" \
  --include_descriptors_from_image "$STAGING/logical/system_dlkm_a.img" \
  --include_descriptors_from_image "$STAGING/descriptor-inputs/dtbo.img" \
  --chain_partition "boot:3:$STAGING/ayn-root.avbpubkey" \
  --chain_partition "recovery:1:$STAGING/ayn-root.avbpubkey" \
  --chain_partition "vbmeta_system:2:$CUSTOM_PUBLIC_KEY"

avbtool info_image --image "$CUSTOM_VBMETA_SYSTEM" \
  > "$STAGING/evidence/vbmeta-system.signed.txt"
avbtool info_image \
  --image "$LAUNCHER_OUTPUT/NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable" \
  > "$STAGING/evidence/vbmeta-system.unsigned.txt"
avbtool info_image --image "$CUSTOM_VBMETA" \
  > "$STAGING/evidence/vbmeta.signed.txt"

normalize_root_info() {
  awk '
    /^Auxiliary Block:/ { print "Auxiliary Block: <KEY-SIZE-DEPENDENT>"; next }
    /^Public key \(sha1\):/ { print "Public key (sha1): <ROOT-SIGNING-KEY>"; next }
    /^Release String:/ { print "Release String: <AVBTOOL-VERSION>"; next }
    /Partition Name:/ { chain_partition = $3 }
    /Public key \(sha1\):/ && chain_partition == "vbmeta_system" {
      sub(/[0-9a-f]{40}/, "<VBMETA-SYSTEM-KEY>"); print; next
    }
    { print }
  ' "$1"
}
normalize_system_info() {
  awk '
    /^Authentication Block:/ { print "Authentication Block: <SIGNATURE>"; next }
    /^Auxiliary Block:/ { print "Auxiliary Block: <KEY-SIZE-DEPENDENT>"; next }
    /^Public key \(sha1\):/ { next }
    /^Algorithm:/ { print "Algorithm: <SIGNING-ALGORITHM>"; next }
    /^Release String:/ { print "Release String: <AVBTOOL-VERSION>"; next }
    { print }
  ' "$1"
}
normalize_root_info "$STAGING/evidence/vbmeta.stock.txt" \
  > "$STAGING/evidence/vbmeta.stock.normalized.txt"
normalize_root_info "$STAGING/evidence/vbmeta.signed.txt" \
  > "$STAGING/evidence/vbmeta.signed.normalized.txt"
diff -u "$STAGING/evidence/vbmeta.stock.normalized.txt" \
  "$STAGING/evidence/vbmeta.signed.normalized.txt" \
  > "$STAGING/evidence/vbmeta-descriptor.diff"
normalize_system_info "$STAGING/evidence/vbmeta-system.unsigned.txt" \
  > "$STAGING/evidence/vbmeta-system.unsigned.normalized.txt"
normalize_system_info "$STAGING/evidence/vbmeta-system.signed.txt" \
  > "$STAGING/evidence/vbmeta-system.signed.normalized.txt"
diff -u "$STAGING/evidence/vbmeta-system.unsigned.normalized.txt" \
  "$STAGING/evidence/vbmeta-system.signed.normalized.txt" \
  > "$STAGING/evidence/vbmeta-system-descriptor.diff"

verify="$STAGING/verify"
ln -s "$CUSTOM_VBMETA" "$verify/vbmeta.img"
ln -s "$CUSTOM_VBMETA_SYSTEM" "$verify/vbmeta_system.img"
for partition in boot recovery dtbo init_boot vendor_boot; do
  ln -s "$STAGING/source/${partition}_a.img" "$verify/$partition.img"
done
for partition in odm system_dlkm vendor vendor_dlkm system system_ext; do
  ln -s "$STAGING/logical/${partition}_a.img" "$verify/$partition.img"
done
ln -s "$CUSTOM_PRODUCT" "$verify/product.img"
avbtool verify_image \
  --image "$verify/vbmeta.img" \
  --key "$SIGNING_KEY" \
  --expected_chain_partition "boot:3:$STAGING/ayn-root.avbpubkey" \
  --expected_chain_partition "recovery:1:$STAGING/ayn-root.avbpubkey" \
  --expected_chain_partition "vbmeta_system:2:$CUSTOM_PUBLIC_KEY" \
  > "$STAGING/evidence/vbmeta-root-verify.txt"
avbtool verify_image --image "$verify/vbmeta_system.img" --key "$SIGNING_KEY" \
  > "$STAGING/evidence/vbmeta-system-verify.txt"
avbtool verify_image --image "$verify/boot.img" \
  > "$STAGING/evidence/boot-chain-verify.txt"
avbtool verify_image --image "$verify/recovery.img" \
  > "$STAGING/evidence/recovery-chain-verify.txt"

cp "$STAGING/ayn-root.avbpubkey" "$STAGING/evidence/ayn-root.avbpubkey"
(
  cd "$LAUNCHER_OUTPUT"
  sha256sum \
    NON_FLASHABLE_ARTIFACTS/super.img.not-flashable \
    NON_FLASHABLE_ARTIFACTS/product_a.img.not-flashable \
    > "$STAGING/evidence/generated-launcher-artifact-SHA256SUMS"
  sha256sum --check evidence/output-SHA256SUMS
) > "$STAGING/evidence/launcher-sha256-final.txt"
mv "$LAUNCHER_OUTPUT/NON_FLASHABLE_ARTIFACTS/super.img.not-flashable" \
  "$STAGING/NON_FLASHABLE_ARTIFACTS/super.img.not-flashable"
(
  cd "$STAGING"
  sha256sum \
    NON_FLASHABLE_ARTIFACTS/super.img.not-flashable \
    NON_FLASHABLE_ARTIFACTS/vbmeta_a.img.not-flashable \
    NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable \
    korri-odin2portal-avb.avbpubkey \
    > evidence/output-SHA256SUMS
)
cp "$NOTICE" "$STAGING/NOT-FLASHABLE.md"
cp "$PROVISIONING" "$STAGING/HOME-PROVISIONING.md"
printf '%s\n' \
  'ODIN2PORTAL_LAUNCHER_IMAGE_DRY_RUN_VERIFIED' \
  'root vbmeta: signed with Korri RSA-4096 key' \
  'vbmeta_system: signed with Korri RSA-4096 key' \
  'boot and recovery chain keys: unchanged AYN key' \
  'root descriptors: stock-equivalent except vbmeta_system key' \
  'system descriptors: launcher-product-equivalent' \
  'Korri APK: verified release signer and arm64 HOME activity' \
  'Korri APK path: /product/app/Korri/Korri.apk' \
  'AYN launcher: retained as fallback' \
  'default home provisioned: no' \
  'private key included: no' \
  'bootloader required: unlocked' \
  'flash ready: no' \
  'device writes: none' > "$STAGING/RESULT.txt"

if ! (
  cd "$SOURCE"
  sha256sum --check "$CONTRACT/SHA256SUMS"
) > "$STAGING/evidence/source-sha256-final.txt" 2>&1; then
  echo 'source changed during launcher image dry run' >&2
  exit 1
fi
if ! cmp -s "$CONTRACT/build-id.txt" "$SOURCE/build-id.txt" ||
   ! cmp -s "$CONTRACT/build-fingerprint.txt" "$SOURCE/build-fingerprint.txt" ||
   ! cmp -s "$CONTRACT/active-slot.txt" "$SOURCE/active-slot.txt" ||
   ! cmp -s "$CONTRACT/super-layout.txt" "$SOURCE/super-layout.txt"; then
  echo 'source metadata changed during launcher image dry run' >&2
  exit 1
fi
[[ "$(sha256sum "$PRIVATE_KEY" | awk '{print $1}')" == "$private_key_sha256" ]] || {
  echo 'private key changed during launcher image dry run' >&2
  exit 1
}
[[ "$(sha256sum "$SIGNING_KEY" | awk '{print $1}')" == "$private_key_sha256" ]]
[[ "$(sha256sum "$APK" | awk '{print $1}')" == "$apk_sha256" ]] || {
  echo 'Korri APK changed during launcher image dry run' >&2
  exit 1
}

"$HERE/redact-evidence-paths.py" "$STAGING/evidence" \
  "$STAGING" '<STAGING>' \
  "$SECURE_KEY_DIR" '<SECURE-KEY-DIRECTORY>' \
  "$SOURCE" '<SOURCE>' \
  "$PRIVATE_KEY" '<PRIVATE-KEY>'
rm -rf "$STAGING/source" "$STAGING/logical" "$STAGING/descriptor-inputs" \
  "$STAGING/verify" "$STAGING/product_a.img" "$STAGING/ayn-root.avbpubkey" \
  "$LAUNCHER_OUTPUT"
(
  cd "$STAGING"
  find . -type f -print0 | sort -z | \
    xargs -0 sha256sum > "$SECURE_KEY_DIR/MANIFEST-SHA256SUMS"
)
mv "$SECURE_KEY_DIR/MANIFEST-SHA256SUMS" "$STAGING/MANIFEST-SHA256SUMS"
rm -rf "$SECURE_KEY_DIR"
SECURE_KEY_DIR=""
find "$STAGING" -type f -exec sync -d {} +
sync -f "$STAGING"
mv -Tn "$STAGING" "$OUTPUT"
if [[ -e "$STAGING" ]]; then
  echo 'output appeared during signed AVB dry run; refusing to publish' >&2
  exit 1
fi
STAGING=""
PUBLISHED_OUTPUT="$OUTPUT"
grep -Fx 'ODIN2PORTAL_LAUNCHER_IMAGE_DRY_RUN_VERIFIED' "$OUTPUT/RESULT.txt" >/dev/null
grep -Fx 'default home provisioned: no' "$OUTPUT/RESULT.txt" >/dev/null
grep -Fx 'flash ready: no' "$OUTPUT/RESULT.txt" >/dev/null
(
  cd "$OUTPUT"
  sha256sum --check evidence/output-SHA256SUMS
  sha256sum --check MANIFEST-SHA256SUMS
) >/dev/null
sync -f "$OUTPUT_PARENT"
PUBLISHED_OUTPUT=""
printf 'ODIN2PORTAL_LAUNCHER_IMAGE_DRY_RUN_VERIFIED output=%s\n' "$OUTPUT"
