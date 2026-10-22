#!/usr/bin/env nix-shell
#! nix-shell -i bash -p nix jq
# An image may carry only packages already signed by the bound plugin cache.
# This runs on CI builders, never on a target device.
set -euo pipefail

device=${1:?usage: fetch-image-plugins.sh DEVICE}
case "$device" in
  rg353m|rgds|r36tmax|rpminiv2|odin2portal|rg35xxsp|rg35xxpro) ;;
  *) echo "unsupported product image: $device" >&2; exit 2 ;;
esac

binding=$(nix eval --json '.#nixosConfigurations.rg353m.config.services.korri.pluginHost.publishers' | jq -e '."@korri"')
cache=$(printf '%s' "$binding" | jq -er '.cacheUrl')
key=$(printf '%s' "$binding" | jq -er '.publicKey')
key_name=${key%%:*}
config="${NIX_CONFIG:-}
extra-substituters = $cache
extra-trusted-public-keys = $key"

if ! NIX_CONFIG="$config" nix config show substituters | grep -Fq "$cache"; then
  echo "plugin cache is not an effective substituter: $cache" >&2
  exit 1
fi
if ! NIX_CONFIG="$config" nix config show trusted-public-keys | grep -Fq "$key"; then
  echo "plugin publisher key is not trusted: $key_name" >&2
  exit 1
fi

# GitHub release payloads can return a transient miss even while another
# runner downloads the same asset. Nix's download-attempts does not retry 404s.
# Evaluation also fetches plugin references, before the explicit build below.
fetch_plugin() {
  local attempt status output
  local -a refresh=()
  for attempt in 1 2 3; do
    # A failed JSON command can write partial stdout. Keep only the successful
    # attempt's result; stderr still shows every original download failure.
    if output=$(NIX_CONFIG="$config" nix "${refresh[@]}" "$@"); then
      printf '%s\n' "$output"
      return 0
    else
      status=$?
    fi
    if (( attempt < 3 )); then
      echo "image plugin fetch failed; retry $((attempt + 1))/3 in $((attempt * 5))s" >&2
      sleep "$((attempt * 5))"
      # A metadata miss must not remain in Nix's one-hour negative cache.
      refresh=(--refresh)
    fi
  done
  return "$status"
}

paths=$(fetch_plugin eval --json ".#nixosConfigurations.$device.config.sdImage.storePaths" --apply 'xs: map toString xs')
mapfile -t packages < <(printf '%s' "$paths" | jq -r '.[] | select(endswith("-korri-plugin"))')
count=$(printf '%s' "$paths" | jq 'length')
if (( ${#packages[@]} < 1 || ${#packages[@]} != count - 1 )); then
  echo "device $device has an unexpected image plugin selection" >&2
  exit 1
fi

for package in "${packages[@]}"; do
  # Read the publisher cache itself. A local unsigned build with the same
  # store path does not count as a release.
  info=$(fetch_plugin path-info --store "$cache" --json "$package")
  if ! printf '%s' "$info" | jq -e --arg path "$package" --arg key "$key_name:" \
    '.[$path].signatures | any(startswith($key))' >/dev/null; then
    echo "plugin package lacks the bound publisher signature: $package" >&2
    exit 1
  fi
  fetch_plugin build --no-link "$package" \
    --option max-jobs 0 --option builders "" --option fallback false --option require-sigs true
  echo "verified signed image plugin: $package"
done

if [[ -n ${GITHUB_ENV:-} ]]; then
  {
    echo 'NIX_CONFIG<<KORRI_IMAGE_PLUGIN_CACHE'
    echo "$config"
    echo 'KORRI_IMAGE_PLUGIN_CACHE'
  } >> "$GITHUB_ENV"
fi
