#!/usr/bin/env nix-shell
#! nix-shell -i bash -p nix git
# Local VM check only. Never deploys or contacts a device.
set -euo pipefail
cd "$(dirname "$0")/../../.."
unset KORRI_WIFI_ENV
exec nix build --no-link --impure --print-out-paths --expr '
  let
    flake = builtins.getFlake (toString ./.);
    pkgs = import flake.inputs.nixpkgs { system = builtins.currentSystem; };
  in import ./nix/devices/rpminiv2/wifi-check.nix {
    inherit pkgs;
    configuration = flake.nixosConfigurations.rpminiv2;
  }
'
