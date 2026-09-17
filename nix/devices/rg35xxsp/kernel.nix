# Kernel configuration for Anbernic RG35XXSP (Allwinner H700 / sun50i-h700).
# Uses mainline Linux 6.12 LTS from nixpkgs, which includes the upstream
# device tree allwinner/sun50i-h700-anbernic-rg35xx-sp.dtb, built-in sunxi-mmc,
# built-in AXP20x/AXP717 regulator support, and Panfrost GPU modules.
{
  linuxPackages,
  ...
}:
linuxPackages.kernel
