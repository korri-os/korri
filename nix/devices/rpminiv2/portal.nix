# RP Mini V2 hardware facts and recorded hardware limits for the shared Korri
# product. The console configuration in default.nix remains the recovery image.
{
  config,
  korri,
  lib,
  pkgs,
  rpminiKorriKernel,
  ...
}:
let
  system = pkgs.stdenv.hostPlatform.system;
  host = config.services.korriLinuxHost;
  displayIdle = pkgs.writeShellScript "rpminiv2-display-idle" ''
    exec ${pkgs.swayidle}/bin/swayidle -w \
      timeout 300 '${pkgs.sway}/bin/swaymsg output DSI-1 power off' \
      resume '${pkgs.sway}/bin/swaymsg output DSI-1 power on' \
      before-sleep '${pkgs.sway}/bin/swaymsg output DSI-1 power off'
  '';
in
{
  imports = [ ./game-plugins.nix ];

  image.baseName = lib.mkForce "nixos-rpminiv2-korri";

  boot = {
    kernelPackages = lib.mkForce (pkgs.linuxPackagesFor rpminiKorriKernel);
    # The bounded kernel has atkbd, ctr, loop and uinput built in and no TUN or
    # kTLS. Keep only the two real root-time modules.
    kernelModules = lib.mkForce [
      "g_serial"
      "retroid"
    ];
  };

  # The recovery module forces graphics off. This product-only override is
  # deliberately stronger while leaving the recovery configuration untouched.
  hardware.graphics.enable = lib.mkOverride 40 true;

  # This package contains the controller profile observed for this board. The
  # shared product owns InputPlumber and inputd themselves.
  services.korriLinuxInput.provider.extraDataPackages = [
    korri.packages.${system}.rpminiv2-inputplumber-data
  ];

  services.korriLinuxHost = {
    label = "rpminiv2";
    compositor = {
      backend = "drm";
      # The verified TTY image exposes one MSM DRM card. Stable platform links
      # and render-node timing remain part of the physical product acceptance.
      drmDevice = "/dev/dri/card0";
      renderDevice = "/dev/dri/renderD128";
      outputName = "DSI-1";
      mode = "1080x1240@60Hz";
      renderer = "gles2";
      localInput.enable = true;
      # Hardware-verified rotation for this portrait-native 1080x1240 panel
      # produces the handheld's 1240x1080 upright landscape layout.
      extraConfig = ''
        output DSI-1 transform 90 scale 1
      '';
    };
  };

  # Recorded limit: Chromium's GPU path has not been accepted on this device.
  services.korri.compositor.kiosk.extraChromiumArgs = [ "--disable-gpu" ];

  # Protect the OLED after five graphical idle minutes. The TTY keeps its
  # separate consoleblank=60 policy. Local power and volume input can wake Sway.
  systemd.services.rpminiv2-display-idle = {
    description = "RP Mini V2 OLED idle protection";
    wantedBy = [ "multi-user.target" ];
    requires = [ "korri-compositor.service" ];
    after = [ "korri-compositor.service" ];
    environment = {
      XDG_RUNTIME_DIR = "/run/user/${toString host.runtimeUid}";
      WAYLAND_DISPLAY = "korri-wayland";
      SWAYSOCK = "/run/korri-compositor/sway-ipc.sock";
    };
    serviceConfig = {
      User = host.runtimeUser;
      Group = host.runtimeGroup;
      ExecStart = displayIdle;
      Restart = "always";
      RestartSec = 1;
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
    };
  };

  assertions = [
    {
      assertion = config.boot.kernelPackages.kernel.kernelConfig == ./kernel/config-korri;
      message = "The RP Mini V2 product must use the Korri kernel profile.";
    }
    {
      assertion = lib.elem "g_serial" config.boot.kernelModules;
      message = "The RP Mini V2 product must retain USB serial recovery.";
    }
  ];
}
