#!/usr/bin/env nix-shell
#! nix-shell -i bash -p nix git coreutils jq gh python3
# Upload what this machine built to Korri's signed cache.
#
# nix/cache/post-build-hook.sh signs every locally built path into a staging
# file cache. This is the batch half: it filters the staged paths against the
# caches Korri's own consumers trust, uploads what is left, and clears the
# entries it handled. A builder runs it on a timer with no arguments; nothing
# here names a cache address, a key, or an attribute list.
set -euo pipefail

dry_run=0
staging="${KORRI_CACHE_STAGING:-/var/cache/korri-nix-cache}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --staging)
      staging="${2:?--staging needs a directory}"
      shift 2
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    *)
      echo "usage: upload-staged.sh [--staging DIR] [--dry-run]" >&2
      exit 2
      ;;
  esac
done

cd "${KORRI_ROOT:?run through the korri-nix-cache-upload Nix app}"
publisher="$KORRI_ROOT/nix/cache/github-cache.py"

# An idle builder is the normal case. Say nothing worth reading and succeed, so
# a timer failure always means a real failure.
shopt -s nullglob
records=("$staging"/*.narinfo)
if [ "${#records[@]}" -eq 0 ]; then
  echo "korri-cache: nothing staged in $staging"
  exit 0
fi

cache="$(nix eval --json "$KORRI_ROOT#cache")"
repo="$(jq -r .repo <<<"$cache")"
metadata_tag="$(jq -r .metadataTag <<<"$cache")"
ours="$(jq -r .url <<<"$cache")"
download_base="$(jq -r .downloadBase <<<"$cache")"

# The consumers decide what may be filtered out, so the list comes from a real
# device configuration rather than from this script. Any device answers the
# question: nix/device-cache/module-check.nix:41 asserts every device's
# substituters equal the builder's, and nix/base/default.nix is shared.
# NixOS merges module definitions with its own default, so the list can repeat a
# cache; unique keeps prepare from fetching the same metadata twice.
mapfile -t trusted < <(
  nix eval --json \
    "$KORRI_ROOT#nixosConfigurations.rg353m.config.nix.settings.substituters" |
    jq -r 'unique | .[]'
)

upstreams=()
serves_us=0
for entry in "${trusted[@]}"; do
  if [ "$entry" = "$ours" ]; then
    serves_us=1
  else
    upstreams+=(--upstream-cache "$entry")
  fi
done

# Both guards protect the same invariant from opposite sides. Without Korri's
# cache in the list, no consumer would ever read what this uploads. Without an
# upstream, prepare would keep every path in the closure and ship gigabytes of
# glibc and friends that consumers can already resolve.
if [ "$serves_us" -eq 0 ]; then
  echo "korri-cache: $ours is not a device substituter; nothing would read this upload" >&2
  exit 1
fi
if [ "${#upstreams[@]}" -eq 0 ]; then
  echo "korri-cache: consumers trust no public cache, so nothing could be filtered out" >&2
  exit 1
fi

# One release per day. GitHub caps a release at 1,000 assets, and a dated tag
# keeps a bad batch deletable without touching the ones before it.
batch_tag="batch-$(date -u +%Y-%m-%d)"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
python3 "$publisher" prepare "$staging" "$work/prepared" \
  --nar-base-url "$download_base$batch_tag/" \
  "${upstreams[@]}"

prepared=("$work/prepared/metadata"/*.narinfo)

drop_staged() {
  local record url
  for record in "${records[@]}"; do
    url="$(sed -n 's/^URL: //p' "$record")"
    # Two staged paths with identical contents share one NAR, so the second
    # removal is expected to find nothing.
    [ -z "$url" ] || rm -f "$staging/$url"
    rm -f "$record"
  done
}

if [ "${#prepared[@]}" -eq 0 ]; then
  echo "korri-cache: all ${#records[@]} staged paths are already publicly cached"
  [ "$dry_run" -eq 1 ] || drop_staged
  exit 0
fi

echo "korri-cache: ${#prepared[@]} of ${#records[@]} staged paths are Korri's to publish"
if [ "$dry_run" -eq 1 ]; then
  du -sh "$work/prepared/nars"
  exit 0
fi

if ! gh release view "$metadata_tag" --repo "$repo" >/dev/null 2>&1; then
  echo "korri-cache: $repo has no '$metadata_tag' release to hold cache metadata" >&2
  exit 1
fi
if ! gh release view "$batch_tag" --repo "$repo" >/dev/null 2>&1; then
  gh release create "$batch_tag" --repo "$repo" --title "$batch_tag" \
    --notes "NAR payloads for Korri's signed Nix cache. Metadata lives on the '$metadata_tag' release."
fi

# NARs first: upload refuses to publish metadata that points at bytes GitHub
# does not already serve.
for part in nars metadata; do
  python3 "$publisher" upload "$work/prepared" \
    --repo "$repo" --tag "$batch_tag" --cache-tag "$metadata_tag" --part "$part"
done

drop_staged
echo "korri-cache: published to $ours"
