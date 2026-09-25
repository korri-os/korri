{
  pkgs,
  module,
  sunshinePackage,
  sunshineV4l2m2mPackage ? null,
  sunshineRkmppPackage ? null,
  inputdPackage,
  inputplumberKorri,
  korridPackage,
  korriBundle,
}:
let
  lib = pkgs.lib;
  evaluate =
    extra:
    import "${pkgs.path}/nixos/lib/eval-config.nix" {
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        module
        {
          system.stateVersion = "26.05";
          boot.loader.grub.enable = false;
          fileSystems."/" = {
            device = "none";
            fsType = "tmpfs";
          };
          networking.hostName = "consumer";
          users.groups.korri.gid = 1000;
          users.users.korri = {
            isNormalUser = true;
            uid = 1000;
            group = "korri";
            home = "/home/korri";
          };
          services.korriBundle = {
            initialPackage = korriBundle;
            launcherPackage = inputdPackage;
          };
          services.korriLinuxInput = {
            provider.package = inputplumberKorri;
            inputd.package = inputdPackage;
          };
          services.korridLinuxDevice.package = korridPackage;
          services.korriLinuxHost = {
            enable = true;
            runtimeUser = "korri";
            runtimeUid = 1000;
            runtimeGroup = "korri";
            runtimeGid = 1000;
            firewallInterfaces = [ "tailscale0" ];
            relays = [ "wss://relay.example.com" ];
            compositor.renderDevice = "/dev/dri/renderD128";
            compositor.extraConfig = "# module-check-extra-config";
          };
        }
        extra
      ];
    };
  allAssertionsPass = system: lib.all (entry: entry.assertion) system.config.assertions;
  hasFailedAssertion =
    needle: system:
    lib.any (entry: !entry.assertion && lib.hasInfix needle entry.message) system.config.assertions;
  valid = evaluate { };
  withAudio = evaluate { services.korriLinuxHost.audio.enable = true; };
  browserPortal = evaluate {
    services.korridLinuxDevice.browser = {
      enable = true;
      origin = "http://127.0.0.1:8099";
    };
  };
  noValidation = evaluate { services.korriLinuxHost.validation.enable = false; };
  physical = evaluate {
    services.korriLinuxHost.compositor = {
      backend = "drm";
      drmDevice = "/dev/dri/card0";
      renderDevice = "/dev/dri/renderD128";
      outputName = "DSI-1";
      mode = "640x480@60Hz";
      renderer = "gles2";
      localInput.enable = true;
    };
  };
  missingDrmDevice = evaluate { services.korriLinuxHost.compositor.backend = "drm"; };
  wrongRuntimeUid = evaluate { users.users.korri.uid = lib.mkForce 1002; };
  cfg = valid.config;
  compositor = cfg.systemd.services.korri-compositor;
  korrid = cfg.systemd.services.korrid;
  inputd = cfg.systemd.services.korri-inputd;
  browserKorrid = browserPortal.config.systemd.services.korrid;
  physicalCompositor = physical.config.systemd.services.korri-compositor;
  validationAction = cfg.services.korriLinuxInput.inputd.actions.workspace-next.command;
