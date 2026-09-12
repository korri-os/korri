#!/usr/bin/env bash
# Let a CI runner read Korri's own cache, and prove it can before building.
#
# A runner starts with cache.nixos.org and nothing else, so it cannot fetch what
# another job just published. On an aarch64 runner that surfaces as a platform
# mismatch — it tries to build the cross-compiled kernel or firmware itself —
# never as a missing substituter, so this step verifies rather than assumes.
#
# The settings travel in NIX_CONFIG through $GITHUB_ENV rather than being written
# into /etc/nix/nix.conf: appending to that file after the installer has run had
# no effect on the daemon, and an environment variable needs no guess about which
# config file wins. The runner's user is a trusted Nix user, so extra-substituters
# and extra-trusted-public-keys from the client are honoured.
#
# The address and keys come from nix/cache/identity.nix through the flake, so CI
# trusts exactly what a device trusts and adding a builder key stays one edit.
set -euo pipefail

url="$(nix eval --raw .#cache.url)"
keys="$(nix eval --json .#cache.publicKeys | tr -d '[]"' | tr ',' ' ')"

config="extra-substituters = $url
extra-trusted-public-keys = $keys"

effective="$(NIX_CONFIG="$config" nix config show substituters)"
if ! grep -qF "$url" <<<"$effective"; then
  echo "Korri's cache did not reach the effective Nix configuration." >&2
  echo "substituters = $effective" >&2
  exit 1
fi

{
  echo 'NIX_CONFIG<<KORRI_NIX_CONFIG'
  echo "$config"
  echo 'KORRI_NIX_CONFIG'
} >> "${GITHUB_ENV:?this step runs inside GitHub Actions}"

echo "trusting $url"
echo "substituters now: $effective"
