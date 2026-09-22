# Odin 2 Portal screen, GPU, and control facts for the shared Korri product.
{ odinRocknix, ... }:
{
  # The AYN MCU profile is observed device data. The product module owns
  # InputPlumber and inputd; this device supplies only its control map.
  services.korriLinuxInput.provider.extraDataPackages = [
    odinRocknix.inputplumberData
  ];

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
