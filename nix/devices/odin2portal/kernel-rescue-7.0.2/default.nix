# Linux kernel for the AYN Odin 2 Portal (Qualcomm SM8550 / QCS8550).
#
# The device has no in-tree support. Mainline carries the SoC but not the
# board: the AYN device trees, the ICNA3512 panel, the rsinput gamepad, the
# HTR3212 LED and aw88166 codec changes, and the haptics driver all live in
# the ROCKNIX SM8550 patch queue. This package rebuilds that kernel under
# Nix with the same 7.0.2 source, the same patch order, the same device
# trees, and the same .config. Nothing here is a local improvement; when the
# AYN series lands upstream this package shrinks.
#
# Provenance: ROCKNIX distribution rev f080b462f54b5807bdd16ac7cc2ab64528b038b1
# (branch next, 2026-05-13), the revision pinned by nix-on-rocks
# upstream.lock and running on sobo today.
#
#   patches/0000-mainline-*   projects/ROCKNIX/packages/linux/patches/mainline/
#   patches/<rest>            projects/ROCKNIX/devices/SM8550/patches/linux/
#   dts/                      projects/ROCKNIX/devices/SM8550/linux/dts/qcom/
#   config                    projects/ROCKNIX/devices/SM8550/linux/linux.aarch64.conf
#
# ROCKNIX applies patches in the order mainline, then device, sorted by
# filename within each directory. The 0000-mainline- prefix keeps that order
# under a single sorted glob.
#
# The config differs from ROCKNIX in two placeholders only:
# CONFIG_DEFAULT_HOSTNAME (was @DEVICENAME@) and CONFIG_INITRAMFS_SOURCE
# (was @INITRAMFS_SOURCE@; ROCKNIX embeds its initramfs in the kernel, NixOS
# supplies its own through the Android boot image).
{
  lib,
  fetchurl,
  linuxManualConfig,
  stdenv,
  # linuxPackagesFor re-invokes this function through `override` to attach
  # kernel features; accept and ignore what it passes.
  features ? { },
  ...
}:

let
  version = "7.0.2";
  patchDir = ./patches;
  patchNames = lib.sort lib.lessThan (
    builtins.filter (name: lib.hasSuffix ".patch" name) (builtins.attrNames (builtins.readDir patchDir))
  );
  dtbName = "qcs8550-ayn-odin2portal";
  kernel = linuxManualConfig {
    inherit version stdenv;
    modDirVersion = version;

    src = fetchurl {
      url = "https://cdn.kernel.org/pub/linux/kernel/v7.x/linux-${version}.tar.xz";
      hash = "sha256-U1kaAylFJ6SMywueVZ6SLfijhVR0WhIGgnynUdLKdmI=";
    };

    kernelPatches = map (name: {
      inherit name;
      patch = patchDir + "/${name}";
    }) patchNames;

    configfile = ./config;
    allowImportFromDerivation = true;

    extraMeta = {
      description = "Linux ${version} with the ROCKNIX SM8550 patch queue for the AYN Odin 2 Portal";
      platforms = [ "aarch64-linux" ];
      license = lib.licenses.gpl2Only;
    };
  };
in
kernel.overrideAttrs (previous: {
  # ROCKNIX copies the device DTS tree over arch/arm64/boot/dts after patching
  # (post_patch in packages/linux/package.mk) and builds only the per-device
  # DTBs listed in config.xml. Copy the two AYN files this board needs and
  # register the Portal DTB so the standard `dtbs` target produces it.
  postPatch = (previous.postPatch or "") + ''
    cp ${./dts}/qcs8550-ayn-common.dtsi arch/arm64/boot/dts/qcom/
    cp ${./dts}/${dtbName}.dts arch/arm64/boot/dts/qcom/
    echo 'dtb-$(CONFIG_ARCH_QCOM) += ${dtbName}.dtb' >> arch/arm64/boot/dts/qcom/Makefile
  '';

  passthru = (previous.passthru or { }) // {
    inherit dtbName;
  };
})
