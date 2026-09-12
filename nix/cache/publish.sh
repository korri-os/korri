#!/usr/bin/env nix-shell
#! nix-shell -i bash -p nix git coreutils jq gh python3
# Publish what a Korri device installs to Korri's signed cache.
#
# A device may not build: nix/device-cache/nixos-module.nix forces max-jobs = 0
# and fallback = false, so every path in its system closure must be downloadable
# or the generation cannot be installed. That closure is known at exactly one
# moment — when this repository builds it — which is why publishing lives here
# and not in a machine-wide Nix hook. A hook sees store paths with no idea which
# project asked for them, so it would publish whatever else that machine built.
#
# Nothing is uploaded that a cache the device already trusts can serve, Korri's
# own cache included. Both lists come from the device being published, so there
# is no list here to keep in step with anything.
set -euo pipefail

usage() {
  echo "usage: publish.sh [--dry-run] <device>..." >&2
  echo "       KORRI_CACHE_SECRET_KEY  this builder's signing key" >&2
  echo "       KORRI_CACHE_STORE       keep the local NAR cache here to reuse it" >&2
}

dry_run=0
devices=()
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run)
      dry_run=1
      shift
      ;;
    -*)
      usage
      exit 2
      ;;
    *)
      devices+=("$1")
      shift
      ;;
  esac
done

if [ "${#devices[@]}" -eq 0 ]; then
  usage
  exit 2
fi

cd "${KORRI_ROOT:?run through the korri-nix-cache-publish Nix app}"
publisher="$KORRI_ROOT/nix/cache/github-cache.py"

key="${KORRI_CACHE_SECRET_KEY:-$HOME/.config/korri/cache-key.secret}"
if [ ! -f "$key" ]; then
  echo "korri-cache: no signing key at $key; consumers refuse unsigned paths" >&2
  exit 1
fi
# A key others can read is a key that can sign for this builder.
if [ -n "$(find "$key" -perm /0077 -maxdepth 0)" ] || [ "${key#/nix/store/}" != "$key" ]; then
  echo "korri-cache: signing key must be private and outside the Nix store: $key" >&2
  exit 1
fi

mapfile -t known < <(nix eval --json .#nixosConfigurations --apply builtins.attrNames | jq -r '.[]')
for device in "${devices[@]}"; do
  if ! printf '%s\n' "${known[@]}" | grep -qxF "$device"; then
    echo "korri-cache: $device is not a Korri device; known: ${known[*]}" >&2
    exit 1
  fi
done

cache="$(nix eval --json .#cache)"
repo="$(jq -r .repo <<<"$cache")"
metadata_tag="$(jq -r .metadataTag <<<"$cache")"
ours="$(jq -r .url <<<"$cache")"
download_base="$(jq -r .downloadBase <<<"$cache")"

# The devices decide what may be skipped. Asking them, rather than carrying a
# list, is what keeps the publisher from filtering against a cache a device does
# not trust: that would leave the device able to fetch Korri's output and unable
# to fetch something it depends on, at install time rather than at publish time.
trusted=""
for device in "${devices[@]}"; do
  device_trusted="$(
    nix eval --json \
      ".#nixosConfigurations.$device.config.nix.settings.substituters" |
      jq -r 'unique | .[]'
  )"
  if [ -n "$trusted" ] && [ "$device_trusted" != "$trusted" ]; then
    echo "korri-cache: these devices trust different caches; publish them separately" >&2
    exit 1
  fi
  trusted="$device_trusted"
done

upstreams=()
serves_us=0
while IFS= read -r entry; do
  [ "$entry" = "$ours" ] && serves_us=1
  upstreams+=(--upstream-cache "$entry")
done <<<"$trusted"

if [ "$serves_us" -eq 0 ]; then
  echo "korri-cache: $ours is not a device substituter; nothing would read this upload" >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# Korri's own cache stays in the list above, so a path it already serves is
# dropped here instead of being uploaded a second time.
store="${KORRI_CACHE_STORE:-$work/store}"

# One release per day. GitHub caps a release at 1,000 assets, and a dated tag
# keeps a bad batch deletable without touching the ones before it.
batch_tag="batch-$(date -u +%Y-%m-%d)"

if [ "$dry_run" -eq 1 ]; then
  echo "korri-cache: would publish ${devices[*]}"
  echo "korri-cache: signing with $key into $store"
  echo "korri-cache: skipping whatever these caches already serve"
  printf '  %s\n' ${upstreams[@]+"${upstreams[@]}"} | grep -v -- '--upstream-cache'
  echo "korri-cache: uploading to $repo, NARs on $batch_tag, metadata on $metadata_tag"
  exit 0
fi

mkdir -p "$store"

outputs=()
for device in "${devices[@]}"; do
  echo "korri-cache: building what $device installs"
  outputs+=("$(
    nix build --no-link --print-out-paths \
      ".#nixosConfigurations.$device.config.system.build.toplevel"
  )")
done

# zstd rather than the default xz: most of a device closure is public and will be
# dropped a moment later, so compressing it is throwaway work either way.
echo "korri-cache: signing ${#outputs[@]} closures into $store"
nix copy --to "file://$store?compression=zstd&secret-key=$key" "${outputs[@]}"

python3 "$publisher" prepare "$store" "$work/prepared" \
  --nar-base-url "$download_base$batch_tag/" \
  "${upstreams[@]}"

shopt -s nullglob
prepared=("$work/prepared/metadata"/*.narinfo)
staged=("$store"/*.narinfo)

if [ "${#prepared[@]}" -eq 0 ]; then
  echo "korri-cache: all ${#staged[@]} paths are already served by a cache these devices trust"
  exit 0
fi

echo "korri-cache: ${#prepared[@]} of ${#staged[@]} paths are new, $(du -sh "$work/prepared/nars" | cut -f1) to upload"

if ! gh release view "$metadata_tag" --repo "$repo" >/dev/null 2>&1; then
  echo "korri-cache: $repo has no '$metadata_tag' release to hold cache metadata" >&2
  exit 1
fi
if ! gh release view "$batch_tag" --repo "$repo" >/dev/null 2>&1; then
  gh release create "$batch_tag" --repo "$repo" --title "$batch_tag" \
    --notes "NAR payloads for Korri's signed Nix cache. Metadata lives on the '$metadata_tag' release."
fi

# NARs first: upload refuses to publish metadata that points at bytes GitHub does
# not already serve.
for part in nars metadata; do
  python3 "$publisher" upload "$work/prepared" \
    --repo "$repo" --tag "$batch_tag" --cache-tag "$metadata_tag" --part "$part"
done

echo "korri-cache: ${devices[*]} published to $ours"
