{
  lib,
  fetchurl,
  linuxManualConfig,
  stdenv,
  pkgs,
  features ? { },
  ...
}:
let
  version = "7.2";
  patchDir = ./patches;
  patchNames = lib.sort lib.lessThan (
    builtins.filter (name: lib.hasSuffix ".patch" name) (builtins.attrNames (builtins.readDir patchDir))
  );

  panelFirmware = ./firmware/panels;

  kernel = linuxManualConfig {
    inherit version stdenv;
    modDirVersion = "7.2.0";
    src = fetchurl {
      url = "https://cdn.kernel.org/pub/linux/kernel/v7.x/linux-${version}.tar.xz";
      hash = "sha256-+f7z0UwN9TgZAm9L50RZg1wqCw3L9bW72eoZ8IKUArM=";
    };
    kernelPatches = map (name: {
      inherit name;
      patch = patchDir + "/${name}";
    }) patchNames;
    configfile = ./config;
    allowImportFromDerivation = true;
    extraMeta = {
      description = "Linux ${version} with ROCKNIX H700 display and hardware patches for Anbernic RG35XXSP";
      platforms = [ "aarch64-linux" ];
      license = lib.licenses.gpl2Only;
    };
  };
in
kernel.overrideAttrs (previous: {
  postPatch =
    (previous.postPatch or "")
    + ''
      install -Dm444 ${pkgs.linux-firmware}/lib/firmware/rtl_bt/rtl8821cs_config.bin external-firmware/rtl_bt/rtl8821cs_config.bin
      install -Dm444 ${pkgs.linux-firmware}/lib/firmware/rtl_bt/rtl8821cs_fw.bin external-firmware/rtl_bt/rtl8821cs_fw.bin
      install -Dm444 ${pkgs.linux-firmware}/lib/firmware/rtw88/rtw8821c_fw.bin external-firmware/rtw88/rtw8821c_fw.bin
      install -Dm444 ${panelFirmware}/anbernic,rg35xx-plus-panel.panel external-firmware/panels/anbernic,rg35xx-plus-panel.panel
      install -Dm444 ${panelFirmware}/anbernic,rg35xx-plus-rev6-panel.panel external-firmware/panels/anbernic,rg35xx-plus-rev6-panel.panel
      install -Dm444 ${panelFirmware}/anbernic,rg35xx-sp-v2-panel.panel external-firmware/panels/anbernic,rg35xx-sp-v2-panel.panel
    '';
})
