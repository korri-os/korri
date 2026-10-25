# Exercise the shared family verifier against the actual RG DS firmware union.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  overlay = builtins.head (import ./firmware.nix { inherit pkgs lib; }).nixpkgs.overlays;
  trimmed = (pkgs.extend overlay).linux-firmware;
  original = pkgs.linux-firmware;
  cfg = configuration.config;
  firmwareNames = lib.sort builtins.lessThan (
    map lib.getName (lib.concatLists configuration.options.hardware.firmware.definitions)
  );
  baseline = configuration.options.hardware.firmware.apply [
    pkgs.linux-firmware
    pkgs.rtl8761b-firmware
    pkgs.wireless-regdb
  ];
in
assert
  firmwareNames == [
    "linux-firmware-rg353m"
    "rtl8761b-firmware"
    "wireless-regdb"
  ];
assert !cfg.hardware.enableRedistributableFirmware;
assert !cfg.hardware.enableAllFirmware;
assert cfg.hardware.wirelessRegulatoryDatabase;
pkgs.runCommand "rgds-firmware-check" { nativeBuildInputs = [ pkgs.python3 ]; } ''
  if python3 ${../rg353m/firmware-check.py} ${original} ${original} ${original.src} ${baseline} ${cfg.hardware.firmware} > negative.log 2>&1; then
    echo 'Untrimmed baseline unexpectedly passed' >&2
    exit 1
  fi
  grep -q 'unexpected entries:' negative.log
  python3 ${../rg353m/firmware-check.py} ${original} ${trimmed} ${original.src} ${baseline} ${cfg.hardware.firmware}
  # The running RG DS loaded rtw88_8821cs, not another vendor's Wi-Fi driver.
  test -f ${cfg.hardware.firmware}/lib/firmware/rtw88/rtw8821c_fw.bin.zst
  test -f ${cfg.hardware.firmware}/lib/firmware/rtl_bt/rtl8821cs_fw.bin.zst
  test -f ${cfg.hardware.firmware}/lib/firmware/rtl_bt/rtl8821cs_config.bin.zst
  touch "$out"
''
