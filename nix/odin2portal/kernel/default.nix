# Linux kernel for the AYN Odin 2 Portal (Qualcomm SM8550 / QCS8550).
#
# The device has no in-tree support. Mainline carries the SoC but not the
# board: the AYN device trees, the ICNA3512 panel, the rsinput gamepad, the
# HTR3212 LED and aw88166 codec changes, and the haptics driver all live in
# the ROCKNIX SM8550 patch queue. This package rebuilds that kernel under
# Nix with the same 7.2 source, patch order, device trees, and .config. It
# excludes one hardware-tested DPU patch as documented below. When the AYN
# series enters the upstream kernel, this package becomes smaller.
#
# Provenance: ROCKNIX distribution rev 1178bc2238de782bf081c558c177d35bb3690021
# (branch next, 2026-09-04).
#
#   patches/0000-mainline-*   projects/ROCKNIX/packages/linux/patches/mainline/
#   patches/0000-version-*    projects/ROCKNIX/packages/linux/patches/7.2/
#   patches/<rest>            projects/ROCKNIX/devices/SM8550/patches/linux/
#   dts/                      projects/ROCKNIX/devices/SM8550/linux/dts/qcom/
#   config                    projects/ROCKNIX/devices/SM8550/linux/linux.aarch64.conf
#
# The file 0000-version-0010-msm-resource-cleanup.patch.disabled is the one
# exception. With this patch, the Portal stops during early DPU startup. The
# panel stays black, the root journal does not start, and SSH does not start.
# Linux 7.2 starts with the old or new DTB after this patch is disabled. Keep
# the source here until ROCKNIX replaces or removes the patch.
#
# ROCKNIX applies patches in this order: mainline, Linux 7.2, and device.
# It sorts each directory by file name. The two 0000 prefixes keep that order
# under one sorted glob.
#
# The config differs from ROCKNIX in two placeholders only:
# CONFIG_DEFAULT_HOSTNAME (was @DEVICENAME@) and CONFIG_INITRAMFS_SOURCE
# (was @INITRAMFS_SOURCE@). ROCKNIX puts its initramfs in the kernel. The
# systemd-boot entry supplies the NixOS initrd.
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
  version = "7.2";
  patchDir = ./patches;
  brokenDpuPatchName = "0000-version-0010-msm-resource-cleanup.patch";
  patchNames =
    assert !builtins.pathExists (patchDir + "/${brokenDpuPatchName}");
    lib.sort lib.lessThan (
      builtins.filter (name: lib.hasSuffix ".patch" name) (builtins.attrNames (builtins.readDir patchDir))
    );
  dtbName = "qcs8550-ayn-odin2portal";
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
