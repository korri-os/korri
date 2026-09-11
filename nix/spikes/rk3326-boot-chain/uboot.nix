# U-Boot for RK3326 handhelds, built from the upstream Odroid Go Advance
# target.
#
# `odroid-go2_defconfig` sets `CONFIG_ROCKCHIP_PX30=y`, `CONFIG_TPL_RAM=y`,
# and `CONFIG_ROCKCHIP_SDRAM_COMMON=y`, and U-Boot carries open PX30 DRAM
# init in `drivers/ram/rockchip/sdram_px30.c` plus its PCTL/PHY helpers and
# DDR3/DDR4/LPDDR2/LPDDR3 timing tables. So U-Boot builds its own TPL and no
# `ROCKCHIP_TPL` blob is passed — unlike the RG353M and RG DS, which both
# consume `rkbin.TPL_RK3568`.
#
# There is no LPDDR4 timing table for PX30 upstream. The stock R36-class
# device tree on the unit we examined identifies itself as
# `rockchip,rk3326-evb-lp3-v12-linux` — "lp3" is LPDDR3, which is covered.
#
# The board DTB is deliberately left at the upstream Odroid Go 2 default.
# Panel, joystick, button, and Wi-Fi nodes are board facts that belong to a
# device directory, not to this boot-chain package.
{
  buildUBoot,
  lib,
  armTrustedFirmwarePX30,
}:

buildUBoot {
  defconfig = "odroid-go2_defconfig";
  BL31 = "${armTrustedFirmwarePX30}/bl31.elf";

  # PX30 gives the TPL only 0x2800 bytes of boot SRAM
  # (`CONFIG_TPL_MAX_SIZE` in `common/spl/Kconfig.tpl`). As shipped, the
  # stage measures 0x3000 and mkimage refuses it.
  #
  # Dropping the serial console out of the TPL is the cheapest 2 KiB
  # available. `CONFIG_DEBUG_UART` has to go with it: `sdram_common.c`
  # calls `printascii` unconditionally under that symbol, so leaving it on
  # with `TPL_SERIAL` off fails to link. The SPL runs from DRAM moments
  # later and keeps its console, so early output is delayed, not lost.
  extraConfig = ''
    # CONFIG_TPL_SERIAL is not set
    # CONFIG_DEBUG_UART is not set
  '';

  filesToInstall = [
    "idbloader.img"
    "u-boot.itb"
    "u-boot-rockchip.bin"
  ];

  extraMeta = {
    description = "U-Boot for Rockchip RK3326/PX30 handhelds";
    platforms = [ "aarch64-linux" ];
    license = lib.licenses.gpl2Plus;
  };
}
