# R36T Max hardware facts and recorded hardware limits.
{ ... }:
{
  # Recorded limit: the normal image has no integrated H.264 encoder. The
  # separately tested out-of-tree VEPU2 path is not part of this image.

  # The saved console survey has no GPU driver bound. The retained kernel builds
  # Panfrost as a module and the DTS enables ff400000.gpu. Load it explicitly.
  boot.kernelModules = [ "panfrost" ];

  services.korriLinuxHost = {
    label = "r36tmax";
    compositor = {
      backend = "drm";
      # Survey 20260924-134219: rockchip-drm binds display-subsystem, DSI-1 is
      # active at 720x720. Its 50 MHz / 1080 / 764 timing rounds to 61 Hz,
      # matching the retained generic DSI description's default mode.
      drmDevice = "/dev/dri/by-path/platform-display-subsystem-card";
      outputName = "DSI-1";
      mode = "720x720@61Hz";
      # Derived from the survey's ff400000.gpu platform device and the enabled
      # DTS node, not an observed render link. Device acceptance must verify it.
      renderDevice = "/dev/dri/by-path/platform-ff400000.gpu-render";
      renderer = "gles2";
      # No physical input mapping, touch transform or controller profile here.
      localInput.enable = false;
    };
  };
}
