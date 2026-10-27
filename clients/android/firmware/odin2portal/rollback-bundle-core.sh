#!/usr/bin/env bash
set -Eeuo pipefail

SOURCE="${1:?usage: rollback-bundle-core.sh <stock-source-directory> <output-directory> <contract-directory> <procedure-file>}"
OUTPUT="${2:?usage: rollback-bundle-core.sh <stock-source-directory> <output-directory> <contract-directory> <procedure-file>}"
CONTRACT="${3:?usage: rollback-bundle-core.sh <stock-source-directory> <output-directory> <contract-directory> <procedure-file>}"
PROCEDURE="${4:?usage: rollback-bundle-core.sh <stock-source-directory> <output-directory> <contract-directory> <procedure-file>}"

SOURCE="$(cd "$SOURCE" && pwd -P)"
CONTRACT="$(cd "$CONTRACT" && pwd -P)"
PROCEDURE="$(cd "$(dirname "$PROCEDURE")" && pwd -P)/$(basename "$PROCEDURE")"
OUTPUT_PARENT="$(cd "$(dirname "$OUTPUT")" && pwd -P)"
OUTPUT="$OUTPUT_PARENT/$(basename "$OUTPUT")"

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
if [[ ! -O "$OUTPUT_PARENT" ]] ||
   find "$OUTPUT_PARENT" -maxdepth 0 -perm /022 -print -quit | grep -q .; then
  echo 'output parent must be owned by the current user and not group/world-writable' >&2
  exit 1
fi

required_contract_files=(
  SHA256SUMS active-slot.txt build-fingerprint.txt build-id.txt super-layout.txt
)
for file in "${required_contract_files[@]}"; do
  if [[ ! -f "$CONTRACT/$file" || -L "$CONTRACT/$file" ]]; then
    echo "source contract is incomplete: missing regular $file" >&2
    exit 1
  fi
done
if [[ ! -f "$PROCEDURE" || -L "$PROCEDURE" ]]; then
  echo "rollback procedure is missing or not regular: $PROCEDURE" >&2
  exit 1
fi

images=()
declare -A image_names=()
while read -r checksum filename extra; do
  if [[ ! "$checksum" =~ ^[0-9a-f]{64}$ ||
        ! "$filename" =~ ^[a-z0-9_]+\.img$ ||
        -n "${extra:-}" ||
        -n "${image_names[$filename]+present}" ]]; then
    echo 'source checksum contract has an invalid image entry' >&2
    exit 1
  fi
  images+=("$filename")
  image_names["$filename"]=1
done < "$CONTRACT/SHA256SUMS"
if [[ "${#images[@]}" -ne 15 ]]; then
  echo 'source checksum contract must name exactly 15 Android images' >&2
  exit 1
fi

source_metadata=(
  SHA256SUMS.local active-slot.txt build-fingerprint.txt build-id.txt super-layout.txt
)
for file in "${images[@]}" "${source_metadata[@]}"; do
  if [[ ! -f "$SOURCE/$file" || -L "$SOURCE/$file" ]]; then
    echo "source file is missing, not regular, or symbolic: $file" >&2
    exit 1
  fi
done

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
if ! cmp -s "$CONTRACT/active-slot.txt" "$SOURCE/active-slot.txt"; then
  echo 'source active slot does not match the build contract' >&2
  exit 1
fi
if ! cmp -s "$CONTRACT/super-layout.txt" "$SOURCE/super-layout.txt"; then
  echo 'source super layout does not match the build contract' >&2
  exit 1
fi
if ! cmp -s "$CONTRACT/SHA256SUMS" "$SOURCE/SHA256SUMS.local"; then
  echo 'source local checksum manifest does not match the build contract' >&2
  exit 1
fi

umask 077
STAGING="$(mktemp -d "$OUTPUT_PARENT/.odin2portal-rollback-bundle.XXXXXX")"
PUBLISHED_OUTPUT=""
cleanup() {
  [[ -z "${STAGING:-}" ]] || rm -rf "$STAGING"
  [[ -z "${PUBLISHED_OUTPUT:-}" ]] || rm -rf --one-file-system -- "$PUBLISHED_OUTPUT"
}
trap cleanup EXIT

