{
  pkgs,
  module,
  sunshinePackage,
  inputdPackage,
  inputplumberKorri,
  korridPackage,
  korriBundle,
}:
let
  lib = pkgs.lib;
  isX86_64 = pkgs.stdenv.hostPlatform.isx86_64;
  peerPublicKey = builtins.concatStringsSep "" (pkgs.lib.replicate 64 "3");
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
            nativePeers = [
              {
                label = "zao";
                baseUrl = "http://zao:43117";
                devicePublicKey = peerPublicKey;
                moonlightAddress = "zao:47989";
              }
            ];
            sunshine.package = sunshinePackage;
            compositor.renderDevice = "/dev/dri/renderD128";
          };
        }
        extra
      ];
    };
  allAssertionsPass =
    system:
    lib.all (
      entry: if entry.assertion then true else builtins.trace entry.message false
    ) system.config.assertions;
  hasFailedAssertion =
    needle: system:
    lib.any (entry: !entry.assertion && lib.hasInfix needle entry.message) system.config.assertions;
  evaluationRejected =
    system: !(builtins.tryEval system.config.system.build.toplevel.drvPath).success;
  valid = evaluate { };
  noValidation = evaluate {
    services.korriLinuxHost.validation.enable = false;
  };
  noRuntimeSettings = evaluate {
    services.korriLinuxHost.sunshine.runtimeSettings.enable = false;
  };
  inputSeats = evaluate {
    services.korriLinuxHost.sunshine.inputSeats.enable = true;
  };
  nvenc = evaluate {
    services.korriLinuxHost.sunshine.encoder = "nvenc";
  };
  highRefresh = evaluate {
    services.korriLinuxHost.compositor.mode = "1920x1080@120Hz";
  };
  pixman = evaluate {
    services.korriLinuxHost.compositor.renderer = "pixman";
  };
  vaapi = evaluate {
    services.korriLinuxHost.sunshine.encoder = "vaapi";
  };
  physicalSoftware = evaluate {
    services.korriLinuxHost = {
      compositor = {
        backend = "drm";
        drmDevice = "/dev/dri/card0";
        renderDevice = "/dev/dri/renderD128";
        outputName = "DSI-1";
        mode = "640x480@60Hz";
        renderer = "gles2";
      };
      sunshine = {
        capture = "kms";
        encoder = "software";
      };
    };
  };
  missingDrmDevice = evaluate {
    services.korriLinuxHost.compositor.backend = "drm";
  };
  kmsWithoutDrm = evaluate {
    services.korriLinuxHost.sunshine.capture = "kms";
  };
  wrongRuntimeUid = evaluate {
    users.users.korri.uid = lib.mkForce 1002;
  };
  stockSunshine = evaluate {
    services.korriLinuxHost.sunshine.package = lib.mkForce pkgs.sunshine;
  };
  lookalikeSunshine = evaluate {
    services.korriLinuxHost.sunshine.package = lib.mkForce (
      pkgs.sunshine.overrideAttrs (_: {
        pname = "sunshine-korri";
      })
    );
  };
  strippedApprovedSunshine = evaluate {
    services.korriLinuxHost.sunshine.package = lib.mkForce (
      sunshinePackage.overrideAttrs (_: {
        patches = [ ];
      })
    );
  };
  collidingIdentity = evaluate {
    services.korriLinuxHost.serviceIdentities.inputdUid = lib.mkForce 1000;
  };
  invalidLabel = evaluate {
    services.korriLinuxHost.label = "bad label";
  };
  wrongCertificateSocketGroup = evaluate {
    systemd.sockets.korri-certificate-control.socketConfig.SocketGroup = lib.mkForce "korri";
  };
  cfg = valid.config;
  inputd = cfg.systemd.services.korri-inputd;
  korrid = cfg.systemd.services.korrid;
  sunshine = cfg.systemd.services.sunshine;
  certificateSocket = cfg.systemd.sockets.korri-certificate-control;
  compositor = cfg.systemd.services.korri-compositor;
  inputSeatReceiver = inputSeats.config.systemd.services.korri-input-seat-receiver;
  inputSeatKorrid = inputSeats.config.systemd.services.korrid;
  inputSeatSunshine = inputSeats.config.systemd.services.sunshine;
  nvencCompositor = nvenc.config.systemd.services.korri-compositor;
  vaapiCompositor = vaapi.config.systemd.services.korri-compositor;
  physicalSoftwareCompositor = physicalSoftware.config.systemd.services.korri-compositor;
  physicalSoftwareSunshine = physicalSoftware.config.systemd.services.sunshine;
  physicalSoftwareReadiness = builtins.readFile (
    builtins.elemAt physicalSoftwareCompositor.serviceConfig.ExecStartPost 1
  );
  deviceConfig = cfg.services.korridLinuxDevice.deviceConfig;
  nvencDeviceConfig = nvenc.config.services.korridLinuxDevice.deviceConfig;
  highRefreshDeviceConfig = highRefresh.config.services.korridLinuxDevice.deviceConfig;
  pixmanDeviceConfig = pixman.config.services.korridLinuxDevice.deviceConfig;
  sunshineExec = pkgs.writeText "korri-linux-host-sunshine-exec" sunshine.serviceConfig.ExecStart;
  compositorExec = pkgs.writeText "korri-linux-host-compositor-exec" compositor.serviceConfig.ExecStart;
  highRefreshCompositorExec = pkgs.writeText "korri-linux-host-high-refresh-compositor-exec" highRefresh.config.systemd.services.korri-compositor.serviceConfig.ExecStart;
  highRefreshPerformance =
    if isX86_64 then highRefresh.config.systemd.services.korri-streaming-performance-profile else null;
  highRefreshPerformanceExec = pkgs.writeText "korri-linux-host-high-refresh-performance-exec" (
    if isX86_64 then highRefreshPerformance.serviceConfig.ExecStart else ""
  );
  highRefreshPerformanceStop = pkgs.writeText "korri-linux-host-high-refresh-performance-stop" (
    if isX86_64 then highRefreshPerformance.serviceConfig.ExecStop else ""
  );
  validationAction = cfg.services.korriLinuxInput.inputd.actions.workspace-next.command;
  udevRules = pkgs.writeText "korri-linux-host-udev-rules" cfg.services.udev.extraRules;
  inputSeatUdevRules = pkgs.writeText "korri-linux-host-input-seat-udev-rules" inputSeats.config.services.udev.extraRules;
