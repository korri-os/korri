{
  config,
  korri,
  lib,
  pkgs,
  ...
}:

let
  system = pkgs.stdenv.hostPlatform.system;
  inputplumberData = import ../../services/inputd/nix/inputplumber-data.nix { inherit pkgs; };
  inputplumber = inputplumberData.composeResolved {
    inputplumberKorri = korri.packages.${system}.inputplumber-korri;
    additionalDataPackages = [ korri.packages.${system}.rg353m-inputplumber-data ];
  };
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
    # Bundle launch owns both the executable and XDG_DATA_DIRS. Package the
    # RG353M map into that same root so updates and reboots cannot drop it.
    initialPackage = import ../../services/inputd/nix/korri-bundle.nix {
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
      # /dev/dri/cardN is assigned in probe order, not by role. This device has
      # two DRM cards: the display controller (rockchipdrm, display-subsystem)
      # drives the panel, and the GPU (panfrost, fde60000.gpu) is render-only.
      # When panfrost probed first, card0 became the GPU, sway found no KMS
      # device, and the compositor restart-looped with the screen blank. Address
      # the display controller by its hardware path, which does not move.
      drmDevice = "/dev/dri/by-path/platform-display-subsystem-card";
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
    {
      # Guard the fix above: a bare cardN name reintroduces the probe-order race.
      assertion =
        builtins.match "/dev/dri/card[0-9]+" config.services.korriLinuxHost.compositor.drmDevice == null;
      message = "The RG353M compositor must address its KMS card by hardware path, not /dev/dri/cardN, because cardN follows probe order.";
    }
  ];
}
