#!/usr/bin/env nix-shell
#! nix-shell -i bash -p nix git coreutils curl jq gh python3
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
# own cache included. The list comes from the device being published, so there is
# no list here to keep in step with anything.
set -euo pipefail

usage() {
  echo "usage: publish.sh [--dry-run] [--package ATTR]... [<device>...]" >&2
  echo "       KORRI_CACHE_BATCH_TAG   another dated batch when today's release is full" >&2
  echo "       KORRI_CACHE_SECRET_KEY  this builder's signing key" >&2
  echo "       KORRI_CACHE_STORE       keep the local NAR cache here to reuse it" >&2
}

dry_run=0
devices=()
packages=()
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run)
      dry_run=1
      shift
      ;;
    # A device's closure is the unit that matters, but part of it is produced by
    # derivations of another system: an aarch64 device carries a kernel that is
    # cross-built on x86_64. A machine of that other system can publish those
    # outputs on their own, so a machine that has neither has nothing left to
    # build. Without this, an aarch64 CI runner is stuck the first time the
    # kernel changes.
    --package)
      packages+=("${2:?--package needs a flake attribute}")
      shift 2
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

if [ "${#devices[@]}" -eq 0 ] && [ "${#packages[@]}" -eq 0 ]; then
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
for device in ${devices[@]+"${devices[@]}"}; do
  if ! printf '%s\n' "${known[@]}" | grep -qxF "$device"; then
    echo "korri-cache: $device is not a Korri device; known: ${known[*]}" >&2
    exit 1
  fi
done

cache="$(nix eval --json .#cache)"
repo="$(jq -r .repo <<<"$cache")"
metadata_branch="$(jq -r .metadataBranch <<<"$cache")"
ours="$(jq -r .url <<<"$cache")"
download_base="$(jq -r .downloadBase <<<"$cache")"

# The devices decide what may be skipped. Asking them, rather than carrying a
# list, is what keeps the publisher from filtering against a cache a device does
# not trust: that would leave the device able to fetch Korri's output and unable
# to fetch something it depends on, at install time rather than at publish time.
# Every device shares nix/base, so any of them answers the question. When only
# packages are published there is still a device to ask: the caches a consumer
# trusts do not depend on which output is being published.
reference_devices=("${devices[@]}")
if [ "${#reference_devices[@]}" -eq 0 ]; then
  reference_devices=("${known[0]}")
fi

trusted=""
for device in "${reference_devices[@]}"; do
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

# Pages serves metadata directly, so prepare can verify Korri's signed narinfos
# through the same transport and signature checks as every other trusted cache.
upstreams=()
serves_us=0
while IFS= read -r entry; do
  upstreams+=(--upstream-cache "$entry")
  if [ "$entry" = "$ours" ]; then
    serves_us=1
  fi
done <<<"$trusted"

if [ "$serves_us" -eq 0 ]; then
  echo "korri-cache: $ours is not a device substituter; nothing would read this upload" >&2
  exit 1
fi

# Keep the prepared batch on the same filesystem as the signed cache. Naming a
# store is how a caller says where the room is, and the batch is a copy of part
# of that cache; putting it somewhere else defeats the point. A CI runner is the
# case that forced this: its Nix volume is carved out of /mnt, so /mnt itself has
# nearly nothing left and a default temporary directory lands there.
if [ -n "${KORRI_CACHE_STORE:-}" ]; then
  store="$KORRI_CACHE_STORE"
  mkdir -p "$store"
  # Inside the store, not beside it. A caller who names a store has made that one
  # directory writable and says nothing about its parent: on a CI runner the
  # parent is /nix/var, which belongs to root.
  work="$(mktemp -d -p "$store")"
else
  work="$(mktemp -d)"
  store="$work/store"
fi
trap 'rm -rf "$work"' EXIT

# One release per day. GitHub caps a release at 1,000 assets, and a dated tag
# keeps a bad batch deletable without touching the ones before it. When a day's
# release is full, KORRI_CACHE_BATCH_TAG names another dated batch
# (batch-YYYY-MM-DD-2, ...); the NAR URLs are fixed at prepare time, so the tag
# must be chosen before prepare, never changed after.
batch_tag="${KORRI_CACHE_BATCH_TAG:-batch-$(date -u +%Y-%m-%d)}"
if ! [[ "$batch_tag" =~ ^batch-[0-9]{4}-[0-9]{2}-[0-9]{2}(-[0-9]+)?$ ]]; then
  echo "korri-cache: batch tag must look like batch-YYYY-MM-DD or batch-YYYY-MM-DD-N: $batch_tag" >&2
  exit 2
fi

if [ "$dry_run" -eq 1 ]; then
  echo "korri-cache: would publish ${devices[*]-} ${packages[*]-}"
  echo "korri-cache: signing with $key into $store"
  echo "korri-cache: skipping whatever these caches already serve"
  printf '  %s\n' ${upstreams[@]+"${upstreams[@]}"} | grep -v -- '--upstream-cache'
  echo "korri-cache: uploading to $repo, NARs on $batch_tag, metadata on $metadata_branch (Pages)"
  exit 0
fi

mkdir -p "$store"

outputs=()
for device in ${devices[@]+"${devices[@]}"}; do
  echo "korri-cache: building what $device installs"
  outputs+=("$(
    nix build --no-link --print-out-paths \
      ".#nixosConfigurations.$device.config.system.build.toplevel"
  )")
done
for package in ${packages[@]+"${packages[@]}"}; do
  echo "korri-cache: building $package"
  # ^* selects every output, not just the default one. A kernel has out, dev and
  # modules, and a device installs the modules; publishing only the default
  # output leaves a machine that cannot cross-build unable to finish the closure,
  # and it reports that as a platform mismatch on the kernel derivation.
  while IFS= read -r out; do
    outputs+=("$out")
  done < <(nix build --no-link --print-out-paths ".#$package^*")
done

# zstd rather than the default xz: most of a device closure is public and will be
# dropped a moment later, so compressing it is throwaway work either way.
echo "korri-cache: signing ${#outputs[@]} closures into $store"
nix copy --to "file://$store?compression=zstd&secret-key=$key" "${outputs[@]}"

python3 "$publisher" prepare "$store" "$work/prepared" \
  --nar-base-url "$download_base$batch_tag/" \
  --cache-url "$ours" \
  "${upstreams[@]}"

shopt -s nullglob

prepared=("$work/prepared/metadata"/*.narinfo)
staged=("$store"/*.narinfo)

if [ "${#prepared[@]}" -eq 0 ]; then
  echo "korri-cache: all ${#staged[@]} paths are already served by a cache these devices trust"
  exit 0
fi


echo "korri-cache: ${#prepared[@]} of ${#staged[@]} paths are new, $(du -sh "$work/prepared/nars" | cut -f1) to upload"

if ! gh release view "$batch_tag" --repo "$repo" >/dev/null 2>&1; then
  gh release create "$batch_tag" --repo "$repo" --title "$batch_tag" \
    --notes "NAR payloads for Korri's signed Nix cache. Metadata lives at $ours."
fi

# NARs first: upload refuses to publish metadata that points at bytes GitHub does
# not already serve. Metadata publication also waits for exact bytes on Pages.
for part in nars metadata; do
  python3 "$publisher" upload "$work/prepared" \
    --repo "$repo" --tag "$batch_tag" --index-branch "$metadata_branch" --cache-url "$ours" --part "$part"
done

echo "korri-cache: ${devices[*]-} ${packages[*]-} published to $ours"