in
assert allAssertionsPass valid;
assert cfg.services.korriBundle.enable;
assert cfg.services.korriLinuxInput.provider.enable;
assert !cfg.services.korriLinuxInput.provider.sourceHiding.enable;
assert cfg.services.korriLinuxInput.inputd.enable;
assert cfg.services.korridLinuxDevice.enable;
assert cfg.services.inputplumber.enable;
assert cfg.services.sunshine.enable;
assert !cfg.services.sunshine.autoStart;
assert !cfg.systemd.user.services.sunshine.enable;
assert cfg.services.sunshine.package == sunshinePackage;
assert builtins.elem "0020-add-korrid-certificate-control.patch" sunshinePackage.korriPatchNames;
assert cfg.services.korriLinuxHost.sunshine.runtimeSettings.enable;
assert !cfg.services.korriLinuxHost.sunshine.inputSeats.enable;
assert !(builtins.hasAttr "korri-input-seat-receiver" cfg.systemd.services);
assert allAssertionsPass inputSeats;
assert inputSeatReceiver.serviceConfig.User == "root";
assert inputSeatReceiver.serviceConfig.Group == "root";
assert inputSeatReceiver.serviceConfig.RuntimeDirectory == "korri-input-seat";
assert inputSeatReceiver.serviceConfig.RuntimeDirectoryMode == "0711";
assert inputSeatReceiver.serviceConfig.NoNewPrivileges;
assert inputSeatReceiver.serviceConfig.CapabilityBoundingSet == [ "CAP_CHOWN" ];
assert inputSeatReceiver.serviceConfig.AmbientCapabilities == [ ];
assert inputSeatReceiver.serviceConfig.RestrictAddressFamilies == [ "AF_UNIX" ];
assert inputSeatReceiver.serviceConfig.DevicePolicy == "closed";
assert inputSeatReceiver.serviceConfig.DeviceAllow == [ "/dev/uinput rw" ];
assert inputSeatReceiver.serviceConfig.PrivatePIDs;
assert inputSeatReceiver.serviceConfig.ProtectSystem == "strict";
assert inputSeatReceiver.serviceConfig.ProtectHome;
assert inputSeatReceiver.serviceConfig.ProtectProc == "invisible";
assert inputSeatReceiver.serviceConfig.ProcSubset == "pid";
assert inputSeatReceiver.serviceConfig.ReadWritePaths == [ "/run/korri-input-seat" ];
assert inputSeatReceiver.serviceConfig.SupplementaryGroups == [ ];
assert lib.hasInfix "/bin/korri-bundle-launch input-seat-receiver"
  inputSeatReceiver.serviceConfig.ExecStart;
assert lib.hasInfix "--control-uid 976" inputSeatReceiver.serviceConfig.ExecStart;
assert lib.hasInfix "--control-gid 976" inputSeatReceiver.serviceConfig.ExecStart;
assert lib.hasInfix "--sunshine-uid 1000" inputSeatReceiver.serviceConfig.ExecStart;
assert lib.hasInfix "--sunshine-gid 980" inputSeatReceiver.serviceConfig.ExecStart;
assert inputSeats.config.users.groups.korri-sunshine-input-seat.gid == 980;
assert
  inputSeatKorrid.environment.KORRID_INPUT_SEAT_CONTROL_SOCKET
  == "/run/korri-input-seat/control.sock";
assert
  inputSeatSunshine.environment.KORRI_INPUT_SEAT_MIRROR_SOCKET
  == "/run/korri-input-seat/sunshine-input-seat.sock";
assert inputSeatSunshine.environment.KORRI_INPUT_SEAT_RUNTIME_DIR == "/run/korri-input-seat";
assert inputSeatSunshine.serviceConfig.Group == "korri-sunshine-input-seat";
assert inputSeatSunshine.environment.KORRI_CERTIFICATE_CONTROL_UID == "976";
assert inputSeatSunshine.environment.KORRI_CERTIFICATE_CONTROL_GID == "976";
assert inputSeatSunshine.environment.KORRI_CERTIFICATE_CONTROL_OWNER_GID == "976";
assert inputSeatKorrid.environment.KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_UID == "0";
assert inputSeatKorrid.environment.KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_GID == "0";
assert builtins.elem "korri-sunshine-uinput" inputSeatSunshine.serviceConfig.SupplementaryGroups;
assert builtins.elem "korri-input-seat-receiver.service" inputSeatKorrid.requires;
assert builtins.elem "korri-input-seat-receiver.service" inputSeatKorrid.after;
assert builtins.elem "korri-input-seat-receiver.service" inputSeatSunshine.requires;
assert builtins.elem "korri-input-seat-receiver.service" inputSeatSunshine.after;
assert builtins.elem "-/run/korri-input-seat"
  inputSeats.config.systemd.services.korri-inputd.serviceConfig.InaccessiblePaths;
