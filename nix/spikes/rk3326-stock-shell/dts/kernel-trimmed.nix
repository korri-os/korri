# A kernel sized for this handheld, carrying the board device tree.
#
# The stock nixpkgs aarch64 configuration builds thousands of modules for
# hardware this board does not have. It took 1h46m on a GitHub runner before
# the runner was killed mid-compile, and over two hours on our own aarch64
# builder. That is not a config a 1 GB handheld wants either.
#
# ROCKNIX ships a real configuration for this exact SoC family, tested on
# shipping devices: 1902 enabled options against nixpkgs' several thousand,
# with Panfrost, Rockchip DRM and DSI, RK817, and the DWMMC controllers all
# present. Using it is the same move `nix/devices/odin2portal/kernel` already
# makes with the SM8550 configuration, for the same reason.
#
# Provenance: ROCKNIX `projects/ROCKNIX/devices/RK3326/linux/linux.aarch64.conf`,
# generated for Linux 6.12.79. Two placeholders differ, exactly as on the
# Odin: CONFIG_DEFAULT_HOSTNAME was `@DEVICENAME@`, and CONFIG_INITRAMFS_SOURCE
# was `@INITRAMFS_SOURCE@`. ROCKNIX builds its initramfs into the kernel;
# NixOS supplies its own initrd, so that one is emptied.
#
# The kernel is pinned to the 6.12 series to match what the configuration was
# generated against. linuxManualConfig takes the file verbatim, so a config
# from a different series would silently default every symbol it does not
# mention.
{
  lib,
  linuxManualConfig,
  linux_6_12,
}:

let
  dtbName = "rk3326-aislpc-r36t-max";

  kernel = linuxManualConfig {
    inherit (linux_6_12) src version modDirVersion;
    configfile = ./config;
    allowImportFromDerivation = true;

    extraMeta = {
      description = "Linux for the AISLPC R36T Max, configured from ROCKNIX's RK3326 target";
      platforms = [ "aarch64-linux" ];
      license = lib.licenses.gpl2Only;
    };
  };
in
kernel.overrideAttrs (previous: {
  postPatch = (previous.postPatch or "") + ''
    cp ${./rk3326-aislpc-r36t-max.dts} arch/arm64/boot/dts/rockchip/${dtbName}.dts
    echo 'dtb-$(CONFIG_ARCH_ROCKCHIP) += ${dtbName}.dtb' \
      >> arch/arm64/boot/dts/rockchip/Makefile
  '';

  passthru = (previous.passthru or { }) // { inherit dtbName; };
})
