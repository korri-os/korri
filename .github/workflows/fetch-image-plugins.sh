#!/usr/bin/env nix-shell
#! nix-shell -i bash -p nix jq
# Build-host only. Import the selected pinned archives with native Nix before
# evaluation forces their store contexts. Never contact the old raw cache.
set -euo pipefail

device=${1:?usage: fetch-image-plugins.sh DEVICE|--inputd|--all}
case "$device" in
  --all) architectures=(aarch64-linux x86_64-linux) ;;
  --inputd) architectures=("$(nix eval --impure --raw --expr builtins.currentSystem)") ;;
  rg353m|rgds|r36tmax|rpminiv2|odin2portal|rg35xxsp|rg35xxpro) architectures=(aarch64-linux) ;;
  *) echo "unsupported product image: $device" >&2; exit 2 ;;
esac

host=$(nix build --no-link --print-out-paths .#korri-plugin-host)
bindings=$(mktemp)
trap 'rm -f "$bindings"' EXIT
nix eval --impure --json --expr '(import ./nix/product/requirements.nix { korri = {}; }).constants.publishers' > "$bindings"
for architecture in "${architectures[@]}"; do
  expression="let korri = builtins.getFlake (toString ./.); pkgs = import korri.inputs.nixpkgs { system = builtins.currentSystem; }; in import ./nix/product/catalog-inputs.nix { inherit pkgs; system = \"$architecture\"; device = \"$device\"; }"
  records=$(nix eval --impure --json --expr "($expression).records")
  archives=$(nix eval --impure --json --expr "builtins.mapAttrs (_: archive: toString archive) ($expression).archives")
  catalog=$(nix eval --impure --raw --expr "toString ($expression).catalog")
  source=$(nix eval --impure --raw --expr "($expression).catalogUrl")
  nix build --impure --no-link --expr "builtins.attrValues ($expression).archives"
  mapfile -t pairs < <(printf '%s' "$records" | jq -r --argjson archives "$archives" 'to_entries[] | .value.plugin_id, $archives[.key]')
  "$host/bin/korri-plugin" image prepare "$bindings" "$architecture" "$catalog" "$source" "${pairs[@]}"
  echo "Verified pinned catalog archives and native Nix closure: $architecture ($device)"
done