assert cfg.services.korriLinuxHost.sunshine.encoder == "auto";
assert sunshine.environment.SUNSHINE_LIVE_SETTINGS_MVP == "1";
assert
  if sunshinePackage.korriCudaEnabled then
    allAssertionsPass nvenc
  else
    hasFailedAssertion "requires a CUDA-enabled sunshine package" nvenc;
assert allAssertionsPass vaapi;
assert allAssertionsPass physicalSoftware;
assert hasFailedAssertion "DRM compositor requires" missingDrmDevice;
assert hasFailedAssertion "KMS capture requires" kmsWithoutDrm;
assert !(builtins.hasAttr "korri-streaming-performance-profile" cfg.systemd.services);
assert
  isX86_64
  == builtins.elem "korri-streaming-performance-profile.service" highRefresh.config.systemd.services.korri-compositor.requires;
assert
  isX86_64
  == builtins.elem "korri-streaming-performance-profile.service" highRefresh.config.systemd.services.korri-compositor.after;
assert !isX86_64 || highRefreshPerformance.serviceConfig.Type == "oneshot";
assert !isX86_64 || highRefreshPerformance.serviceConfig.RemainAfterExit;
assert
  nvenc.config.systemd.services.sunshine.environment.LD_LIBRARY_PATH == "/run/opengl-driver/lib";
assert nvenc.config.systemd.services.sunshine.environment.SUNSHINE_STRICT_ENCODER == "1";
assert !(builtins.hasAttr "LD_LIBRARY_PATH" sunshine.environment);
assert !(builtins.hasAttr "SUNSHINE_STRICT_ENCODER" sunshine.environment);
assert !(builtins.hasAttr "LD_LIBRARY_PATH" vaapi.config.systemd.services.sunshine.environment);
assert
  !(builtins.hasAttr "SUNSHINE_STRICT_ENCODER" vaapi.config.systemd.services.sunshine.environment);
assert builtins.length nvenc.config.systemd.services.sunshine.serviceConfig.ExecCondition == 1;
assert vaapi.config.systemd.services.sunshine.serviceConfig.ExecCondition == [ ];
assert
  let
    script = builtins.readFile (
      builtins.elemAt nvenc.config.systemd.services.sunshine.serviceConfig.ExecCondition 0
    );
  in
  lib.hasInfix "/run/opengl-driver/lib/libcuda.so.1" script
  && lib.hasInfix "/run/opengl-driver/lib/libnvidia-encode.so.1" script;
assert
  nvenc.config.systemd.services.sunshine.serviceConfig.ExecStart
  == "${sunshinePackage}/bin/sunshine /home/korri/.config/sunshine/sunshine.conf log_path=/dev/null encoder=nvenc";
assert noRuntimeSettings.config.services.sunshine.package == sunshinePackage;
assert
  noRuntimeSettings.config.systemd.services.sunshine.serviceConfig.ExecStart
  == "${sunshinePackage}/bin/sunshine /home/korri/.config/sunshine/sunshine.conf log_path=/dev/null";
assert
  !(builtins.hasAttr "SUNSHINE_LIVE_SETTINGS_MVP" noRuntimeSettings.config.systemd.services.sunshine.environment);
assert cfg.hardware.graphics.enable;
assert
  !pkgs.stdenv.hostPlatform.isx86_64
  || builtins.elem pkgs.intel-media-driver cfg.hardware.graphics.extraPackages;
assert physicalSoftware.config.hardware.graphics.extraPackages == [ ];
assert physicalSoftware.config.services.seatd.enable;
assert !(builtins.elem "seat" physicalSoftware.config.users.users.korri.extraGroups);
assert builtins.elem "seat" physicalSoftwareCompositor.serviceConfig.SupplementaryGroups;
assert !(builtins.elem "seat" physicalSoftwareSunshine.serviceConfig.SupplementaryGroups);
assert !(builtins.elem "seat" compositor.serviceConfig.SupplementaryGroups);
assert physicalSoftwareCompositor.environment.WLR_BACKENDS == "drm";
assert physicalSoftwareCompositor.environment.WLR_DRM_DEVICES == "/dev/dri/card0";
assert physicalSoftwareCompositor.environment.WLR_RENDER_DRM_DEVICE == "/dev/dri/renderD128";
assert !(builtins.hasAttr "WLR_LIBINPUT_NO_DEVICES" physicalSoftwareCompositor.environment);
assert builtins.elem "seatd.service" physicalSoftwareCompositor.requires;
assert builtins.elem "seatd.service" physicalSoftwareCompositor.after;
assert physicalSoftware.config.services.sunshine.capSysAdmin;
assert !physicalSoftwareSunshine.serviceConfig.NoNewPrivileges;
assert !physicalSoftwareSunshine.serviceConfig.PrivatePIDs;
assert
  physicalSoftwareSunshine.serviceConfig.CapabilityBoundingSet == [
    "CAP_SETPCAP"
    "CAP_SYS_ADMIN"
  ];
