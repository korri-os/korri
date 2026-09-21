# RG353M hardware facts and recorded hardware limits.
{
  config,
  korri,
  pkgs,
  ...
}:
let
  system = pkgs.stdenv.hostPlatform.system;
in
{
  # This package contains the controller profile observed for this board. The
  # product module owns InputPlumber and inputd themselves.
  services.korriLinuxInput.provider.extraDataPackages = [
    korri.packages.${system}.rg353m-inputplumber-data
  ];

  services.korriLinuxHost = {
    label = "haku";

    deviceConfig = pkgs.writeText "korrid-rg353m-host.toml" ''
      label = "haku"
      [environment]
      DISPLAY = ":0"
      XDG_SESSION_TYPE = "x11"
      PULSE_SERVER = "unix:/run/korri-game-audio/native"
    '';

    compositor = {
      backend = "drm";
      localInput.enable = true;
      # /dev/dri/cardN is assigned in probe order, not by role. This device has
      # two DRM cards: rockchipdrm drives the panel and Panfrost is render-only.
      drmDevice = "/dev/dri/by-path/platform-display-subsystem-card";
      renderDevice = "/dev/dri/renderD128";
      outputName = "DSI-1";
      mode = "640x480@60Hz";
      renderer = "gles2";
    };
  };

  # Recorded limit: Chromium's GPU path has not been accepted on the RG353M.
  # Keep software rendering local to this device rather than product policy.
  services.korri.compositor.kiosk.extraChromiumArgs = [ "--disable-gpu" ];

  # Native device testing crossed 68 C at the previous 1.8 GHz ceiling. The
  # 1.104 GHz ceiling sustained GBA below 60 C with schedutil.
  powerManagement = {
    cpuFreqGovernor = "schedutil";
    cpufreq.max = 1104000;
  };
  # RTL8821CS deep power saving caused pairing failures and dropped links.
  # Keep this as NetworkManager hardware configuration, not executable profile
  # hook source owned by the product gate.
  networking.networkmanager.wifi.powersave = false;
  boot.kernelParams = [ "video=DSI-1:640x480@60" ];

  assertions = [
    {
      assertion =
        builtins.match "/dev/dri/card[0-9]+" config.services.korriLinuxHost.compositor.drmDevice == null;
      message = "The RG353M compositor must address its KMS card by hardware path, not /dev/dri/cardN, because cardN follows probe order.";
    }
  ];
}
