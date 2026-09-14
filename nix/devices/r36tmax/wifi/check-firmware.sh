#!/usr/bin/env nix
#! nix shell github:NixOS/nixpkgs/a6531044f6d0bef691ea18d4d4ce44d0daa6e816#bash github:NixOS/nixpkgs/a6531044f6d0bef691ea18d4d4ce44d0daa6e816#coreutils github:NixOS/nixpkgs/a6531044f6d0bef691ea18d4d4ce44d0daa6e816#findutils --command bash
set -euo pipefail
if [[ $# != 1 ]]; then
  echo "usage: $0 FIRMWARE-OUTPUT" >&2
  exit 2
fi
cd "$1/lib/firmware"
# Measured from the saved stock artifacts and the pinned upstream tree.
sha256sum --check <<'HASHES'
f2d3a67176eaf06bce02a50ffc292c89c5f02fe33ba66daf18dec3389377e9e4  rk915_fw.bin
d63f8d62e0ae0d7b21fd7b031a6e8360bcf90b722516c7e1278ed72c2e62c275  rk915_patch.bin
HASHES
test "$(find . -type f | wc -l)" -eq 2
