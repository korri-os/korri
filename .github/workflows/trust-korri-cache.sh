#!/usr/bin/env bash
# Let a CI runner read Korri's own cache.
#
# A runner starts with cache.nixos.org and nothing else, so it cannot fetch what
# another job just published. On an aarch64 runner that shows up as a platform
# mismatch — it tries to build the cross-compiled kernel and firmware itself —
# rather than as a missing substituter, which is why this is its own step.
#
# The address and the keys come from nix/cache/identity.nix through the flake, so
# CI trusts exactly what a device trusts and adding a builder key stays one edit.
set -euo pipefail

url="$(nix eval --raw .#cache.url)"
keys="$(nix eval --json .#cache.publicKeys | tr -d '[]"' | tr ',' ' ')"

printf 'extra-substituters = %s\nextra-trusted-public-keys = %s\n' "$url" "$keys" |
  sudo tee -a /etc/nix/nix.conf >/dev/null

sudo systemctl restart nix-daemon

echo "trusting $url"
echo "keys: $keys"