in
assert allAssertionsPass valid;
assert cfg.services.korriBundle.enable;
assert cfg.services.korriLinuxInput.provider.enable;
assert cfg.services.korriLinuxInput.inputd.enable;
assert cfg.services.korridLinuxDevice.enable;
assert cfg.services.inputplumber.enable;
assert !cfg.services.sunshine.enable;
assert !(cfg.systemd.services ? sunshine);
assert !(cfg.systemd.services ? korri-input-seat-receiver);
assert !(cfg.systemd.services ? korri-streaming-performance-profile);
assert !(cfg.systemd.sockets ? korri-certificate-control);
assert !(cfg.users.groups ? korri-sunshine-input-seat);
assert !(cfg.users.groups ? korri-sunshine-uinput);
# The base host owns /dev/uinput through one memberless group that approved
# plugin units join.
assert cfg.users.groups ? uinput;
assert (cfg.users.groups.uinput.members or [ ]) == [ ];
assert lib.hasInfix ''OWNER="korri-inputd", GROUP="uinput", MODE="0660"'' cfg.services.udev.extraRules;
assert !(lib.hasInfix "korri-sunshine" cfg.services.udev.extraRules);
assert cfg.services.korriLinuxHost.enable;
assert cfg.services.korriLinuxHost.runtimeUser == "korri";
assert cfg.services.korriLinuxHost.runtimeUid == 1000;
assert cfg.services.korriLinuxHost.runtimeGroup == "korri";
assert cfg.services.korriLinuxHost.runtimeGid == 1000;
assert cfg.services.korriLinuxHost.relays == [ "wss://relay.example.com" ];
assert cfg.services.korridLinuxDevice.relays == [ "wss://relay.example.com" ];
assert cfg.hardware.graphics.enable;
assert compositor.serviceConfig.User == "korri";
assert compositor.serviceConfig.Group == "korri";
assert compositor.environment.WLR_BACKENDS == "headless";
assert compositor.environment.WLR_RENDERER == "gles2";
assert compositor.environment.WLR_RENDER_DRM_DEVICE == "/dev/dri/renderD128";
assert compositor.environment.SWAYSOCK == "/run/korri-compositor/sway-ipc.sock";
assert compositor.serviceConfig.RuntimeDirectory == "korri-compositor";
assert builtins.elem "user-runtime-dir@1000.service" compositor.requires;
assert builtins.elem "user@1000.service" compositor.requires;
assert builtins.elem "korrid.service" compositor.wants;
assert !(builtins.elem "sunshine.service" compositor.wants);
assert builtins.elem "korri-compositor.service" korrid.bindsTo;
assert builtins.elem "korri-compositor.service" korrid.requires;
assert builtins.elem "korri-compositor.service" korrid.after;
assert cfg.services.korridLinuxDevice.compositorControlDirectory == "/run/korri-compositor";
assert korrid.environment.KORRID_COMPOSITOR_CONTROL_DIRECTORY == "/run/korri-compositor";
assert korrid.environment.KORRID_RELAYS == ''["wss://relay.example.com"]'';
assert korrid.environment.HOSTNAME == "consumer";
assert builtins.elem "/var/lib/korrid" korrid.serviceConfig.ReadWritePaths;
assert inputd.serviceConfig.User == "korri-inputd";
assert builtins.elem 39217 cfg.networking.firewall.interfaces.tailscale0.allowedTCPPorts;
assert builtins.hasAttr "workspace-next" cfg.services.korriLinuxInput.inputd.actions;
assert validationAction == [
  "${pkgs.sway-unwrapped}/bin/swaymsg"
  "-s"
  "/run/korri-compositor/sway-ipc.sock"
  ''workspace "korri:game:active"; focus child; fullscreen enable; border none''
];
assert noValidation.config.services.korriLinuxInput.inputd.actions == { };
assert allAssertionsPass withAudio;
assert withAudio.config.services.pipewire.enable;
assert withAudio.config.services.pipewire.pulse.enable;
assert withAudio.config.services.pipewire.alsa.enable;
assert withAudio.config.services.pipewire.wireplumber.enable;
assert withAudio.config.users.users.korri.linger;
assert builtins.elem "audio" withAudio.config.users.users.korri.extraGroups;
assert allAssertionsPass browserPortal;
assert browserKorrid.environment.KORRID_BROWSER_ADDRESS == "127.0.0.1:0";
assert browserKorrid.environment.KORRID_BROWSER_ORIGIN == "http://127.0.0.1:8099";
assert browserKorrid.environment.KORRID_BROWSER_INFO_PATH == "/run/korrid-browser/brain.json";
assert builtins.elem "/run/korrid-browser" browserKorrid.serviceConfig.ReadWritePaths;
assert allAssertionsPass physical;
assert physical.config.services.seatd.enable;
assert physicalCompositor.environment.WLR_BACKENDS == "drm,libinput";
assert physicalCompositor.environment.WLR_DRM_DEVICES == "/dev/dri/card0";
assert physicalCompositor.environment.WLR_RENDER_DRM_DEVICE == "/dev/dri/renderD128";
assert builtins.elem "seat" physicalCompositor.serviceConfig.SupplementaryGroups;
assert hasFailedAssertion "DRM compositor requires" missingDrmDevice;
assert hasFailedAssertion "runtime identity" wrongRuntimeUid;
pkgs.runCommand "korri-linux-host-module-check" { } ''
  touch "$out"
''
