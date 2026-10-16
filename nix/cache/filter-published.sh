#!/usr/bin/env bash
# Filter a prepared batch against the device's own cache.
set -euo pipefail

prepared="${1:?prepared cache directory required}"
ours="${2:?cache URL required}"
remote="$(mktemp -p "$prepared")"
trap 'rm -f "$remote"' EXIT

# Drop what Korri's cache already serves, so a second publish of the same
# generation uploads only what changed. A narinfo is named after the store path
# hash, so its presence identifies the exact path; compare StorePath anyway,
# because a published narinfo is the claim being relied on.
shopt -s nullglob
republished=0
for narinfo in "$prepared/metadata"/*.narinfo; do
  hash="$(basename "$narinfo" .narinfo)"
  curl -fsSL --max-time 60 -o "$remote" "$ours$hash.narinfo" || continue
  if grep -qxF "$(grep '^StorePath: ' "$narinfo")" "$remote"; then
    rm -f "$narinfo"
    republished=$((republished + 1))
  fi
done

# Different store paths can contain identical bytes and share one compressed
# NAR. Decide which payloads to drop only after all metadata has been filtered.
declare -A needed_nars=()
for narinfo in "$prepared/metadata"/*.narinfo; do
  needed_nars["$(sed -n 's|^URL: .*/||p' "$narinfo")"]=1
done
for nar in "$prepared/nars"/*; do
  if [ -z "${needed_nars[${nar##*/}]+present}" ]; then
    rm -f "$nar"
  fi
done

if [ "$republished" -gt 0 ]; then
  echo "korri-cache: $republished paths are already published, so they are not uploaded again"
fi