assert physicalSoftwareSunshine.serviceConfig.ExecCondition == [ ];
assert !(builtins.hasAttr "LD_LIBRARY_PATH" physicalSoftwareSunshine.environment);
assert !(builtins.hasAttr "SUNSHINE_STRICT_ENCODER" physicalSoftwareSunshine.environment);
assert
  physicalSoftwareSunshine.serviceConfig.ExecStart
  == "/run/wrappers/bin/sunshine /home/korri/.config/sunshine/sunshine.conf log_path=/dev/null capture=kms encoder=software";
assert lib.hasInfix "DSI-1" physicalSoftwareReadiness;
assert lib.hasInfix ".active == true" physicalSoftwareReadiness;
assert lib.hasInfix ".rect.width == $width" physicalSoftwareReadiness;
assert lib.hasInfix ".rect.height == $height" physicalSoftwareReadiness;
assert builtins.elem "uinput" cfg.boot.kernelModules;
assert cfg.users.users.korri-inputd.uid == 977;
assert cfg.users.groups.korri-control.gid == 977;
assert cfg.users.users.korrid.uid == 976;
assert cfg.users.groups.korrid.gid == 976;
assert cfg.users.groups.korri-sunshine-uinput.gid == 979;
assert builtins.elem "render" cfg.users.users.korri.extraGroups;
assert builtins.elem "video" cfg.users.users.korri.extraGroups;
assert !(builtins.elem "input" cfg.users.users.korri.extraGroups);
assert !(builtins.elem "uinput" cfg.users.users.korri.extraGroups);
assert inputd.serviceConfig.User == "korri-inputd";
assert korrid.serviceConfig.User == "korrid";
assert korrid.environment.KORRID_SUNSHINE_PRIVATE_STATE_ROOT == "/home/korri/.config/sunshine";
assert korrid.environment.KORRID_RELAYS == ''["wss://relay.example.com"]'';
assert
  korrid.environment.KORRID_UPSTREAMS
  == ''[{"baseUrl":"http://zao:43117","devicePublicKey":"${peerPublicKey}","kind":"native","label":"zao","moonlightAddress":"zao:47989"}]'';
assert
  korrid.environment.KORRID_SUNSHINE_CERTIFICATE_CONTROL_SOCKET
  == "/run/korri-certificate-control/control.sock";
assert korrid.environment.KORRID_SUNSHINE_CERTIFICATE_CONTROL_GID == "976";
assert korrid.environment.KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_UID == "0";
assert korrid.environment.KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_GID == "0";
assert cfg.services.korridLinuxDevice.sunshinePrivateStateRoot == "/home/korri/.config/sunshine";
assert cfg.services.korridLinuxDevice.relays == [ "wss://relay.example.com" ];
assert cfg.services.korridLinuxDevice.nativePeers == cfg.services.korriLinuxHost.nativePeers;
assert
  cfg.systemd.services.korrid-identity.serviceConfig.ExecStart
  == "${inputdPackage}/bin/korri-bundle-launch korrid identity status";
assert builtins.elem "/var/lib/korrid" korrid.serviceConfig.ReadWritePaths;
assert sunshine.serviceConfig.User == "korri";
assert sunshine.serviceConfig.WorkingDirectory == "/home/korri";
assert sunshine.environment.DISPLAY == ":0";
assert sunshine.environment.WAYLAND_DISPLAY == "korri-wayland";
assert sunshine.environment.XDG_RUNTIME_DIR == "/run/user/1000";
assert sunshine.environment.HOME == "/home/korri";
assert sunshine.environment.XDG_CONFIG_HOME == "/home/korri/.config";
assert sunshine.environment.KORRI_CERTIFICATE_CONTROL_UID == "976";
assert sunshine.environment.KORRI_CERTIFICATE_CONTROL_GID == "976";
assert sunshine.environment.KORRI_CERTIFICATE_CONTROL_OWNER_GID == "976";
assert sunshine.environment.KORRI_CERTIFICATE_CONTROL_MODE == "0660";
assert
  sunshine.environment.KORRI_CERTIFICATE_CONTROL_PATH
  == "/run/korri-certificate-control/control.sock";
assert certificateSocket.socketConfig.Accept == false;
assert
  certificateSocket.socketConfig.ListenSequentialPacket
  == "/run/korri-certificate-control/control.sock";
assert certificateSocket.socketConfig.FileDescriptorName == "korri-certificate-control";
assert certificateSocket.socketConfig.SocketUser == "root";
assert certificateSocket.socketConfig.SocketGroup == "korrid";
assert certificateSocket.socketConfig.SocketMode == "0660";
assert certificateSocket.socketConfig.DirectoryMode == "0751";
assert certificateSocket.socketConfig.RemoveOnStop;
assert certificateSocket.socketConfig.NonBlocking;
assert certificateSocket.socketConfig.Service == "sunshine.service";
assert builtins.elem "sockets.target" certificateSocket.wantedBy;
assert builtins.elem "systemd-tmpfiles-setup.service" certificateSocket.requires;
assert builtins.elem "systemd-tmpfiles-setup.service" certificateSocket.after;
assert builtins.elem "korri-certificate-control.socket" sunshine.requires;
assert builtins.elem "korri-certificate-control.socket" sunshine.after;
assert sunshine.serviceConfig.Sockets == [ "korri-certificate-control.socket" ];
assert builtins.elem "d /run/korri-certificate-control 0751 root korrid -"
  cfg.systemd.tmpfiles.rules;
