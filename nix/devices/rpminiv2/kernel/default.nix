# ROCKNIX distribution e81d1fc943458fb13cffe1646761e9452b29ddc1:
# projects/ROCKNIX/packages/linux/package.mk selects Linux 7.2 for SM8250.
# See README.md for the source paths, patch order and hardware-tested config baseline.
{
  lib,
  fetchurl,
  linuxManualConfig,
  rpminiFirmware,
  rpminiRocknixBaseline,
  stdenv,
  # linuxPackagesFor supplies features when it re-invokes this function.
  features ? { },
  ...
}:
let
  version = "7.2";
  patchDir = ./patches;
  patchNames = lib.sort lib.lessThan (
    builtins.filter (name: lib.hasSuffix ".patch" name) (builtins.attrNames (builtins.readDir patchDir))
  );
  dtbName = "sm8250-retroidpocket-rpminiv2";
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
      description = "Linux ${version} with the ROCKNIX SM8250 baseline for Retroid Pocket Mini V2";
      platforms = [ "aarch64-linux" ];
      license = lib.licenses.gpl2Only;
    };
  };
in
kernel.overrideAttrs (previous: {
  # V2 includes the Mini DTS, which includes the shared Retroid DTSI.
  # ROCKNIX copies these after patching; preserve that sequence.
  postPatch = (previous.postPatch or "") + ''
    cp ${./dts}/sm8250-retroidpocket-common.dtsi arch/arm64/boot/dts/qcom/
    cp ${./dts}/sm8250-retroidpocket-rpmini.dts arch/arm64/boot/dts/qcom/
    cp ${./dts}/${dtbName}.dts arch/arm64/boot/dts/qcom/
    echo 'dtb-$(CONFIG_ARCH_QCOM) += ${dtbName}.dtb' >> arch/arm64/boot/dts/qcom/Makefile
    ln -s ${rpminiFirmware}/lib/firmware external-firmware
  '';
  # Hardware testing isolated the remaining display failure to the Nix-built
  # kernel binary. Keep building modules from the audited source/config, but
  # boot the exact ROCKNIX 20260901 Image and DTB that produced a native TTY on
  # this Mini V2 with those modules and the NixOS userspace.
  postInstall = (previous.postInstall or "") + ''
    rm -f "$out/Image" "$out/dtbs/qcom/${dtbName}.dtb"
    install -m 0444 ${rpminiRocknixBaseline}/KERNEL "$out/Image"
    install -m 0444 \
      ${rpminiRocknixBaseline}/${dtbName}.dtb \
      "$out/dtbs/qcom/${dtbName}.dtb"
  '';
  passthru = (previous.passthru or { }) // {
    inherit dtbName;
    rocknixBaseline = rpminiRocknixBaseline;
    usesRocknixBootImage = true;
  };
})
