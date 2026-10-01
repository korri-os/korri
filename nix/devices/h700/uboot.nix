# U-Boot for the Anbernic H700 handhelds (RG35XX SP, RG35XX Pro).
# Mainline U-Boot 2025.10 with anbernic_rg35xx_h700_defconfig and ARM Trusted
# Firmware (BL31) for Allwinner H616/H700.
#
# ROCKNIX builds this one defconfig for every H700 board and keeps its default
# control tree (rg35xx-2024): U-Boot uses only the DRAM, PMIC, UART and MMC,
# which these boards share. The kernel's own tree comes from extlinux. The two
# settings below are ROCKNIX's boot settings for H700 (devices/H700/packages/
# u-boot/patches/anbernic_rg35xx_h700_defconfig.patch): no autoboot countdown,
# and no EFI loader in front of extlinux.
{
  buildUBoot,
  armTrustedFirmwareAllwinnerH616,
}:
buildUBoot {
  defconfig = "anbernic_rg35xx_h700_defconfig";
  extraMeta = {
    description = "U-Boot bootloader for the Anbernic H700 handhelds";
    platforms = [ "aarch64-linux" ];
  };
  BL31 = "${armTrustedFirmwareAllwinnerH616}/bl31.bin";
  extraConfig = ''
    CONFIG_BOOTDELAY=0
    # CONFIG_EFI_LOADER is not set
  '';
  filesToInstall = [
    "u-boot-sunxi-with-spl.bin"
  ];
}
