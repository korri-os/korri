#!/usr/bin/env nix
#! nix shell nixpkgs#bash nixpkgs#python3 nixpkgs#mesa --command bash
# shellcheck shell=bash
set -euo pipefail
cd "$(dirname "$0")"
launcher="$(./fuji-build.sh | tail -1)/bin/korri-portal-wpe"
# Nix interpolation is deliberately not shell interpolation.
# shellcheck disable=SC2016
mesa="$(nix eval --impure --raw --expr '
let lock = builtins.fromJSON (builtins.readFile ../../../flake.lock);
    source = builtins.fetchTarball {
      url = "https://github.com/NixOS/nixpkgs/archive/${lock.nodes.nixpkgs.locked.rev}.tar.gz";
      sha256 = lock.nodes.nixpkgs.locked.narHash;
    };
in (import source { system = "aarch64-linux"; }).mesa.outPath')"
nix-store --realise "$mesa" >/dev/null
export GBM_BACKENDS_PATH="$mesa/lib/gbm"
export LIBGL_DRIVERS_PATH="$mesa/lib/dri"
export __EGL_VENDOR_LIBRARY_FILENAMES="$mesa/share/glvnd/egl_vendor.d/50_mesa.json"
python3 acceptance.py "$launcher" "$@"
