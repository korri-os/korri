# BL31 for RK3326 handhelds.
#
# RK3326 is the consumer sibling of PX30 and shares its TRM, so upstream
# Trusted Firmware-A supports it as `PLAT=px30`. nixpkgs exports no
# `armTrustedFirmwarePX30` attribute, but it does export the builder itself
# through all-packages, so this needs no nixpkgs patch.
#
# Contrast with the RG353M and RG DS: those consume a prebuilt Rockchip
# `rkbin.BL31_RK3568` blob because nixpkgs carries no RK3568 TF-A platform
# path for them. This BL31 remains source-built; uboot.nix uses vendor DDR
# firmware for the earlier stage.
{
  buildArmTrustedFirmware,
  lib,
}:

buildArmTrustedFirmware rec {
  platform = "px30";
  extraMakeFlags = [ "bl31" ];
  filesToInstall = [ "build/${platform}/release/bl31/bl31.elf" ];

  extraMeta = {
    description = "Trusted Firmware-A BL31 for Rockchip PX30/RK3326";
    platforms = [ "aarch64-linux" ];
    license = lib.licenses.bsd3;
  };
}
