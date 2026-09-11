# Compare real package contents and the NixOS-compressed firmware union.
{ pkgs, korri }:
let
  lib = pkgs.lib;
  overlay = builtins.head (import ./firmware.nix { inherit pkgs lib; }).nixpkgs.overlays;
  trimmed = (pkgs.extend overlay).linux-firmware;
  original = pkgs.linux-firmware;
  normal = korri.nixosConfigurations.rg353m;
  rescue = korri.nixosConfigurations.rg353m-rescue;
  firmwareNames =
    device:
    lib.sort builtins.lessThan (
      map lib.getName (lib.concatLists device.options.hardware.firmware.definitions)
    );
  expectedNames = [
    "linux-firmware-rg353m"
    "rtl8761b-firmware"
    "wireless-regdb"
  ];
  # Native option apply preserves the same compression/priority behavior used
  # by the image. This baseline retains the observed rtl8761b overrides.
  baseline = normal.options.hardware.firmware.apply [
    pkgs.linux-firmware
    pkgs.rtl8761b-firmware
    pkgs.wireless-regdb
  ];
in
assert firmwareNames normal == expectedNames;
assert firmwareNames rescue == expectedNames;
assert !normal.config.hardware.enableRedistributableFirmware;
assert !normal.config.hardware.enableAllFirmware;
assert normal.config.hardware.wirelessRegulatoryDatabase;
assert rescue.config.hardware.wirelessRegulatoryDatabase;
assert korri.nixosConfigurations.odin2portal.pkgs.linux-firmware.pname == "linux-firmware";
assert korri.nixosConfigurations.odin2portal.config.hardware.enableRedistributableFirmware;
pkgs.runCommand "rg353m-firmware-check" { nativeBuildInputs = [ pkgs.python3 ]; } ''
  if python3 ${./firmware-check.py} ${original} ${original} ${original.src} ${baseline} ${normal.config.hardware.firmware} > negative.log 2>&1; then
    echo 'Untrimmed baseline unexpectedly passed' >&2
    exit 1
  fi
  grep -q 'unexpected entries:' negative.log
  python3 ${./firmware-check.py} ${original} ${trimmed} ${original.src} ${baseline} ${normal.config.hardware.firmware}
  touch "$out"
''
