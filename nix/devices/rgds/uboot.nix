# Board source: U-Boot v2026.10-rc4, configs/anbernic-rg-ds-rk3568_defconfig.
# The RK3568 firmware inputs use the same rkbin contract as the RG353M.
{
  buildUBoot,
  fetchurl,
  rkbin,
}:
buildUBoot {
  version = "2026.10-rc4";
  src = fetchurl {
    url = "https://codeload.github.com/u-boot/u-boot/tar.gz/refs/tags/v2026.10-rc4";
    name = "u-boot-2026.10-rc4.tar.gz";
    hash = "sha256-sMb/PMIpz2PmnpGzcTZELuJcxucFNjjSThAMkJstG4k=";
  };
  defconfig = "anbernic-rg-ds-rk3568_defconfig";
  BL31 = rkbin.BL31_RK3568;
  ROCKCHIP_TPL = rkbin.TPL_RK3568;
  filesToInstall = [
    "idbloader.img"
    "u-boot.itb"
    "u-boot-rockchip.bin"
  ];
  # Exercise the resolved Kconfig rather than assuming the board defaults
  # retain the MBR/ext4/extlinux path used by the SD image builder.
  postBuild = ''
    for setting in CONFIG_DOS_PARTITION=y CONFIG_FS_EXT4=y CONFIG_BOOTMETH_EXTLINUX=y CONFIG_ENV_IS_NOWHERE=y; do
      grep -qx "$setting" .config || { echo "Missing SD boot requirement: $setting" >&2; exit 1; }
    done
    test -s u-boot-rockchip.bin
  '';
  extraMeta = {
    description = "SD boot loader for the Anbernic RG DS";
    platforms = [ "aarch64-linux" ];
  };
}
