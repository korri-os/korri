{
  buildUBoot,
  lib,
  rkbin,
}:

buildUBoot {
  defconfig = "anbernic-rgxx3-rk3566_defconfig";
  BL31 = rkbin.BL31_RK3568;
  # RG353M uses the RK3568 DDR training blob in upstream U-Boot and in the
  # legacy ROCKNIX image that booted this device.
  ROCKCHIP_TPL = rkbin.TPL_RK3568;

  filesToInstall = [
    "idbloader.img"
    "u-boot.itb"
    "u-boot-rockchip.bin"
  ];

  extraMeta = {
    description = "U-Boot for Anbernic RG353-series RK3566 handhelds";
    platforms = [ "aarch64-linux" ];
    license = lib.licenses.gpl2Plus;
  };
}
