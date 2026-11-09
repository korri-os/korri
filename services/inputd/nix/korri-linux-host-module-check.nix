{
  pkgs,
  module,
  sunshinePlugin,
  inputdPackage,
  inputplumberKorri,
  korridPackage,
  korriBundle,
}:
let
  lib = pkgs.lib;
  # The server part alone (ADR 0051): no local controller capture.
  evaluateServer =
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
  # A device people hold also captures its own controllers.
  evaluate =
    extra:
    evaluateServer {
      imports = [ extra ];
      services.korriLinuxInput = {
        provider.enable = true;
        inputd.enable = true;
      };
    };
  allAssertionsPass = system: lib.all (entry: entry.assertion) system.config.assertions;
  hasFailedAssertion =
    needle: system:
    lib.any (entry: !entry.assertion && lib.hasInfix needle entry.message) system.config.assertions;
  valid = evaluate { };
  serverOnly = evaluateServer { };
  serverCfg = serverOnly.config;
  withAudio = evaluate { services.korriLinuxHost.audio.enable = true; };
  noValidation = evaluate { services.korriLinuxHost.validation.enable = false; };
  unwatched = evaluate {
    services.korriLinuxHost.compositor.displays = lib.mkForce [
      {
        name = "HEADLESS-1";
        mode = "1920x1080@120Hz";
        unwatched = true;
        x = 0;
        y = 0;
        touch = null;
      }
    ];
  };
  physical = evaluate {
    services.korriLinuxHost.compositor = {
      backend = "drm";
      drmDevice = "/dev/dri/card0";
      renderDevice = "/dev/dri/renderD128";
      mainDisplay = "DSI-1";
      displays = [
        {
          name = "DSI-1";
          mode = "640x480@60Hz";
          x = 0;
          y = 0;
          touch = null;
        }
      ];
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
  receiver = cfg.systemd.services.korri-input-seat-receiver;
  seatRules = import ./input-seat-rules.nix {
    inherit pkgs;
    eventGid = 1000;
  };
  physicalCompositor = physical.config.systemd.services.korri-compositor;
  validationAction = cfg.services.korriLinuxInput.inputd.actions.workspace-next.command;
in
assert allAssertionsPass valid;
# The server part alone: no local capture, but streamed controllers still
# get their seats and the uinput access the stream plugin needs.
assert allAssertionsPass serverOnly;
assert !serverCfg.services.inputplumber.enable;
assert !(serverCfg.systemd.services ? korri-inputd);
assert serverCfg.systemd.services ? korri-input-seat-receiver;
assert serverCfg.users.groups ? uinput;
# The bundle still carries this device's InputPlumber data, whose Korri
# profile the bundle format requires, although the provider does not run.
assert serverCfg.services.korriBundle.carriesDeviceInputPlumber;
assert toString serverCfg.services.inputplumber.package == toString cfg.services.inputplumber.package;
# Plugin units such as Sunshine's require the source guard on every host.
assert serverCfg.systemd.services ? korri-input-source-guard;
assert builtins.elem "multi-user.target" serverCfg.systemd.services.korri-input-source-guard.wantedBy;
assert builtins.elem "uhid" serverCfg.boot.kernelModules;
assert lib.hasInfix ''OWNER="root", GROUP="uinput", MODE="0660"'' serverCfg.services.udev.extraRules;
assert !(lib.hasInfix ''GROUP="uinput", MODE="0660"'' (
  lib.replaceStrings [ ''OWNER="korri-inputd", GROUP="uinput", MODE="0660"'' ] [ "" ] cfg.services.udev.extraRules
));
assert cfg.services.korriBundle.enable;
assert cfg.services.korriLinuxInput.provider.enable;
assert cfg.services.korriLinuxInput.inputd.enable;
assert cfg.services.korridLinuxDevice.enable;
assert cfg.services.inputplumber.enable;
assert !cfg.services.sunshine.enable;
assert !(cfg.systemd.services ? sunshine);
assert cfg.systemd.services ? korri-input-seat-receiver;
assert builtins.elem "multi-user.target" receiver.wantedBy;
assert builtins.elem "korri-bundle-selector.service" receiver.requires;
assert builtins.elem "korri-bundle-selector.service" receiver.after;
assert builtins.elem "korrid.service" receiver.before;
assert !(builtins.elem "korrid.service" receiver.requires);
assert !(builtins.elem "korrid.service" receiver.after);
assert builtins.elem "korri-input-seat-receiver.service" korrid.requires;
assert builtins.elem "korri-input-seat-receiver.service" korrid.after;
assert receiver.serviceConfig.Type == "notify";
assert receiver.serviceConfig.User == "root";
assert receiver.serviceConfig.Group == "root";
assert receiver.serviceConfig.CapabilityBoundingSet == [ "CAP_CHOWN" ];
assert receiver.serviceConfig.DeviceAllow == [ "/dev/uinput rw" ];
assert receiver.serviceConfig.RuntimeDirectory == "korri-input-seat";
assert receiver.serviceConfig.RuntimeDirectoryMode == "0711";
assert receiver.serviceConfig.SupplementaryGroups == [ "uinput" ];
assert receiver.environment.KORRI_BUNDLE_ACTIVE == cfg.services.korriBundle.activePath;
assert
  receiver.serviceConfig.ExecStart
  == "${inputdPackage}/bin/korri-bundle-launch input-seat-receiver --runtime-dir /run/korri-input-seat --control-uid 976 --control-gid 976 --sunshine-uid 1000 --sunshine-gid 980 --event-gid 1000";
assert !(receiver.serviceConfig ? ExecStartPre);
assert !(receiver.serviceConfig ? ExecStopPost);
assert builtins.elem seatRules cfg.services.udev.packages;
assert physical.config.systemd.services ? korri-input-seat-receiver;
assert builtins.elem "korri-input-seat-receiver.service"
  physical.config.systemd.services.korrid.requires;
assert !(cfg.systemd.services ? korri-streaming-performance-profile);
assert !(cfg.systemd.sockets ? korri-certificate-control);
assert !(cfg.users.groups ? korri-sunshine-input-seat);
assert !(cfg.users.groups ? korri-sunshine-uinput);
# The base host owns /dev/uinput through one memberless group that approved
# plugin units join.
assert cfg.users.groups ? uinput;
assert (cfg.users.groups.uinput.members or [ ]) == [ ];
assert lib.hasInfix ''OWNER="korri-inputd", GROUP="uinput", MODE="0660"''
  cfg.services.udev.extraRules;
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
assert (builtins.head (builtins.fromJSON korrid.environment.KORRID_DISPLAYS)).unwatched == false;
assert (builtins.head (builtins.fromJSON korrid.environment.KORRID_DISPLAYS)).mode == "1920x1080@60Hz";
assert (builtins.fromJSON unwatched.config.systemd.services.korrid.environment.KORRID_DISPLAYS) == [
  {
    name = "HEADLESS-1";
    main = true;
    unwatched = true;
    mode = "1920x1080@120Hz";
    x = 0;
    y = 0;
    width = 1920;
    height = 1080;
    touch = null;
  }
];
assert cfg.services.korridLinuxDevice.compositorControlDirectory == "/run/korri-compositor";
assert korrid.environment.KORRID_COMPOSITOR_CONTROL_DIRECTORY == "/run/korri-compositor";
assert korrid.environment.KORRID_COMPOSITOR_WAYLAND_SOCKET == "/run/user/1000/korri-wayland";
assert korrid.environment.KORRID_RELAYS == ''["wss://relay.example.com"]'';
assert korrid.environment.HOSTNAME == "consumer";
assert builtins.elem "/var/lib/korrid" korrid.serviceConfig.ReadWritePaths;
assert inputd.serviceConfig.User == "korri-inputd";
assert builtins.elem 39217 cfg.networking.firewall.allowedTCPPorts;
assert cfg.services.korridLinuxDevice.address == "[::]:39217";
assert builtins.hasAttr "workspace-next" cfg.services.korriLinuxInput.inputd.actions;
assert
  validationAction == [
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
# Games reach PipeWire only through the Pulse socket korrid binds into them.
assert lib.hasInfix ''PULSE_SERVER = "unix:/run/korri-game-audio/native"'' (
  builtins.readFile withAudio.config.services.korridLinuxDevice.deviceConfig
);
assert
  !(lib.hasInfix "PULSE_SERVER" (
    builtins.readFile valid.config.services.korridLinuxDevice.deviceConfig
  ));
assert allAssertionsPass physical;
assert physical.config.services.seatd.enable;
assert physicalCompositor.environment.WLR_BACKENDS == "drm,libinput";
assert physicalCompositor.environment.WLR_DRM_DEVICES == "/dev/dri/card0";
assert physicalCompositor.environment.WLR_RENDER_DRM_DEVICE == "/dev/dri/renderD128";
assert builtins.elem "seat" physicalCompositor.serviceConfig.SupplementaryGroups;
assert hasFailedAssertion "DRM compositor requires" missingDrmDevice;
assert hasFailedAssertion "runtime identity" wrongRuntimeUid;
pkgs.runCommand "korri-linux-host-module-check"
  {
    nativeBuildInputs = [
      pkgs.systemd
      pkgs.python3
    ];
    passthru.seatVmTest = import ./input-seat-core-vm-test.nix {
      inherit
        pkgs
        inputdPackage
        receiver
        seatRules
        sunshinePlugin
        ;
      selector = cfg.systemd.services.korri-bundle-selector;
    };
  }
  ''
      udevadm verify --resolve-names=never --no-style ${seatRules}/lib/udev/rules.d/99-z-korri-input-seat.rules
      python3 - ${seatRules}/lib/udev/rules.d/99-z-korri-input-seat.rules <<'PY'
    import re, sys
    rules = open(sys.argv[1]).read().splitlines()
    assert len(rules) == 255
    for slot, rule in enumerate(rules, 1):
        assert 'KERNEL=="event*|js*"' in rule
        assert f'ATTRS{{name}}=="Korri Seat P{slot}"' in rule
        assert f'ATTRS{{phys}}=="korri/input-seat/p{slot}"' in rule
        for key, value in [('bustype', '0003'), ('vendor', '045e'), ('product', '028e'), ('version', '0114')]:
            assert f'ATTRS{{id/{key}}}=="{value}"' in rule
        assert 'GROUP=' not in rule
        assert 'TAG-="uaccess"' in rule
        assert '/bin/chgrp 1000 $env{DEVNAME}' in rule
        assert '/bin/chmod 0660 $env{DEVNAME}' in rule
    assert set(re.findall(r'Korri Seat P([^"\n]+)', '\n'.join(rules))) == {str(n) for n in range(1, 256)}
    PY
      touch "$out"
  ''
