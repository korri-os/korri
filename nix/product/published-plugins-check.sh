#!/usr/bin/env nix-shell
#! nix-shell -i bash -p nix jq python3 git curl
# Combined product/catalog acceptance. Build host only; never a target device.
set -euo pipefail
cd "${KORRI_ROOT:?task must supply the Core checkout}"
failed=0
gate() {
  local status=0
  "$@" || status=$?
  printf 'catalog gate: status=%s command=' "$status"
  printf '%q ' "$@"
  printf '\n'
  if (( status != 0 )); then failed=1; fi
}
# Retain every failure, but continue independent proofs. A bad published native
# path must not hide the cold VM or the other architecture's actual results.
gate bash .github/workflows/fetch-image-plugins.sh --all
gate python3 nix/product/published-plugin-stability-check.py "$KORRI_ROOT"
system=$(nix eval --impure --raw --expr builtins.currentSystem)
expression="let korri = builtins.getFlake (toString ./.); pkgs = import korri.inputs.nixpkgs { system = builtins.currentSystem; }; in import ./nix/product/catalog-plugins.nix { inherit pkgs; system = \"$system\"; withContext = false; }"
export KORRI_IMAGE_CATALOG KORRI_IMAGE_SOURCE KORRI_IMAGE_BINDINGS KORRI_PUBLISH_NIX KORRI_IMAGE_CURL KORRI_TEST_TRUE
KORRI_IMAGE_CATALOG=$(nix eval --impure --raw --expr "toString ($expression).catalog")
KORRI_IMAGE_SOURCE=$(nix eval --impure --raw --expr "($expression).catalogUrl")
KORRI_IMAGE_BINDINGS=$(mktemp)
trap 'rm -f "$KORRI_IMAGE_BINDINGS"' EXIT
nix eval --impure --json --expr '(import ./nix/product/requirements.nix { korri = {}; }).constants.publishers' > "$KORRI_IMAGE_BINDINGS"
KORRI_PUBLISH_NIX=$(readlink -f "$(command -v nix)")
KORRI_IMAGE_CURL=$(readlink -f "$(command -v curl)")
KORRI_TEST_TRUE=$(readlink -f "$(type -P true)")
gate nix develop .#plugin-host --command cargo test --locked --manifest-path services/korrid/plugin-host/Cargo.toml \
  --lib published_catalog_install_and_same_release_update -- --ignored
gate nix build --keep-going --no-link \
  ".#checks.$system.korri-published-plugins-aarch64-linux" \
  ".#checks.$system.korri-published-plugins-x86_64-linux" \
  ".#checks.$system.korri-published-game-launch" \
  ".#checks.$system.korri-published-interface-break" \
  ".#checks.$system.korri-product-image-plugins" \
  ".#checks.$system.korri-product" \
  ".#checks.$system.rpminiv2-delivery"
if [[ $system == x86_64-linux ]]; then
  gate nix build --no-link ".#checks.$system.korri-published-plugin-lifecycle"
else
  echo 'Offline image lifecycle acceptance requires the x86_64 build-host VM check.' >&2
  exit 1
fi
exit "$failed"
