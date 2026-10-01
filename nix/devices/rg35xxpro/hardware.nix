# RG35XX Pro hardware facts. The H700 family facts are in ../h700/board.nix.
{ ... }:
{
  networking.hostName = "rg35xxpro";

  # ROCKNIX's Pro tree (kernel patch 0223) is the Plus board with the joypad's
  # four analog-stick ADC channels enabled. It has no lid switch and no
  # external RTC. ROCKNIX ships one Pro tree; no panel variant is listed.
  hardware.deviceTree = {
    filter = "sun50i-h700-anbernic-rg35xx-pro.dtb";
    name = "allwinner/sun50i-h700-anbernic-rg35xx-pro.dtb";
  };

  # USB gadget identity. The H616 has one MUSB controller (usb@5100000), so
  # exactly one UDC is expected. The next free /24 after the RG35XX SP
  # (10.42.5); "35P" in ASCII after the shared 02:52 prefix.
  services.korriProduct.usbGadget = {
    name = "rg35xxpro";
    product = "RG35XX Pro NixOS";
    address = "10.42.6.1";
    hostMac = "02:52:33:35:50:01";
    deviceMac = "02:52:33:35:50:02";
  };

  services.korriLinuxHost.label = "rg35xxpro";

  image.baseName = "nixos-rg35xxpro";
  sdImage.rootVolumeLabel = "NIXOS_RG35XXPRO";
}
