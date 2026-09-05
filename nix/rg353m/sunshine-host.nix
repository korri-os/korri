{
  config,
  korri,
  lib,
  pkgs,
  ...
}:

let
  system = pkgs.stdenv.hostPlatform.system;
in
{
  # Keep this device profile thin. Korri's shared host module owns Sunshine,
  # compositor, input, identity, certificate, state, and service policy.
  # Keep the deployed identity until the separate device-backed ownership
  # cutover can preserve Sunshine state and the rollback generation.
  users.groups.games.gid = 1001;
  users.users.gameplay = {
    isNormalUser = true;
    uid = 1001;
    group = "games";
    home = "/home/gameplay";
    createHome = true;
  };

  services.korriBundle = {
    initialPackage = korri.packages.${system}.korri-bundle;
    launcherPackage = korri.packages.${system}.korri-inputd;
  };
  services.korriLinuxInput = {
    provider.package = korri.packages.${system}.inputplumber-korri;
    inputd.package = korri.packages.${system}.korri-inputd;
  };
  services.korridLinuxDevice.package = korri.packages.${system}.korrid;

  services.korriLinuxHost = {
    enable = true;
    label = "rg353m";
    runtimeUser = "gameplay";
    runtimeUid = 1001;
    runtimeGroup = "games";
    runtimeGid = 1001;
    # This first host slice has no production federation relay. Keep the
    # existing module's explicit loopback-test contract until one is assigned.
    relays = [ "ws://127.0.0.1:9" ];
    # The current deployment gate is intentionally NVIDIA-specific. Keep its
    # validation game disabled until that existing gate gains an ARM profile.
    validation.enable = false;

    compositor = {
      backend = "drm";
      drmDevice = "/dev/dri/card0";
      renderDevice = "/dev/dri/renderD128";
      outputName = "DSI-1";
      mode = "640x480@60Hz";
      renderer = "gles2";
    };

    sunshine = {
      package = korri.packages.${system}.sunshine-korri;
      capture = "kms";
      encoder = "software";
      openFirewall = false;
    };
  };

  # The complete Sunshine path sustained about 36 fps at native resolution.
  # schedutil completed the sustained CPU tests; ondemand reached 93.75 C and
  # rebooted the device.
  powerManagement.cpuFreqGovernor = "schedutil";
  boot.kernelParams = [ "video=DSI-1:640x480@60" ];

  # Keep Sunshine's administrative UI (TCP 47990) off the LAN. Pairing and
  # administration use an SSH tunnel. Expose only discovery and stream ports
  # on the two usable network paths.
  networking.firewall.interfaces =
    lib.genAttrs
      [
        "enu1"
        "wlan0"
      ]
      (_: {
        allowedTCPPorts = [
          47984
          47989
          48010
        ];
        allowedUDPPorts = [
          5353
          47998
          47999
          48000
          48002
          48010
        ];
      });

  assertions = [
    {
      assertion = config.services.sunshine.package == korri.packages.${system}.sunshine-korri;
      message = "The RG353M host must use the approved sunshine-korri package.";
    }
  ];
}