assert builtins.elem "d /home/korri/.config 0700 korri korri -" cfg.systemd.tmpfiles.rules;
assert builtins.elem "d /home/korri/.config/sunshine 0700 korri korri -" cfg.systemd.tmpfiles.rules;
assert
  sunshine.serviceConfig.ExecStart
  == "${sunshinePackage}/bin/sunshine /home/korri/.config/sunshine/sunshine.conf log_path=/dev/null";
assert sunshine.serviceConfig.PrivatePIDs;
assert sunshine.serviceConfig.ProtectSystem == "strict";
assert sunshine.serviceConfig.ProtectHome == "read-only";
assert builtins.elem "/home/korri/.config/sunshine" sunshine.serviceConfig.ReadWritePaths;
assert !(cfg.systemd.services ? x11-headless);
assert compositor.serviceConfig.User == "korri";
assert compositor.environment.WLR_BACKENDS == "headless";
assert compositor.environment.WLR_RENDERER == "gles2";
assert pixman.config.systemd.services.korri-compositor.environment.WLR_RENDERER == "pixman";
assert compositor.environment.WLR_RENDER_DRM_DEVICE == "/dev/dri/renderD128";
assert (compositor.environment.WAYLAND_DISPLAY or null) == null;
assert compositor.environment.SWAYSOCK == "/run/korri-compositor/sway-ipc.sock";
assert !(builtins.hasAttr "GBM_BACKEND" compositor.environment);
assert !(builtins.hasAttr "__GLX_VENDOR_LIBRARY_NAME" compositor.environment);
assert !(builtins.hasAttr "LD_LIBRARY_PATH" compositor.environment);
assert nvencCompositor.environment.GBM_BACKEND == "nvidia-drm";
assert nvencCompositor.environment.__GLX_VENDOR_LIBRARY_NAME == "nvidia";
assert nvencCompositor.environment.LD_LIBRARY_PATH == "/run/opengl-driver/lib";
assert !(builtins.hasAttr "GBM_BACKEND" vaapiCompositor.environment);
assert !(builtins.hasAttr "__GLX_VENDOR_LIBRARY_NAME" vaapiCompositor.environment);
assert !(builtins.hasAttr "LD_LIBRARY_PATH" vaapiCompositor.environment);
assert compositor.serviceConfig.PrivateDevices == false;
assert compositor.serviceConfig.ProtectHome == "read-only";
assert compositor.serviceConfig.RuntimeDirectory == "korri-compositor";
assert builtins.elem "user-runtime-dir@1000.service" compositor.requires;
assert builtins.elem "user@1000.service" compositor.requires;
assert builtins.elem "korrid.service" compositor.wants;
assert builtins.elem "sunshine.service" compositor.wants;
assert builtins.elem "korri-compositor.service" korrid.bindsTo;
assert builtins.elem "korri-compositor.service" korrid.requires;
assert builtins.elem "korri-compositor.service" korrid.after;
assert builtins.elem "korri-compositor.service" sunshine.bindsTo;
assert builtins.length compositor.serviceConfig.ExecStartPost == 2;
assert lib.hasInfix "korri-publish-wayland-socket" (
  builtins.elemAt compositor.serviceConfig.ExecStartPost 0
);
assert lib.hasInfix "korri-wait-for-compositor" (
  builtins.elemAt compositor.serviceConfig.ExecStartPost 1
);
assert builtins.elem "korri-input-source-guard.service" sunshine.requires;
assert builtins.elem "korri-compositor.service" sunshine.requires;
assert builtins.elem "/dev/inputplumber/sources" sunshine.serviceConfig.InaccessiblePaths;
assert builtins.elem "/run/korri-compositor" sunshine.serviceConfig.InaccessiblePaths;
# The readiness probe reads the Sway IPC socket the sandbox hides, so it must
# run with full privileges outside the unit's mount namespace.
assert lib.hasPrefix "+/nix/store/" sunshine.serviceConfig.ExecStartPre;
assert lib.hasSuffix "-korri-wait-for-compositor" sunshine.serviceConfig.ExecStartPre;
assert builtins.elem 39217 cfg.networking.firewall.interfaces.tailscale0.allowedTCPPorts;
assert builtins.hasAttr "workspace-next" cfg.services.korriLinuxInput.inputd.actions;
assert
  validationAction == [
    "${pkgs.sway-unwrapped}/bin/swaymsg"
    "-s"
    "/run/korri-compositor/sway-ipc.sock"
    ''workspace "korri:game:active"; focus child; fullscreen enable; border none''
  ];
