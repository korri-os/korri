# Build-only R36T Max candidate: mainline SPL/U-Boot and TF-A, vendor DDR init.
# rkbin's RKBOOT/RK3326MINIALL.ini names this exact DDR file. The repository's
# flake.lock pins the nixpkgs rkbin producer; see README.md for hashes/license.
# Keep the upstream Odroid Go 2 board configuration. This is not boot proof.
{
  buildUBoot,
  lib,
  rkbin,
  armTrustedFirmwarePX30,
}:

buildUBoot {
  defconfig = "odroid-go2_defconfig";
  BL31 = "${armTrustedFirmwarePX30}/bl31.elf";
  ROCKCHIP_TPL = "${rkbin}/bin/rk33/rk3326_ddr_333MHz_v2.11.bin";

  extraPatches = [ ./px30-external-tpl.patch ];
  extraConfig = ''
    CONFIG_ROCKCHIP_EXTERNAL_TPL=y
    # CONFIG_TPL is not set
    # CONFIG_SPL_BOOTROM_SUPPORT is not set
  '';

  filesToInstall = [
    "idbloader.img"
    "u-boot.itb"
    "u-boot-rockchip.bin"
  ];

  postBuild = ''
    python3 ${./check-artifacts.py} . "$ROCKCHIP_TPL" "$BL31"
    python3 ${./test-check-artifacts.py} ${./check-artifacts.py} . "$ROCKCHIP_TPL" "$BL31"
    tools/mkimage -T rksd -l idbloader.img
    tools/dumpimage -l u-boot.itb
  '';

  postInstall = ''
    # Carry the resolved config and original notices with the mixed-license
    # output, rather than labelling the proprietary DDR bytes as GPL U-Boot.
    cp .config "$out/u-boot.config"
    cp -r Licenses "$out/Licenses"
    cp ${rkbin}/share/doc/LICENSE "$out/Licenses/rkbin-LICENSE"
    cp ${armTrustedFirmwarePX30.src}/docs/license.rst "$out/Licenses/TF-A-license.rst"
    cp -r ${armTrustedFirmwarePX30.src}/licenses "$out/Licenses/TF-A"
  '';

  extraMeta = {
    description = "Build-only RK3326/PX30 U-Boot with Rockchip DDR firmware";
    platforms = [ "aarch64-linux" ];
    license = with lib.licenses; [
      gpl2Plus
      bsd3
      unfreeRedistributableFirmware
    ];
  };
}
