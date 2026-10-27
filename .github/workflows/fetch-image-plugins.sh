#!/usr/bin/env nix-shell
#! nix-shell -i bash -p nix jq python3
# Images and Inputd checks use only packages signed by the bound plugin cache.
# This runs on CI builders, never on a target device.
set -euo pipefail

device=${1:?usage: fetch-image-plugins.sh DEVICE|--inputd}
case "$device" in
  --inputd)
    label=Inputd
    binding=$(nix eval --impure --json --expr '(import ./nix/product/requirements.nix { korri = {}; }).constants.publishers' | jq -e '."@korri"')
    # flake.nix supplies this exact native pin to Inputd's module/seat checks.
    pin_expression='[(import ./nix/product/published-plugins.nix { system = builtins.currentSystem; withContext = false; }).korri-plugin-sunshine]'
    ;;
  rg353m|rgds|r36tmax|rpminiv2|odin2portal|rg35xxsp|rg35xxpro)
    label=image
    binding=$(nix eval --json '.#nixosConfigurations.rg353m.config.services.korri.pluginHost.publishers' | jq -e '."@korri"')
    pin_expression='builtins.attrValues (import ./nix/product/published-plugins.nix { system = "aarch64-linux"; withContext = false; })'
    ;;
  *) echo "unsupported product image: $device" >&2; exit 2 ;;
esac

read_parser="${KORRI_PLUGIN_READ_PARSER:-$(dirname "${BASH_SOURCE[0]}")/../../nix/cache/plugin-read-failure.py}"
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

# These are facts from the same producer, not an extra pin list. Reading facts
# must not force appendContext's cold substitution before retry scope is known.
pins=$(nix eval --impure --json --expr "$pin_expression")

# GitHub release payloads can return a transient miss even while another
# runner downloads the same asset. Nix's download-attempts does not retry 404s.
# Evaluation also fetches plugin references, before the explicit build below.
# Under load GitHub also answers 403 for a while (run 39744047580: a 404, then
# 403 for every request in the next 15 s), so the waits are long: 30, 60 and
# 120 s by default. The tests shorten them.
retry_seconds="${KORRI_IMAGE_PLUGIN_RETRY_SECONDS:-30}"
attempts=4
fetch_errors=$(mktemp)
trap 'rm -f "$fetch_errors"' EXIT
fetch_plugin() {
  local attempt status output delay requested_path
  requested_path=${!#}
  local -a refresh=() missing=()
  for (( attempt = 1; attempt <= attempts; attempt++ )); do
    missing=()
    # A failed JSON command can write partial stdout. Keep only the successful
    # attempt's result; stderr still shows every original download failure.
    if output=$(NIX_CONFIG="$config" nix "${refresh[@]}" "$@" 2>"$fetch_errors"); then
      # Nix 2.34 reports an absent remote narinfo as successful JSON null.
      # Treat only this exact requested-path result as a missing read. Invalid
      # metadata/signatures and unrelated successful responses never qualify.
      if [[ "$1" == path-info ]] && printf '%s' "$output" | jq -e --arg path "$requested_path" \
        'type == "object" and length == 1 and has($path) and .[$path] == null' >/dev/null; then
        status=1
        missing=(--missing-metadata)
        echo "plugin metadata missing from bound cache: $requested_path" >&2
      else
        cat "$fetch_errors" >&2
        printf '%s\n' "$output"
        return 0
      fi
    else
      status=$?
    fi
    cat "$fetch_errors" >&2
    # Evaluation can fail an assertion before a download starts. Retry only
    # transport reads, never compiler/assertion failures or invalid metadata.
    if (( status != 1 )) || ! python3 "$read_parser" "${missing[@]}" "$cache" "$pins" "$@" < "$fetch_errors"; then
      return "$status"
    fi
    if (( attempt < attempts )); then
      delay=$(( retry_seconds * (1 << (attempt - 1)) ))
      echo "$label plugin fetch failed; retry $((attempt + 1))/$attempts in ${delay}s" >&2
      sleep "$delay"
      # A metadata miss must not remain in Nix's one-hour negative cache.
      refresh=(--refresh)
    fi
  done
  return "$status"
}

if [[ "$device" == --inputd ]]; then
  mapfile -t packages < <(printf '%s' "$pins" | jq -r '.[]')
else
  paths=$(fetch_plugin eval --json ".#nixosConfigurations.$device.config.sdImage.storePaths" --apply 'xs: map toString xs')
  mapfile -t packages < <(printf '%s' "$paths" | jq -r '.[] | select(endswith("-korri-plugin"))')
  count=$(printf '%s' "$paths" | jq 'length')
  if (( ${#packages[@]} < 1 || ${#packages[@]} != count - 1 )); then
    echo "device $device has an unexpected image plugin selection" >&2
    exit 1
  fi
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
  echo "verified signed $label plugin: $package"
done

if [[ -n ${GITHUB_ENV:-} ]]; then
  {
    echo 'NIX_CONFIG<<KORRI_IMAGE_PLUGIN_CACHE'
    echo "$config"
    echo 'KORRI_IMAGE_PLUGIN_CACHE'
  } >> "$GITHUB_ENV"
fi
