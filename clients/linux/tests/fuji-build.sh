#!/usr/bin/env nix
#! nix shell nixpkgs#bash nixpkgs#nix --command bash
# shellcheck shell=bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Nix interpolation is deliberately not shell interpolation.
# shellcheck disable=SC2016
nix build --builders "" --impure --no-link --print-out-paths --expr '
let
  lock = builtins.fromJSON (builtins.readFile ../../flake.lock);
  source = builtins.fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/${lock.nodes.nixpkgs.locked.rev}.tar.gz";
    sha256 = lock.nodes.nixpkgs.locked.narHash;
  };
  pkgs = import source { system = "aarch64-linux"; };
  dev = builtins.storePath /nix/store/s07if8sg6wx717j6350rg3zrh7ml0ms4-wpewebkit-2.52.3-dev;
  runtime = builtins.storePath /nix/store/df18k0jw3z9q6hkcdk0sdr8adf27dgp8-wpewebkit-2.52.3;
  engine = pkgs.stdenv.mkDerivation {
    name = "wpewebkit-prebuilt-test-only";
    dontUnpack = true;
    buildInputs = [ runtime ];
    installPhase = "mkdir -p $out; ln -s ${dev}/include $out/include; ln -s ${dev}/lib $out/lib";
  };
in import ./package.nix { inherit pkgs; wpewebkit = engine; }
'
