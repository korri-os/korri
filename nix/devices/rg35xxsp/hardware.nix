# RG35XX SP hardware facts. The H700 family facts are in ../h700/board.nix.
{ ... }:
{
  networking.hostName = "rg35xxsp";

  # The SP tree adds the PE7 lid switch (SW_LID) and an NXP PCF8563 RTC to
  # the Plus board. With no declared sleep state, logind powers off on lid
  # close (product default). Recorded limit: this kernel configuration does
  # not build the PCF8563 driver, so the board clock is the SoC RTC only.
  hardware.deviceTree = {
    filter = "sun50i-h700-anbernic-rg35xx-sp.dtb";
    name = "allwinner/sun50i-h700-anbernic-rg35xx-sp.dtb";
  };

  # USB gadget identity. The H616 has one MUSB controller (usb@5100000), so
  # exactly one UDC is expected. The next free /24 after the Odin 2 Portal
  # (10.42.4); "35S" in ASCII after the shared 02:52 prefix.
  services.korriProduct.usbGadget = {
    name = "rg35xxsp";
    product = "RG35XX SP NixOS";
    address = "10.42.5.1";
    hostMac = "02:52:33:35:53:01";
    deviceMac = "02:52:33:35:53:02";
  };

  services.korriLinuxHost.label = "rg35xxsp";

  image.baseName = "nixos-rg35xxsp";
  sdImage.rootVolumeLabel = "NIXOS_RG35XXSP";
}
