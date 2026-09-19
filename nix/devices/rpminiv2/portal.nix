# Korri product candidate for the RP Mini V2. The console configuration in
# default.nix remains the separate recovery image.
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
  inputplumberData = import ../../../services/inputd/nix/inputplumber-data.nix { inherit pkgs; };
  inputplumber = inputplumberData.composeResolved {
    inputplumberKorri = korri.packages.${system}.inputplumber-korri;
    additionalDataPackages = [ korri.packages.${system}.rpminiv2-inputplumber-data ];
  };
  displayIdle = pkgs.writeShellScript "rpminiv2-display-idle" ''
    exec ${pkgs.swayidle}/bin/swayidle -w \
      timeout 300 '${pkgs.sway}/bin/swaymsg output DSI-1 power off' \
      resume '${pkgs.sway}/bin/swaymsg output DSI-1 power on' \
      before-sleep '${pkgs.sway}/bin/swaymsg output DSI-1 power off'
  '';
in
{
  imports = [
    ./game-plugins.nix
    (import ../../../clients/portal/nix/nixos-module.nix {
      inherit korri;
      chromiumArgs = [
        "--force-prefers-reduced-motion"
        # Keep Chromium sandboxed while the first device pass concentrates on
        # portal delivery and controller navigation. Sway still uses Adreno.
        "--disable-gpu"
      ];
    })
  ];

  users.groups.games.gid = 1001;
  users.users.gameplay = {
    isNormalUser = true;
    uid = 1001;
    group = "games";
    home = "/home/gameplay";
    createHome = true;
  };

  image.baseName = lib.mkForce "nixos-rpminiv2-korri";

  boot = {
    kernelPackages = lib.mkForce (pkgs.linuxPackagesFor rpminiKorriKernel);
    # The bounded kernel has atkbd, ctr, loop and uinput built in and no kTLS.
    # Force only the two real root-time modules instead of inheriting nginx's
    # optional `tls` request, which makeModulesClosure correctly rejects.
    kernelModules = lib.mkForce [
      "g_serial"
      "retroid"
    ];
  };
  # The recovery module forces graphics off. This product-only override is
  # deliberately stronger while leaving the recovery configuration untouched.
  hardware.graphics.enable = lib.mkOverride 40 true;

  services.korriBundle = {
    initialPackage = import ../../../services/inputd/nix/korri-bundle.nix {
      inherit pkgs;
      inputdPackage = korri.packages.${system}.korri-inputd;
      inputplumberKorri = inputplumber;
      korridPackage = korri.packages.${system}.korrid;
    };
    launcherPackage = korri.packages.${system}.korri-inputd;
  };
  services.korriLinuxInput = {
    provider.package = inputplumber;
    inputd.package = korri.packages.${system}.korri-inputd;
  };
  services.korridLinuxDevice = {
    package = korri.packages.${system}.korrid;
    address = lib.mkForce "127.0.0.1:39217";
  };

  services.korriLinuxHost = {
    enable = true;
    label = "rpminiv2";
    runtimeUser = "gameplay";
    runtimeUid = 1001;
    runtimeGroup = "games";
    runtimeGid = 1001;
    # Korrid requires a relay entry. This inert loopback endpoint keeps the
    # first milestone local and does not require a physical network driver.
    relays = [ "ws://127.0.0.1:9" ];
    validation.enable = false;
    audio.enable = false;
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
      remoteInput.enable = false;
      # Hardware-verified rotation for this portrait-native 1080x1240 panel
      # produces the handheld's 1240x1080 upright landscape layout.
      extraConfig = ''
        output DSI-1 transform 90 scale 1
      '';
    };
    sunshine.openFirewall = false;
  };

  services.korri.webSurfaceHost = {
    enable = true;
    surfaceId = "pico";
  };
  services.korri.compositor.kiosk.enable = true;

  # Protect the OLED after five graphical idle minutes. The TTY keeps its
  # separate consoleblank=60 policy. Local power/volume input can wake Sway.
  # The shared host can stream, but the first RP Mini V2 milestone cannot.
  # Remove every boot and socket activation edge while retaining the package
  # composition expected by the shared host module.
  systemd.services.sunshine = {
    enable = lib.mkForce false;
    wantedBy = lib.mkForce [ ];
  };
  systemd.sockets.korri-certificate-control = {
    enable = lib.mkForce false;
    wantedBy = lib.mkForce [ ];
  };
  systemd.services.korri-compositor.wants = lib.mkForce [ "korrid.service" ];

  systemd.services.rpminiv2-display-idle = {
    description = "RP Mini V2 OLED idle protection";
    wantedBy = [ "multi-user.target" ];
    requires = [ "korri-compositor.service" ];
    after = [ "korri-compositor.service" ];
    environment = {
      XDG_RUNTIME_DIR = "/run/user/1001";
      WAYLAND_DISPLAY = "korri-wayland";
      SWAYSOCK = "/run/korri-compositor/sway-ipc.sock";
    };
    serviceConfig = {
      User = "gameplay";
      Group = "games";
      ExecStart = displayIdle;
      Restart = "always";
      RestartSec = 1;
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
    };
  };

  # config-tty-trim intentionally omits netfilter and every physical network
  # driver. The first portal is loopback-only; do not start a firewall service
  # that this bounded kernel cannot implement.
  networking = {
    firewall.enable = false;
    networkmanager.enable = lib.mkForce false;
    useDHCP = false;
  };
  services.avahi.enable = lib.mkForce false;

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
