# U-Boot bootloader for Anbernic RG35XXSP (Allwinner H700 / sun50i-h700).
# Uses mainline U-Boot 2025.10 with anbernic_rg35xx_h700_defconfig and
# ARM Trusted Firmware (BL31) for Allwinner H616/H700.
{
  buildUBoot,
  armTrustedFirmwareAllwinnerH616,
}:
buildUBoot {
  defconfig = "anbernic_rg35xx_h700_defconfig";
  extraMeta = {
    description = "U-Boot bootloader for the Anbernic RG35XXSP";
    platforms = [ "aarch64-linux" ];
  };
  BL31 = "${armTrustedFirmwareAllwinnerH616}/bl31.bin";
  extraConfig = ''
    CONFIG_DEFAULT_DEVICE_TREE="allwinner/sun50i-h700-anbernic-rg35xx-sp"
    CONFIG_OF_LIST="allwinner/sun50i-h700-anbernic-rg35xx-sp"
  '';
  filesToInstall = [
    "u-boot-sunxi-with-spl.bin"
  ];
}