for image in "${images[@]}"; do
  cp --reflink=auto --sparse=always "$SOURCE/$image" "$STAGING/$image"
done
cp "$SOURCE/SHA256SUMS.local" "$STAGING/SHA256SUMS.local"
cp "$SOURCE/active-slot.txt" "$STAGING/active-slot.txt"
cp "$SOURCE/build-fingerprint.txt" "$STAGING/build-fingerprint.txt"
cp "$SOURCE/build-id.txt" "$STAGING/build-id.txt"
cp "$SOURCE/super-layout.txt" "$STAGING/super-layout.txt"
cp "$PROCEDURE" "$STAGING/README.md"

expected_files=(
  "${images[@]}" README.md SHA256SUMS.local active-slot.txt
  build-fingerprint.txt build-id.txt super-layout.txt
)
declare -A expected_names=()
for file in "${expected_files[@]}"; do
  expected_names["$file"]=1
done

verify_bundle() {
  local directory="$1"
  local entry name count=0
  while IFS= read -r -d '' entry; do
    name="${entry##*/}"
    if [[ -L "$entry" || ! -f "$entry" || -z "${expected_names[$name]+present}" ]]; then
      echo "rollback bundle contains an unexpected or non-regular entry: $name" >&2
      return 1
    fi
    count=$((count + 1))
  done < <(find "$directory" -mindepth 1 -maxdepth 1 -print0)
  if [[ "$count" -ne "${#expected_files[@]}" ]]; then
    echo 'rollback bundle file inventory is incomplete' >&2
    return 1
  fi
  if ! cmp -s "$CONTRACT/SHA256SUMS" "$directory/SHA256SUMS.local" ||
     ! cmp -s "$CONTRACT/active-slot.txt" "$directory/active-slot.txt" ||
     ! cmp -s "$CONTRACT/build-fingerprint.txt" "$directory/build-fingerprint.txt" ||
     ! cmp -s "$CONTRACT/build-id.txt" "$directory/build-id.txt" ||
     ! cmp -s "$CONTRACT/super-layout.txt" "$directory/super-layout.txt" ||
     ! cmp -s "$PROCEDURE" "$directory/README.md"; then
    echo 'rollback bundle metadata differs from the trusted contract' >&2
    return 1
  fi
  if ! (
    cd "$directory"
    sha256sum --check "$CONTRACT/SHA256SUMS"
  ) >/dev/null 2>&1; then
    echo 'rollback bundle images differ from the trusted contract' >&2
    return 1
  fi
}

verify_bundle "$STAGING"

if ! (
  cd "$SOURCE"
  sha256sum --check "$CONTRACT/SHA256SUMS"
) >/dev/null 2>&1; then
  echo 'source changed during rollback staging' >&2
  exit 1
fi
if ! cmp -s "$CONTRACT/SHA256SUMS" "$SOURCE/SHA256SUMS.local" ||
   ! cmp -s "$CONTRACT/build-id.txt" "$SOURCE/build-id.txt" ||
   ! cmp -s "$CONTRACT/build-fingerprint.txt" "$SOURCE/build-fingerprint.txt" ||
   ! cmp -s "$CONTRACT/active-slot.txt" "$SOURCE/active-slot.txt" ||
   ! cmp -s "$CONTRACT/super-layout.txt" "$SOURCE/super-layout.txt"; then
  echo 'source metadata changed during rollback staging' >&2
  exit 1
fi

sync -d "$STAGING"/*
sync -f "$STAGING"
mv -Tn "$STAGING" "$OUTPUT"
if [[ -e "$STAGING" ]]; then
  echo 'output appeared during rollback staging; refusing to publish' >&2
  exit 1
fi
STAGING=""
PUBLISHED_OUTPUT="$OUTPUT"
if [[ "$(stat -c %a "$OUTPUT")" != 700 ]]; then
  echo 'published rollback bundle is not private mode 0700' >&2
  exit 1
fi
verify_bundle "$OUTPUT"
sync -f "$OUTPUT_PARENT"
PUBLISHED_OUTPUT=""
printf 'ODIN2PORTAL_ROLLBACK_BUNDLE_VERIFIED output=%s\n' "$OUTPUT"