assert cfg.services.korridLinuxDevice.compositorControlDirectory == "/run/korri-compositor";
assert korrid.environment.KORRID_COMPOSITOR_CONTROL_DIRECTORY == "/run/korri-compositor";
assert noValidation.config.services.korriLinuxInput.inputd.actions == { };
assert hasFailedAssertion "runtime identity" wrongRuntimeUid;
assert evaluationRejected wrongRuntimeUid;
assert hasFailedAssertion "exact approved sunshine-korri" stockSunshine;
assert evaluationRejected stockSunshine;
assert hasFailedAssertion "exact approved sunshine-korri" lookalikeSunshine;
assert evaluationRejected lookalikeSunshine;
assert hasFailedAssertion "exact approved sunshine-korri" strippedApprovedSunshine;
assert evaluationRejected strippedApprovedSunshine;
assert hasFailedAssertion "differ from the runtime identity" collidingIdentity;
assert evaluationRejected invalidLabel;
assert hasFailedAssertion "certificate-control socket inode ownership" wrongCertificateSocketGroup;
assert evaluationRejected wrongCertificateSocketGroup;
pkgs.runCommand "korri-linux-host-module-check" { } ''
    grep -F 'id = "inputd-gate"' ${deviceConfig} >/dev/null
    grep -F 'title = "Streaming gate"' ${deviceConfig} >/dev/null
    grep -F '/bin/tini' ${deviceConfig} >/dev/null
    grep -F '/bin/timeout' ${deviceConfig} >/dev/null
    grep -F -- '--kill-after=5s' ${deviceConfig} >/dev/null
    grep -F '/bin/korri-streaming-validation-motion' ${deviceConfig} >/dev/null
    grep -F '/bin/korri-streaming-validation-motion' ${highRefreshDeviceConfig} >/dev/null
    grep -F '/bin/korri-streaming-validation-motion' ${pixmanDeviceConfig} >/dev/null
    ! grep -F '/bin/mpv' ${deviceConfig} >/dev/null
    ! grep -F -- '--loop-file=inf' ${highRefreshDeviceConfig} >/dev/null
    ! grep -F 'LD_LIBRARY_PATH=/run/opengl-driver/lib' ${nvencDeviceConfig} >/dev/null
    validation_motion="$(${pkgs.gnugrep}/bin/grep -oE '/nix/store/[^\"]+/bin/korri-streaming-validation-motion' ${deviceConfig} | head -n1)"
    high_refresh_validation_motion="$(${pkgs.gnugrep}/bin/grep -oE '/nix/store/[^\"]+/bin/korri-streaming-validation-motion' ${highRefreshDeviceConfig} | head -n1)"
    test -x "$validation_motion"
    test "$validation_motion" = "$high_refresh_validation_motion"
    ${pkgs.gnugrep}/bin/grep -A4 -F "$validation_motion" ${deviceConfig} | ${pkgs.gnugrep}/bin/grep -F '"60"' >/dev/null
    ${pkgs.gnugrep}/bin/grep -A4 -F "$validation_motion" ${highRefreshDeviceConfig} | ${pkgs.gnugrep}/bin/grep -F '"120"' >/dev/null
    grep -F 'id = "neverball"' ${deviceConfig} >/dev/null
    grep -F 'title = "Neverball (consumer)"' ${deviceConfig} >/dev/null
    grep -F '${pkgs.neverball}/bin/neverball' ${deviceConfig} >/dev/null
    grep -F 'DISPLAY = ":0"' ${deviceConfig} >/dev/null
    grep -F 'XDG_SESSION_TYPE = "x11"' ${deviceConfig} >/dev/null
    ! grep -F 'WAYLAND_DISPLAY' ${deviceConfig} >/dev/null
    ! grep -F 'XDG_RUNTIME_DIR' ${deviceConfig} >/dev/null
    ! grep -F 'SWAYSOCK' ${deviceConfig} >/dev/null
    grep -F -- '--config /nix/store/' ${compositorExec} >/dev/null
    high_refresh_compositor_config="$(${pkgs.gnugrep}/bin/grep -oE '/nix/store/[^ ]+-korri-sway\.conf' ${highRefreshCompositorExec} | head -n1)"
    grep -F 'output HEADLESS-1 mode 1920x1080@120Hz' "$high_refresh_compositor_config" >/dev/null
    ${lib.optionalString isX86_64 ''
      performance_script="$(${pkgs.gnugrep}/bin/grep -oE '/nix/store/[^ ]+-korri-streaming-performance-profile' ${highRefreshPerformanceExec} | head -n1)"
      test -x "$performance_script"
      grep -F 'performance' "$performance_script" >/dev/null
      grep -F "printf '60\\n'" "$performance_script" >/dev/null
      grep -F "printf '40\\n'" "$performance_script" >/dev/null
      grep -F "printf '100\\n'" "$performance_script" >/dev/null
      grep -F "printf '16\\n'" "$performance_script" >/dev/null
      grep -F 'balanced' "$performance_script" >/dev/null
      grep -F ' start' ${highRefreshPerformanceExec} >/dev/null
      grep -F ' stop' ${highRefreshPerformanceStop} >/dev/null
      profile_test="$TMPDIR/performance-profile"
      mkdir "$profile_test"
      printf 'balanced\n' >"$profile_test/profile"
      printf 'cool quiet balanced performance\n' >"$profile_test/choices"
      printf '16\n' >"$profile_test/min"
      printf '100\n' >"$profile_test/max"
      run_profile() {
        KORRI_PLATFORM_PROFILE_PATH="$profile_test/profile" \
        KORRI_PLATFORM_PROFILE_CHOICES_PATH="$profile_test/choices" \
        KORRI_INTEL_PSTATE_MIN_PATH="$profile_test/min" \
        KORRI_INTEL_PSTATE_MAX_PATH="$profile_test/max" \
          "$performance_script" "$1"
      }
      run_profile start
      test "$(cat "$profile_test/profile")" = performance
      test "$(cat "$profile_test/min")" = 40
      test "$(cat "$profile_test/max")" = 60
      chmod 400 "$profile_test/profile"
      set +e
      run_profile stop >/dev/null 2>&1
      failed_stop_status=$?
      set -e
      chmod 600 "$profile_test/profile"
      test "$failed_stop_status" -ne 0
      test "$(cat "$profile_test/profile")" = performance
      test "$(cat "$profile_test/min")" = 40
      test "$(cat "$profile_test/max")" = 60
      run_profile stop
      test "$(cat "$profile_test/profile")" = balanced
      test "$(cat "$profile_test/min")" = 16
      test "$(cat "$profile_test/max")" = 100
      chmod 400 "$profile_test/min"
      set +e
      run_profile start >/dev/null 2>&1
      failed_start_status=$?
      set -e
      chmod 600 "$profile_test/min"
      test "$failed_start_status" -ne 0
      test "$(cat "$profile_test/profile")" = balanced
      test "$(cat "$profile_test/min")" = 16
      test "$(cat "$profile_test/max")" = 100
    ''}
    grep -Fx '${sunshinePackage}/bin/sunshine /home/korri/.config/sunshine/sunshine.conf log_path=/dev/null' ${sunshineExec} >/dev/null
    grep -F 'TAG-="uaccess"' ${udevRules} >/dev/null
    grep -F 'ATTRS{name}=="Korri Seat P[1-4]"' ${inputSeatUdevRules} >/dev/null
    grep -F 'GROUP="korri"' ${inputSeatUdevRules} >/dev/null
    grep -F 'MODE="0660"' ${inputSeatUdevRules} >/dev/null

    compositor_config="$(${pkgs.gnugrep}/bin/grep -oE '/nix/store/[^ ]+-korri-sway\.conf' ${compositorExec} | head -n1)"
    test -f "$compositor_config"
    ! ${pkgs.gnugrep}/bin/grep -F 'exec_always' "$compositor_config" >/dev/null
    runtime="$TMPDIR/runtime"
    control="$TMPDIR/control"
    mkdir -m 700 "$runtime" "$control"
    export PATH=${
      lib.makeBinPath [
        pkgs.sway
        pkgs.xwayland
        pkgs.coreutils
        pkgs.procps
        pkgs.gnugrep
      ]
    }
    XDG_RUNTIME_DIR="$runtime" \
      XDG_CONFIG_HOME="$TMPDIR/config" \
      XDG_STATE_HOME="$TMPDIR/state" \
      XDG_DATA_HOME="$TMPDIR/data" \
      WLR_BACKENDS=headless \
      WLR_LIBINPUT_NO_DEVICES=1 \
      WLR_RENDERER=pixman \
      SWAYSOCK="$control/sway-ipc.sock" \
      ${pkgs.sway-unwrapped}/bin/sway --unsupported-gpu --config "$compositor_config" \
      >"$TMPDIR/sway.log" 2>&1 &
    sway_pid=$!
    trap '${pkgs.coreutils}/bin/cat "$TMPDIR/sway.log" >&2 2>/dev/null || true; ${pkgs.coreutils}/bin/cat "$TMPDIR/motion.log" >&2 2>/dev/null || true; ${pkgs.coreutils}/bin/cat "$TMPDIR/motion-120.log" >&2 2>/dev/null || true; ${pkgs.coreutils}/bin/kill "$sway_pid" 2>/dev/null || true' EXIT
    attempt=0
    while [ "$attempt" -lt 80 ]; do
      raw_wayland_display="$(${pkgs.findutils}/bin/find "$runtime" -maxdepth 1 -type s -name 'wayland-[0-9]*' -printf '%f\n' | head -n1)"
      if test -S "$control/sway-ipc.sock" && test -n "$raw_wayland_display"; then
        break
      fi
      test -e "/proc/$sway_pid"
      attempt=$((attempt + 1))
      ${pkgs.coreutils}/bin/sleep 0.1
    done
    test -S "$control/sway-ipc.sock"
    raw_wayland_display="$(${pkgs.findutils}/bin/find "$runtime" -maxdepth 1 -type s -name 'wayland-[0-9]*' -printf '%f\n' | head -n1)"
    test -n "$raw_wayland_display"
    publisher=${builtins.elemAt compositor.serviceConfig.ExecStartPost 0}
    test -x "$publisher"
    XDG_RUNTIME_DIR="$runtime" "$publisher"
    test -S "$runtime/korri-wayland"
    test "$(${pkgs.coreutils}/bin/readlink "$runtime/korri-wayland")" = "$raw_wayland_display"
    wayland_display=korri-wayland
    ${pkgs.sway}/bin/swaymsg -s "$control/sway-ipc.sock" -t get_outputs -r \
      | ${pkgs.jq}/bin/jq -e '.[] | select(.name == "HEADLESS-1" and .active == true and .current_mode.width == 1920 and .current_mode.height == 1080 and .current_mode.refresh == 60000)' \
      >/dev/null
    test -S /tmp/.X11-unix/X0

    DISPLAY=:0 XDG_SESSION_TYPE=x11 \
      ${pkgs.coreutils}/bin/timeout --signal=TERM --kill-after=1s 8 \
      "$validation_motion" 1920 1080 60 >"$TMPDIR/motion.log" 2>&1 &
    player_pid=$!
    attempt=0
    while [ "$attempt" -lt 80 ]; do
      if ${pkgs.sway}/bin/swaymsg -s "$control/sway-ipc.sock" -t get_tree -r \
        | ${pkgs.jq}/bin/jq -e '.. | objects | select(.name? == "Korri streaming gate" and .fullscreen_mode == 0)' \
        >/dev/null 2>&1; then
        break
      fi
      test -e "/proc/$player_pid"
      attempt=$((attempt + 1))
      ${pkgs.coreutils}/bin/sleep 0.1
    done
    ${pkgs.sway}/bin/swaymsg -s "$control/sway-ipc.sock" -t get_tree -r \
      | ${pkgs.jq}/bin/jq -e '.. | objects | select(.name? == "Korri streaming gate" and .fullscreen_mode == 0)' \
      >/dev/null
    ${pkgs.sway}/bin/swaymsg -s "$control/sway-ipc.sock" '[title="Korri streaming gate"] floating enable' >/dev/null
    ${pkgs.sway}/bin/swaymsg -s "$control/sway-ipc.sock" -t get_tree -r \
      | ${pkgs.jq}/bin/jq -e '.. | objects | select(.name? == "Korri streaming gate" and .floating == "user_on" and .fullscreen_mode == 0)' \
      >/dev/null
    ${pkgs.sway}/bin/swaymsg -s "$control/sway-ipc.sock" ${lib.escapeShellArg (builtins.elemAt validationAction 3)} >/dev/null
    ${pkgs.sway}/bin/swaymsg -s "$control/sway-ipc.sock" -t get_tree -r \
      | ${pkgs.jq}/bin/jq -e '.. | objects | select(.name? == "Korri streaming gate" and .fullscreen_mode > 0)' \
      >/dev/null

    XDG_RUNTIME_DIR="$runtime" WAYLAND_DISPLAY="$wayland_display" \
      ${pkgs.grim}/bin/grim -o HEADLESS-1 "$TMPDIR/frame-1.png"
    ${pkgs.coreutils}/bin/sleep 0.5
    XDG_RUNTIME_DIR="$runtime" WAYLAND_DISPLAY="$wayland_display" \
      ${pkgs.grim}/bin/grim -o HEADLESS-1 "$TMPDIR/frame-2.png"
    test "$(${pkgs.imagemagick}/bin/magick identify -format '%wx%h' "$TMPDIR/frame-1.png")" = 1920x1080
    set +e
    ${pkgs.imagemagick}/bin/magick compare -metric AE "$TMPDIR/frame-1.png" "$TMPDIR/frame-2.png" null: >/dev/null 2>&1
    compare_status=$?
    set -e
    test "$compare_status" -eq 1
    ${pkgs.coreutils}/bin/sleep 0.6
    ${pkgs.gnugrep}/bin/grep -E 'korri-validation-fps=(5[5-9]|6[0-5])\.' "$TMPDIR/motion.log" >/dev/null

    ${pkgs.coreutils}/bin/kill "$player_pid"
    wait "$player_pid" || true

    DISPLAY=:0 XDG_SESSION_TYPE=x11 \
      ${pkgs.coreutils}/bin/timeout --signal=TERM --kill-after=1s 3 \
      "$validation_motion" 1920 1080 120 >"$TMPDIR/motion-120.log" 2>&1 &
    producer_120_pid=$!
    ${pkgs.coreutils}/bin/sleep 1.5
    ${pkgs.gnugrep}/bin/grep -E 'korri-validation-fps=(11[5-9]|12[0-5])\.' "$TMPDIR/motion-120.log" >/dev/null
    ${pkgs.coreutils}/bin/kill "$producer_120_pid"
    wait "$producer_120_pid" || true

    ${pkgs.coreutils}/bin/kill "$sway_pid"
    wait "$sway_pid" || true
    trap - EXIT

    home="$TMPDIR/home"
    private="$home/.config/sunshine"
    mkdir -p "$private/credentials"
    printf '{"root":{"named_devices":[]}}\n' > "$private/sunshine_state.json"
    printf '{}\n' > "$private/apps.json"
    cat > "$private/sunshine.conf" <<EOF
  file_state = $private/sunshine_state.json
  log_path = $private/sunshine.log
  EOF
    private_before="$(${inputdPackage}/bin/korri-sunshine-state-digest "$home" "$(${pkgs.coreutils}/bin/id -u)")"
    HOME="$home" XDG_CONFIG_HOME="$home/.config" \
      ${sunshinePackage}/bin/sunshine "$private/sunshine.conf" log_path=/dev/null --version \
      > "$TMPDIR/sunshine-version.txt"
    private_after="$(${inputdPackage}/bin/korri-sunshine-state-digest "$home" "$(${pkgs.coreutils}/bin/id -u)")"
    test "$private_before" = "$private_after"
    test ! -e "$private/sunshine.log"
    grep -F 'Sunshine version: 2025.924.154138-korri' "$TMPDIR/sunshine-version.txt" >/dev/null

    touch "$out"
''
