{ korri, pkgs, ... }:
let
  system = pkgs.stdenv.hostPlatform.system;
in
{
  # The runtime identity and PipeWire are already owned by runtime-user.nix
  # and audio/default.nix. The shared host adds only its service groups.
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
    label = "odin2portal";
    runtimeUser = "korri";
    runtimeUid = 1000;
    runtimeGroup = "korri";
    runtimeGid = 1000;
    # Reuse RG353M's explicit loopback-test relay until a real relay is
    # assigned. This does not claim federation discovery is configured.
    relays = [ "ws://127.0.0.1:9" ];
    validation.enable = false;
    compositor = {
      backend = "drm";
      drmDevice = "/dev/dri/card0";
      renderDevice = "/dev/dri/renderD128";
      outputName = "DSI-1";
      mode = "1080x1920@120Hz";
      renderer = "gles2";
      extraConfig = ''
        output DSI-1 transform 270
        input type:touch map_to_output DSI-1
        floating_maximum_size -1 x -1
        # Chromium 143 derives app_id from the bootstrap URL, ignoring --class
        # for app windows. Verified with the native Wayland launcher probe.
        for_window [app_id="^chrome-127[.]0[.]0[.]1__kiosk-blank[.]html-Default$"] fullscreen disable, floating enable, border none, resize set width 100 ppt height 100 ppt, move position 0 0
        for_window [shell="xwayland"] fullscreen disable, border none
      '';
    };
    sunshine = {
      package = korri.packages.${system}.sunshine-korri;
      capture = "kms";
      encoder = "software";
      openFirewall = false;
    };
  };
  services.korriKiosk.enable = true;
}
