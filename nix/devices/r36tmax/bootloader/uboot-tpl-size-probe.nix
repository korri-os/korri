# Measurement only. Not a boot artifact.
#
# The real build fails with "TPL image too big": U-Boot's open PX30 DRAM
# init overflows `CONFIG_TPL_MAX_SIZE`, which `common/spl/Kconfig.tpl` sets
# to 0x2800 (10240 bytes) for ROCKCHIP_PX30 — the tightest budget of any
# Rockchip SoC in that file.
#
# Raising the ceiling turns the linker assertion off so the TPL links and
# can be weighed. The resulting image would not fit the PX30 boot SRAM and
# must never be written to a device.
{
  buildUBoot,
  lib,
  armTrustedFirmwarePX30,
}:

buildUBoot {
  defconfig = "odroid-go2_defconfig";
  BL31 = "${armTrustedFirmwarePX30}/bl31.elf";

  extraConfig = ''
    CONFIG_TPL_MAX_SIZE=0x20000
  '';

  filesToInstall = [
    "tpl/u-boot-tpl.bin"
    "tpl/u-boot-tpl"
    "spl/u-boot-spl.bin"
  ];

  extraMeta = {
    description = "PX30 TPL size probe — oversized, never bootable";
    platforms = [ "aarch64-linux" ];
    license = lib.licenses.gpl2Plus;
  };
}
