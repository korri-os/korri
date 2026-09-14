# Existing Korri session, not a new handheld surface or input implementation.
{
  korri,
  lib,
  pkgs,
  ...
}:
let
  system = pkgs.stdenv.hostPlatform.system;
in
{
  # Use the existing RG DS runtime account contract without a new identity scheme.
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

  # The saved console survey has no GPU driver bound. The retained kernel builds
  # Panfrost as a module and the DTS enables ff400000.gpu. Load it explicitly.
  # The hardware forces its USB module list; same-priority lists merge, retaining
  # those recovery modules rather than replacing them.
  boot.kernelModules = lib.mkForce [ "panfrost" ];

  services.korriLinuxHost = {
    enable = true;
    label = "r36tmax";
    runtimeUser = "gameplay";
    runtimeUid = 1001;
    runtimeGroup = "games";
    runtimeGid = 1001;
    # Existing Odin/RG353M loopback-test relay, not a public relay or an owner
    # binding. korrid requires a nonempty relay list even for a local session.
    relays = [ "ws://127.0.0.1:9" ];
    validation.enable = false;
    audio.enable = false;
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
      remoteInput.enable = false;
    };
    # The shared host still starts Sunshine and inputd. This is a full-stack
    # memory candidate, not a claim that those costs have been removed.
    sunshine.openFirewall = false;
  };

  # One private Chromium launcher and static-web-server. Do not also enable the
  # nginx/korri-chromium-kiosk module. Keep the launcher's existing GPU flags and
  # Chromium sandbox; no unverified --disable-gpu workaround.
  services.korriKiosk.enable = true;
}
