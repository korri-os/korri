{
  lib,
  fetchurl,
  fetchFromGitHub,
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

  # The pin ROCKNIX next uses for H700 on Linux 7.2 (packages/linux-drivers/
  # rocknix-joypad/package.mk, PKG_SHA256 89ade176...). This revision uses the
  # GPIO descriptor API, so it needs no gpiolib revert patch.
  joypad = fetchFromGitHub {
    owner = "ROCKNIX";
    repo = "rocknix-joypad";
    rev = "d02ed13aae08113f6f9e0e9d699cb29bb3450fa2";
    hash = "sha256-ad3IRojGrvP2d+C9cIfpjiEgl7GcdXsXWWbgfaVxFZM=";
  };

  # Apply config-korri.delta to the unchanged ROCKNIX configuration: replace
  # each named symbol in place and append the ones the baseline omits.
  parseSetting =
    line:
    let
      set = builtins.match "(CONFIG_[A-Za-z0-9_]+)=.*" line;
      unset = builtins.match "# (CONFIG_[A-Za-z0-9_]+) is not set" line;
    in
    if set != null then
      builtins.head set
    else if unset != null then
      builtins.head unset
    else
      null;
  settingLines = text: builtins.filter (line: parseSetting line != null) (lib.splitString "\n" text);
  deltaLines = settingLines (builtins.readFile ./config-korri.delta);
  deltaNames = map parseSetting deltaLines;
  overrides = lib.listToAttrs (map (line: lib.nameValuePair (parseSetting line) line) deltaLines);
  baseLines = lib.splitString "\n" (builtins.readFile ./config);
  baseNames = lib.genAttrs (builtins.filter (name: name != null) (map parseSetting baseLines)) (
    _: true
  );
  derivedLines =
    map (
      line:
      let
        name = parseSetting line;
      in
      if name != null && overrides ? ${name} then overrides.${name} else line
    ) baseLines
    ++ map (name: overrides.${name}) (builtins.filter (name: !(baseNames ? ${name})) deltaNames);
  configfile =
    assert lib.allUnique deltaNames;
    builtins.toFile "h700-korri-config" (lib.concatStringsSep "\n" derivedLines);

  kernel = linuxManualConfig {
    inherit version stdenv configfile;
    modDirVersion = "7.2.0";
    src = fetchurl {
      url = "https://cdn.kernel.org/pub/linux/kernel/v7.x/linux-${version}.tar.xz";
      hash = "sha256-+f7z0UwN9TgZAm9L50RZg1wqCw3L9bW72eoZ8IKUArM=";
    };
    kernelPatches = map (name: {
      inherit name;
      patch = patchDir + "/${name}";
    }) patchNames;
    allowImportFromDerivation = true;
    extraMeta = {
      description = "Linux ${version} with ROCKNIX H700 display, joypad and hardware patches for Anbernic RG35XX handhelds";
      platforms = [ "aarch64-linux" ];
      license = lib.licenses.gpl2Only;
    };
  };
in
kernel.overrideAttrs (previous: {
  postPatch = (previous.postPatch or "") + ''
    install -Dm444 ${pkgs.linux-firmware}/lib/firmware/rtl_bt/rtl8821cs_config.bin external-firmware/rtl_bt/rtl8821cs_config.bin
    install -Dm444 ${pkgs.linux-firmware}/lib/firmware/rtl_bt/rtl8821cs_fw.bin external-firmware/rtl_bt/rtl8821cs_fw.bin
    install -Dm444 ${pkgs.linux-firmware}/lib/firmware/rtw88/rtw8821c_fw.bin external-firmware/rtw88/rtw8821c_fw.bin
    install -Dm444 ${panelFirmware}/anbernic,rg35xx-plus-panel.panel external-firmware/panels/anbernic,rg35xx-plus-panel.panel
    install -Dm444 ${panelFirmware}/anbernic,rg35xx-plus-rev6-panel.panel external-firmware/panels/anbernic,rg35xx-plus-rev6-panel.panel
    install -Dm444 ${panelFirmware}/anbernic,rg35xx-sp-v2-panel.panel external-firmware/panels/anbernic,rg35xx-sp-v2-panel.panel
    # Patch 0222 declares the symbol and the Makefile line for this source.
    install -Dm644 ${joypad}/rocknix-singleadc-joypad.c drivers/input/joystick/rocknix-singleadc-joypad.c
    install -Dm644 ${joypad}/rocknix-joypad.h drivers/input/joystick/rocknix-joypad.h
  '';
})
