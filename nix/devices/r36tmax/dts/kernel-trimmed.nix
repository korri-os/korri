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
# Linux 7.2.6 uses this retained configuration as its seed. Kconfig resolves
# new symbols during the build. Native and cross builds passed, and the SD
# image booted with USB console access; peripheral regression tests remain.
{
  lib,
  fetchurl,
  linuxManualConfig,
  stdenv,
  # linuxPackagesFor re-invokes this function through `override` to attach
  # kernel features; accept and ignore what it passes. Same reason as
  # nix/devices/odin2portal/kernel/default.nix.
  features ? { },
  ...
}:

let
  version = "7.2.6";
  dtbName = "rk3326-aislpc-r36t-max";

  kernel = linuxManualConfig {
    inherit version stdenv;
    modDirVersion = version;

    src = fetchurl {
      url = "https://cdn.kernel.org/pub/linux/kernel/v7.x/linux-${version}.tar.xz";
      hash = "sha256-A5rvhPKwmUrto/T8/D0C7J16m7uQIOomTEP0Rshg9gY=";
    };

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
  # The panel. Mainline's ST7703 driver carries per-panel init sequences in
  # C, and this glass needs its own: SETVCOM 0x97 and SETPOWER_EXT 0x26 0x22
  # where every variant the driver knows wants something else. The sequence
  # is the vendor's, decoded from the stock device tree and checked byte for
  # byte against the community ROCKNIX overlay. Same shape as the RG353M's
  # st7703 patch on main; a candidate for upstream once the board is.
  patches = (previous.patches or [ ]) ++ [
    ./panel-st7703-r36t-max.patch
    # Prior RK915 proof's per-host MMC quirks. The local copy retains a
    # 14-byte CIS bound because the parser reads buf[12] and buf[13].
    ../wifi/mmc-support.patch
  ];

  postPatch = (previous.postPatch or "") + ''
    cp ${./rk3326-aislpc-r36t-max.dts} arch/arm64/boot/dts/rockchip/${dtbName}.dts
    echo 'dtb-$(CONFIG_ARCH_ROCKCHIP) += ${dtbName}.dtb' \
      >> arch/arm64/boot/dts/rockchip/Makefile

    # ROCKNIX's data-driven panel driver, built in unconditionally: it is
    # the one driver known to draw on this glass. See drivers/README.md.
    cp ${./drivers/panel-generic-dsi.c} drivers/gpu/drm/panel/panel-generic-dsi.c
    echo 'obj-y += panel-generic-dsi.o' >> drivers/gpu/drm/panel/Makefile
  '';

  passthru = (previous.passthru or { }) // {
    inherit dtbName;
  };
})
