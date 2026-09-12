# Korri session for the RG DS: compositor, korrid, and the portal kiosk.
#
# Connector and device names come from the running candidate, not assumption:
# /dev/dri/by-path/platform-display-subsystem-card resolves to card1
# (display-subsystem) with connectors DSI-1 and DSI-2, while the Panfrost GPU
# owns card0 and renderD128. Card numbers follow probe order, so the display
# controller is named by hardware path.
{
  config,
  korri,
  pkgs,
  ...
}:
let
  system = pkgs.stdenv.hostPlatform.system;
  inputplumberData = import ../../../services/inputd/nix/inputplumber-data.nix { inherit pkgs; };
  # InputPlumber found this board's buttons but had no profile for them, so it
  # never built a controller target and only the touchscreens worked.
  inputplumber = inputplumberData.composeResolved {
    inputplumberKorri = korri.packages.${system}.inputplumber-korri;
    additionalDataPackages = [
      (import ./inputplumber-data.nix {
        inherit pkgs;
        inputplumber = korri.packages.${system}.inputplumber-korri;
      })
    ];
  };
in
{
  imports = [
    (import ../../../clients/portal/nix/nixos-module.nix {
      inherit korri;
      # The RG353M's Chromium hits a GPU-process seccomp fault on this same
      # pinned browser. Keep software drawing for this first RG DS session and
      # retest hardware rendering on the device before removing the flag.
      chromiumArgs = [
        "--force-prefers-reduced-motion"
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

  services.korriBundle.initialPackage = import ../../../services/inputd/nix/korri-bundle.nix {
    inherit pkgs;
    inputdPackage = korri.packages.${system}.korri-inputd;
    inputplumberKorri = inputplumber;
    korridPackage = korri.packages.${system}.korrid;
  };
  services.korriBundle.launcherPackage = korri.packages.${system}.korri-inputd;
  services.korriLinuxInput = {
    provider.package = inputplumber;
    inputd.package = korri.packages.${system}.korri-inputd;
  };
  services.korridLinuxDevice.package = korri.packages.${system}.korrid;

  services.korriLinuxHost = {
    enable = true;
    label = "rgds";
    runtimeUser = "gameplay";
    runtimeUid = 1001;
    runtimeGroup = "games";
    runtimeGid = 1001;
    # The relays the other Korri devices already publish to. Peers are matched
    # by owner, so discovery still needs an owner binding on this device.
    relays = [
      "wss://relay.nostr.band"
      "wss://relay.primal.net"
    ];
    # The existing validation gate is NVIDIA-specific; audio routing on this
    # board is unverified. Both stay off until tested on the device.
    validation.enable = false;
    audio.enable = false;
    compositor = {
      backend = "drm";
      localInput.enable = true;
      drmDevice = "/dev/dri/by-path/platform-display-subsystem-card";
      renderDevice = "/dev/dri/renderD128";
      outputName = "DSI-1";
      mode = "640x480@60Hz";
      renderer = "gles2";
    };
    sunshine.openFirewall = false;
  };

  services.korri.webSurfaceHost.enable = true;
  services.korri.compositor.kiosk.enable = true;

  assertions = [
    {
      assertion =
        builtins.match "/dev/dri/card[0-9]+" config.services.korriLinuxHost.compositor.drmDevice == null;
      message = "The RG DS compositor must name its KMS card by hardware path, because card numbers follow probe order.";
    }
  ];
}
