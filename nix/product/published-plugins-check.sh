#!/usr/bin/env nix-shell
#! nix-shell -i bash -p nix jq python3 git
# Build-host acceptance only. Never run this app on a Korri device.
set -euo pipefail
cd "${KORRI_ROOT:?task must supply the Core checkout}"

binding=$(nix eval --impure --json --expr '(import ./nix/product/requirements.nix { korri = {}; }).constants.publishers."@korri"')
cache=$(printf '%s' "$binding" | jq -er '.cacheUrl')
key=$(printf '%s' "$binding" | jq -er '.publicKey')
export NIX_CONFIG="${NIX_CONFIG:-}
extra-substituters = $cache
extra-trusted-public-keys = $key"

# Native path contexts substitute exact outputs, with no plugin derivations.
# Eval can fail on cache absence. It must never replace a pin with a recipe.
for architecture in aarch64-linux x86_64-linux; do
  nix eval --impure --json --option max-jobs 0 --option builders "" \
    --option fallback false --option require-sigs true \
    --expr "builtins.attrValues (import ./nix/product/published-plugins.nix { system = \"$architecture\"; })"
done
python3 nix/product/published-plugin-stability-check.py "$KORRI_ROOT"
system=$(nix eval --impure --raw --expr builtins.currentSystem)
nix build --no-link \
  ".#checks.$system.korri-published-plugins-aarch64-linux" \
  ".#checks.$system.korri-published-plugins-x86_64-linux" \
  ".#checks.$system.korri-published-game-launch" \
  ".#checks.$system.korri-published-interface-break" \
  ".#checks.$system.korri-product-image-plugins"
if [[ $system == x86_64-linux ]]; then
  nix build --no-link ".#checks.$system.korri-published-plugin-lifecycle"
else
  echo 'Offline image lifecycle acceptance requires the x86_64 build-host VM check.' >&2
fi
