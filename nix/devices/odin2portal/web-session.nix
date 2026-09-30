# Odin 2 Portal screen, GPU, and control facts for the shared Korri product.
{ odinRocknix, ... }:
{
  # The AYN MCU profile is observed device data. The product module owns
  # InputPlumber and inputd; this device supplies only its control map.
  services.korriLinuxInput.provider.extraDataPackages = [
    odinRocknix.inputplumberData
  ];

  # USB gadget identity. Unverified on hardware: usb_1 is dwc3 with a Type-C
  # role switch, and this kernel gained ACM on 2026-09-30. The next free /24;
  # "OD2" in ASCII after the shared 02:52 prefix.
  services.korriProduct.usbGadget = {
    name = "odin2portal";
    product = "Odin 2 Portal NixOS";
    address = "10.42.4.1";
    hostMac = "02:52:4f:44:32:01";
    deviceMac = "02:52:4f:44:32:02";
  };

  services.korriLinuxHost = {
    label = "odin2portal";
    compositor = {
      backend = "drm";
      drmDevice = "/dev/dri/card0";
      renderDevice = "/dev/dri/renderD128";
      outputName = "DSI-1";
      mode = "1080x1920@120Hz";
      renderer = "gles2";
      localInput.enable = true;
      extraConfig = ''
        output DSI-1 transform 270
        input type:touch map_to_output DSI-1
      '';
    };
  };
}
