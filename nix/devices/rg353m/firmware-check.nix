# Build real firmware outputs on the check host and compare every retained file.
{ pkgs, korri }:
let
  overlay = builtins.head (import ./firmware.nix { }).nixpkgs.overlays;
  trimmed = (pkgs.extend overlay).linux-firmware;
  original = pkgs.linux-firmware;
  normal = korri.nixosConfigurations.rg353m;
  rescue = korri.nixosConfigurations.rg353m-rescue;
  # The option's apply builds a compressed union. Inspect its native merged
  # package definitions here; content checks below compare the real packages.
  firmwareNames =
    device: map pkgs.lib.getName (pkgs.lib.concatLists device.options.hardware.firmware.definitions);
in
assert normal.pkgs.linux-firmware.pname == trimmed.pname;
assert rescue.pkgs.linux-firmware.pname == trimmed.pname;
assert builtins.elem trimmed.pname (firmwareNames normal);
assert builtins.elem trimmed.pname (firmwareNames rescue);
assert !(builtins.elem "linux-firmware" (firmwareNames normal));
assert korri.nixosConfigurations.odin2portal.pkgs.linux-firmware.pname == "linux-firmware";
assert normal.config.hardware.wirelessRegulatoryDatabase;
assert rescue.config.hardware.wirelessRegulatoryDatabase;
pkgs.runCommand "rg353m-firmware-phase1-check" { nativeBuildInputs = [ pkgs.python3 ]; } ''
  # The untrimmed baseline must fail the same content check.
  if python3 ${./firmware-check.py} ${original} ${original} > negative.log 2>&1; then
    echo 'Untrimmed baseline unexpectedly passed' >&2
    exit 1
  fi
  grep -q 'unexpected entries:' negative.log
  python3 ${./firmware-check.py} ${original} ${trimmed}
  touch "$out"
''
