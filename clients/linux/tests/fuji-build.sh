#!/usr/bin/env nix
#! nix shell nixpkgs#bash nixpkgs#nix --command bash
set -euo pipefail
cd "$(dirname "$0")/.."
nix build --impure --no-link --print-out-paths --expr '
let
  lock = builtins.fromJSON (builtins.readFile ./flake.lock);
  source = builtins.fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/${lock.nodes.nixpkgs.locked.rev}.tar.gz";
    sha256 = lock.nodes.nixpkgs.locked.narHash;
  };
  pkgs = import source { system = "aarch64-linux"; };
  engine = pkgs.stdenv.mkDerivation {
    name = "wpewebkit-prebuilt-test-only";
    dontUnpack = true;
    installPhase = "mkdir -p $out; ln -s /nix/store/s07if8sg6wx717j6350rg3zrh7ml0ms4-wpewebkit-2.52.3-dev/include $out/include; ln -s /nix/store/s07if8sg6wx717j6350rg3zrh7ml0ms4-wpewebkit-2.52.3-dev/lib $out/lib";
  };
in import ./package.nix { inherit pkgs; wpewebkit = engine; }
'
