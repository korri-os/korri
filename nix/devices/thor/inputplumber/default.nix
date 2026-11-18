# InputPlumber data for the AYN Thor: the Thor composite from this directory
# with the Portal's capability maps (the same AYN MCU and volume keys). The
# Portal composite is not included; it matches only "AYN Odin 2 Portal".
{
  pkgs,
  inputplumber,
  runCommand,
}:

let
  src = runCommand "inputplumber-ayn-thor-src" { } ''
    mkdir -p $out/devices
    cp -r ${../../odin2portal/rocknix/inputplumber/capability_maps} $out/capability_maps
    cp ${./devices}/02-ayn-thor-controller.yaml $out/devices/
  '';
in
(import ../../../base/inputplumber-data.nix {
  inherit pkgs inputplumber src;
  name = "inputplumber-ayn-thor-data-2026-09-04";
  deviceCheck = ./check.py;
}).overrideAttrs
  {
    pname = "inputplumber-ayn-thor-data";
    version = "2026-09-04";
    meta.description = "InputPlumber composite and AYN MCU capability map for the AYN Thor";
  }
