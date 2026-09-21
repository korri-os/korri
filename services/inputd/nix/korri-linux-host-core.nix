# Product-agnostic Linux compositor, input, audio, and korrid substrate.
# Streaming integrations compose this module separately.
{ korri }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korriLinuxHost;
  runtimeHome = config.users.users.${cfg.runtimeUser}.home or "/home/${cfg.runtimeUser}";
  runtimeDir = "/run/user/${toString cfg.runtimeUid}";
  compositorWorkspace = "korri:game:active";
  compositorMode = builtins.match "^([1-9][0-9]*)x([1-9][0-9]*)@([1-9][0-9]*)Hz$" cfg.compositor.mode;
  compositorWidth = builtins.elemAt compositorMode 0;
  compositorHeight = builtins.elemAt compositorMode 1;
  compositorRefreshRate = builtins.elemAt compositorMode 2;
  compositorReadiness = import ./korri-compositor-readiness.nix {
    inherit lib pkgs runtimeDir;
  };
  inherit (compositorReadiness)
    compositorControlDirectory
    compositorControlSocket
    compositorStillRunning
    validAbsolutePath
    waitForCompositor
    waylandDisplay
    xwaylandDisplay
    xwaylandLock
    xwaylandSocket
    ;
  validationMotion = pkgs.stdenv.mkDerivation {
    pname = "korri-validation-motion";
    version = "0.0.0";
    src = ../validation/x11-native-motion.c;
    dontUnpack = true;
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.xorg.libX11 ];
    buildPhase = ''
      runHook preBuild
      $CC -std=c11 -O2 -Wall -Wextra -Werror "$src" \
        $(${pkgs.pkg-config}/bin/pkg-config --cflags --libs x11) \
        -o korri-validation-motion
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      install -Dm755 korri-validation-motion "$out/bin/korri-validation-motion"
      runHook postInstall
    '';
    meta.mainProgram = "korri-validation-motion";
  };
  generatedDeviceConfig = pkgs.writeText "korrid-${cfg.label}-host.toml" ''
    label = "${cfg.label}"

    [environment]
    DISPLAY = "${xwaylandDisplay}"
    XDG_SESSION_TYPE = "x11"

    ${lib.optionalString cfg.validation.enable ''
      [[games]]
      id = "inputd-gate"
      title = "Input validation gate"
      command = [
        "${lib.getExe pkgs.tini}",
        "--",
        "${lib.getExe' pkgs.coreutils "timeout"}",
        "--signal=TERM",
        "--kill-after=5s",
        "600",
        "${lib.getExe validationMotion}",
        "${compositorWidth}",
        "${compositorHeight}",
        "${compositorRefreshRate}",
        "--fullscreen"
      ]

      [[games]]
      id = "neverball"
      title = "Neverball (${cfg.label})"
      command = [
        "${lib.getExe pkgs.tini}",
        "--",
        "${lib.getExe' pkgs.coreutils "timeout"}",
        "--signal=TERM",
        "--kill-after=5s",
        "600",
        "${lib.getExe' pkgs.neverball "neverball"}"
      ]
    ''}
  '';
  deviceConfig = if cfg.deviceConfig == null then generatedDeviceConfig else cfg.deviceConfig;
  swayConfig = pkgs.writeText "korri-sway.conf" ''
    default_border none
    default_floating_border none
    hide_edge_borders both
    xwayland force
    seat * hide_cursor 1500
    ${lib.optionalString (!cfg.compositor.localInput.enable) ''
      input "*" events disabled
      ${lib.concatMapStringsSep "\n" (
        id: ''input "${id}" events enabled''
      ) cfg.compositor.allowedInputIdentifiers}
    ''}
    output ${cfg.compositor.outputName} mode ${cfg.compositor.mode}
    output ${cfg.compositor.outputName} bg #000000 solid_color
    workspace "${compositorWorkspace}" output ${cfg.compositor.outputName}
    workspace "${compositorWorkspace}"
    ${cfg.compositor.extraConfig}
  '';
  resolveDrmDevice = pkgs.writeShellScript "korri-resolve-drm-device" ''
    set -eu
    want="''${WLR_DRM_DEVICES:-}"
    if [ -n "$want" ] && [ ! -e "$want" ]; then
      case "$want" in
        /dev/dri/by-path/platform-*-card)
          name="''${want#/dev/dri/by-path/platform-}"
          name="''${name%-card}"
          for card in /sys/class/drm/card[0-9]*; do
            [ -e "$card/device" ] || continue
            target=$(${pkgs.coreutils}/bin/readlink -f "$card/device")
            if [ "''${target##*/}" = "$name" ]; then
              WLR_DRM_DEVICES="/dev/dri/''${card##*/}"
              export WLR_DRM_DEVICES
              break
            fi
          done
          ;;
      esac
    fi
    exec ${pkgs.sway}/bin/sway --unsupported-gpu --config "$KORRI_SWAY_CONFIG"
  '';
  # The executable is product identity; the render node is a hardware fact
  # supplied through the service environment so it cannot perturb that identity.
  waitForRenderDevice = pkgs.writeShellScript "korri-wait-for-render-device" ''
    set -eu
    device="''${KORRI_RENDER_DEVICE:?}"
    for _ in $(${pkgs.coreutils}/bin/seq 1 300); do
      if [ -r "$device" ] && [ -w "$device" ]; then
        exit 0
      fi
      ${pkgs.coreutils}/bin/sleep 0.1
    done
    echo "render node $device did not become usable" >&2
    exit 1
  '';
  publishWaylandSocket = pkgs.writeShellScript "korri-publish-wayland-socket" ''
    set -eu
    destination="$XDG_RUNTIME_DIR/${waylandDisplay}"
    attempt=0
    while [ "$attempt" -lt 60 ]; do
      ${compositorStillRunning}
      source=
      count=0
      for socket in "$XDG_RUNTIME_DIR"/wayland-[0-9]*; do
        [ -S "$socket" ] || continue
        source="$socket"
        count=$((count + 1))
      done
      if [ "$count" -gt 1 ]; then
        echo "Sway published more than one numeric Wayland socket" >&2
        exit 1
      fi
      if [ "$count" -eq 1 ]; then
        target="''${source##*/}"
        next="$destination.next.$$"
        ${pkgs.coreutils}/bin/ln -s -- "$target" "$next"
        ${pkgs.coreutils}/bin/mv -Tf -- "$next" "$destination"
        exit 0
      fi
      attempt=$((attempt + 1))
      ${pkgs.coreutils}/bin/sleep 0.25
    done
    echo "Sway Wayland socket did not become ready" >&2
    exit 1
  '';
  cleanupCompositorSockets = pkgs.writeShellScript "korri-clean-compositor-sockets" ''
    set -eu
    stable_wayland=${lib.escapeShellArg "${runtimeDir}/${waylandDisplay}"}
    if [ -e "$stable_wayland" ] && [ ! -L "$stable_wayland" ]; then
      echo "stable Wayland path is not a symbolic link" >&2
      exit 1
    fi
    ${pkgs.coreutils}/bin/rm -f -- "$stable_wayland" "$stable_wayland".next.*
    for socket in ${lib.escapeShellArg runtimeDir}/wayland-[0-9]*; do
      [ -e "$socket" ] || continue
      case "$socket" in
        *.lock) continue ;;
      esac
      [ -S "$socket" ] || {
        echo "unexpected Wayland path type: $socket" >&2
        exit 1
      }
      if ${pkgs.psmisc}/bin/fuser -s "$socket"; then
        echo "Wayland socket is still owned: $socket" >&2
        exit 1
      fi
      ${pkgs.coreutils}/bin/rm -f -- "$socket" "$socket.lock"
    done
    for socket in \
      ${lib.escapeShellArg compositorControlSocket} \
      ${lib.escapeShellArg xwaylandSocket}; do
      if [ -e "$socket" ]; then
        if ${pkgs.psmisc}/bin/fuser -s "$socket"; then
          echo "compositor socket is still owned: $socket" >&2
          exit 1
        fi
        ${pkgs.coreutils}/bin/rm -f -- "$socket"
      fi
    done
    if [ -e ${lib.escapeShellArg xwaylandLock} ]; then
      lock_pid="$(${pkgs.coreutils}/bin/tr -d '[:space:]' < ${lib.escapeShellArg xwaylandLock})"
      case "$lock_pid" in
        ""|*[!0-9]*)
          echo "Xwayland lock has an invalid PID" >&2
          exit 1
          ;;
      esac
      if [ -e "/proc/$lock_pid" ]; then
        echo "Xwayland display ${xwaylandDisplay} is still owned by PID $lock_pid" >&2
        exit 1
      fi
      ${pkgs.coreutils}/bin/rm -f -- ${lib.escapeShellArg xwaylandLock}
    fi
  '';
  validationActions = lib.optionalAttrs cfg.validation.enable {
    workspace-next.command = [
      "${pkgs.sway-unwrapped}/bin/swaymsg"
      "-s"
      compositorControlSocket
      ''workspace "${compositorWorkspace}"; focus child; fullscreen enable; border none''
    ];
  };
in
{
  imports = [
    (import ./korri-bundle-module.nix { inherit korri; })
    (import ./korri-input.nix { inherit korri; })
    (import ../../korrid/nixos-module.nix { inherit korri; })
  ];

  options.services.korriLinuxHost = {
    enable = lib.mkEnableOption "the Korri Linux host substrate";
    label = lib.mkOption {
      type = lib.types.strMatching "^[A-Za-z0-9._-]+$";
      default = config.networking.hostName;
    };
    runtimeUser = lib.mkOption { type = lib.types.str; };
    runtimeUid = lib.mkOption { type = lib.types.ints.positive; };
    runtimeGroup = lib.mkOption { type = lib.types.str; };
    runtimeGid = lib.mkOption { type = lib.types.ints.positive; };
    apiPort = lib.mkOption {
      type = lib.types.port;
      default = 39217;
    };
    firewallInterfaces = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
    };
    deviceConfig = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
    };
    storageRoot = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/korri";
    };
    privateStateRoot = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/korrid";
    };
    relays = lib.mkOption { type = lib.types.listOf lib.types.str; };
    advertisedEndpoints = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
    };
    ownerBindingFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
    };
    compositor = {
      localInput.enable = lib.mkEnableOption "physical touch and controller input on DRM";
      allowedInputIdentifiers = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [ ];
        internal = true;
        description = "Exact compositor input identifiers admitted by an optional integration.";
      };
      backend = lib.mkOption {
        type = lib.types.enum [
          "headless"
          "drm"
        ];
        default = "headless";
      };
      drmDevice = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
      };
      renderDevice = lib.mkOption { type = lib.types.str; };
      outputName = lib.mkOption {
        type = lib.types.strMatching "^[A-Za-z0-9._-]+$";
        default = "HEADLESS-1";
      };
      mode = lib.mkOption {
        type = lib.types.strMatching "^[1-9][0-9]*x[1-9][0-9]*@[1-9][0-9]*Hz$";
        default = "1920x1080@60Hz";
      };
      extraConfig = lib.mkOption {
        type = lib.types.lines;
        default = "";
      };
      neverFocusAppIds = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [ ];
      };
      renderer = lib.mkOption {
        type = lib.types.enum [
          "gles2"
          "pixman"
        ];
        default = "gles2";
      };
    };
    serviceIdentities = {
      inputdUid = lib.mkOption {
        type = lib.types.ints.positive;
        default = 977;
      };
      controlGid = lib.mkOption {
        type = lib.types.ints.positive;
        default = 977;
      };
      korridUid = lib.mkOption {
        type = lib.types.ints.positive;
        default = 976;
      };
      korridGid = lib.mkOption {
        type = lib.types.ints.positive;
        default = 976;
      };
      localSignerUid = lib.mkOption {
        type = lib.types.ints.positive;
        default = 978;
      };
      localSignerGid = lib.mkOption {
        type = lib.types.ints.positive;
        default = 978;
      };
    };
    audio.enable = lib.mkEnableOption "gameplay-user PipeWire audio";
    validation.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion =
          let
            user = config.users.users.${cfg.runtimeUser} or { };
            group = config.users.groups.${cfg.runtimeGroup} or { };
          in
          (user.isNormalUser or false)
          && (user.uid or null) == cfg.runtimeUid
          && (user.group or null) == cfg.runtimeGroup
          && (group.gid or null) == cfg.runtimeGid;
        message = "services.korriLinuxHost runtime identity must match an existing user and primary group exactly.";
      }
      {
        assertion = lib.all (value: value != cfg.runtimeUid && value != cfg.runtimeGid) [
          cfg.serviceIdentities.inputdUid
          cfg.serviceIdentities.controlGid
          cfg.serviceIdentities.korridUid
          cfg.serviceIdentities.korridGid
          cfg.serviceIdentities.localSignerUid
          cfg.serviceIdentities.localSignerGid
        ];
        message = "Korri service identities must differ from the runtime identity.";
      }
      {
        assertion =
          cfg.serviceIdentities.inputdUid != cfg.serviceIdentities.korridUid
          && cfg.serviceIdentities.inputdUid != cfg.serviceIdentities.localSignerUid
          && cfg.serviceIdentities.controlGid != cfg.serviceIdentities.korridGid
          && cfg.serviceIdentities.controlGid != cfg.serviceIdentities.localSignerGid
          && cfg.serviceIdentities.korridUid != cfg.serviceIdentities.localSignerUid
          && cfg.serviceIdentities.korridGid != cfg.serviceIdentities.localSignerGid;
        message = "Korri service identities must remain distinct.";
      }
      {
        assertion = lib.all validAbsolutePath [
          runtimeHome
          cfg.storageRoot
          cfg.privateStateRoot
          cfg.compositor.renderDevice
        ];
        message = "services.korriLinuxHost paths must be normalized absolute paths without whitespace.";
      }
      {
        assertion = lib.hasPrefix "/dev/dri/" cfg.compositor.renderDevice;
        message = "services.korriLinuxHost compositor renderDevice must be under /dev/dri/.";
      }
      {
        assertion =
          cfg.compositor.backend != "drm"
          || (
            cfg.compositor.drmDevice != null
            &&
              builtins.match "^/dev/dri/(card[0-9]+|by-path/[A-Za-z0-9._:+-]+-card)$" cfg.compositor.drmDevice
              != null
          );
        message = "services.korriLinuxHost DRM compositor requires an exact /dev/dri/cardN device or a /dev/dri/by-path/*-card link.";
      }
      {
        assertion =
          cfg.compositor.backend == "drm"
          || (!cfg.compositor.localInput.enable && cfg.compositor.allowedInputIdentifiers == [ ]);
        message = "services.korriLinuxHost compositor input requires the DRM backend.";
      }
      {
        assertion = lib.all (name: builtins.match "[A-Za-z0-9_.:-]+" name != null) cfg.firewallInterfaces;
        message = "services.korriLinuxHost firewall interface names are invalid.";
      }
    ];

    services.korriBundle.enable = true;
    services.korriLinuxInput = {
      provider.enable = true;
      inputd = {
        enable = true;
        requireProvider = true;
        uid = cfg.serviceIdentities.inputdUid;
        controlGid = cfg.serviceIdentities.controlGid;
        actionUser = cfg.runtimeUser;
        actionUid = cfg.runtimeUid;
        actionGid = cfg.runtimeGid;
        actions = validationActions;
      };
    };
    services.korridLinuxDevice = {
      enable = true;
      uid = cfg.serviceIdentities.korridUid;
      gid = cfg.serviceIdentities.korridGid;
      runtimeUser = cfg.runtimeUser;
      runtimeUid = cfg.runtimeUid;
      runtimeGid = cfg.runtimeGid;
      inputdUid = cfg.serviceIdentities.inputdUid;
      controlGid = cfg.serviceIdentities.controlGid;
      localSignerUid = cfg.serviceIdentities.localSignerUid;
      localSignerGid = cfg.serviceIdentities.localSignerGid;
      inherit deviceConfig;
      address = "0.0.0.0:${toString cfg.apiPort}";
      storageRoot = cfg.storageRoot;
      privateStateRoot = cfg.privateStateRoot;
      inherit (cfg)
        relays
        ownerBindingFile
        advertisedEndpoints
        ;
      inherit compositorControlDirectory compositorControlSocket;
      neverFocusAppIds = cfg.compositor.neverFocusAppIds;
    };

    services.pipewire = lib.mkIf cfg.audio.enable {
      enable = true;
      alsa.enable = true;
      pulse.enable = true;
      wireplumber.enable = true;
    };
    systemd.user.services.pipewire.wantedBy = lib.mkIf cfg.audio.enable [ "default.target" ];
    systemd.user.services.pipewire-pulse.wantedBy = lib.mkIf cfg.audio.enable [ "default.target" ];
    security.rtkit.enable = lib.mkIf cfg.audio.enable true;
    hardware.graphics.enable = true;
    services.seatd.enable = cfg.compositor.backend == "drm";

    users.users.${cfg.runtimeUser} = {
      uid = lib.mkDefault cfg.runtimeUid;
      group = lib.mkDefault cfg.runtimeGroup;
      linger = lib.mkIf cfg.audio.enable true;
      extraGroups = lib.mkAfter (
        [
          "render"
          "video"
        ]
        ++ lib.optional cfg.audio.enable "audio"
      );
    };

    systemd.tmpfiles.rules = [
      "d ${cfg.storageRoot} 0700 korrid korrid -"
      "d ${runtimeHome}/.config :0700 ${cfg.runtimeUser} ${cfg.runtimeGroup} -"
    ];

    networking.firewall.interfaces = lib.genAttrs cfg.firewallInterfaces (_: {
      allowedTCPPorts = [ cfg.apiPort ];
    });

    systemd.services.korri-compositor = {
      description = "Korri Sway compositor";
      wantedBy = [ "multi-user.target" ];
      startLimitIntervalSec = 120;
      startLimitBurst = 60;
      wants = [ "korrid.service" ];
      requires = [
        "user-runtime-dir@${toString cfg.runtimeUid}.service"
        "user@${toString cfg.runtimeUid}.service"
      ]
      ++ lib.optional (cfg.compositor.backend == "drm") "seatd.service";
      after = [
        "systemd-tmpfiles-setup.service"
        "user-runtime-dir@${toString cfg.runtimeUid}.service"
        "user@${toString cfg.runtimeUid}.service"
      ]
      ++ lib.optional (cfg.compositor.backend == "drm") "seatd.service";
      before = [ "korrid.service" ];
      path = [
        pkgs.dbus
        pkgs.sway
        pkgs.xwayland
      ];
      environment = {
        HOME = runtimeHome;
        XDG_RUNTIME_DIR = runtimeDir;
        XDG_CONFIG_HOME = "${compositorControlDirectory}/config";
        XDG_STATE_HOME = "${compositorControlDirectory}/state";
        XDG_DATA_HOME = "${compositorControlDirectory}/data";
        DBUS_SESSION_BUS_ADDRESS = "unix:path=${runtimeDir}/bus";
        XDG_CURRENT_DESKTOP = "sway";
        SWAYSOCK = compositorControlSocket;
        KORRI_WAYLAND_DISPLAY = waylandDisplay;
        KORRI_SWAY_CONFIG = swayConfig;
        KORRI_COMPOSITOR_OUTPUT_NAME = cfg.compositor.outputName;
        KORRI_COMPOSITOR_OUTPUT_WIDTH = compositorWidth;
        KORRI_COMPOSITOR_OUTPUT_HEIGHT = compositorHeight;
        WLR_RENDERER = cfg.compositor.renderer;
        WLR_RENDER_DRM_DEVICE = cfg.compositor.renderDevice;
        KORRI_RENDER_DEVICE = cfg.compositor.renderDevice;
        WLR_NO_HARDWARE_CURSORS = "1";
      }
      // lib.optionalAttrs (cfg.compositor.backend == "headless") {
        WLR_BACKENDS = "headless";
        WLR_LIBINPUT_NO_DEVICES = "1";
      }
      // lib.optionalAttrs (cfg.compositor.backend == "drm") {
        LIBSEAT_BACKEND = "seatd";
        WLR_BACKENDS =
          if cfg.compositor.localInput.enable || cfg.compositor.allowedInputIdentifiers != [ ] then
            "drm,libinput"
          else
            "drm";
        WLR_DRM_DEVICES = cfg.compositor.drmDevice;
      };
      serviceConfig = {
        Type = "simple";
        User = cfg.runtimeUser;
        Group = cfg.runtimeGroup;
        SupplementaryGroups = [
          "video"
          "render"
        ]
        ++ lib.optional (cfg.compositor.backend == "drm") "seat";
        RuntimeDirectory = "korri-compositor";
        RuntimeDirectoryMode = "0750";
        ExecStartPre = [
          "+${cleanupCompositorSockets}"
          waitForRenderDevice
        ];
        ExecStart = resolveDrmDevice;
        ExecStartPost = [
          publishWaylandSocket
          waitForCompositor
        ];
        ExecStopPost = "+${cleanupCompositorSockets}";
        Restart = "on-failure";
        RestartSec = 2;
        UMask = "0007";
        NoNewPrivileges = true;
        CapabilityBoundingSet = [ ];
        AmbientCapabilities = [ ];
        PrivateTmp = false;
        PrivateDevices = false;
        ProtectSystem = "strict";
        ProtectHome = "read-only";
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = false;
        SystemCallArchitectures = "native";
        ReadWritePaths = [
          compositorControlDirectory
          runtimeDir
          "/tmp"
        ];
      };
    };

    systemd.services.korrid = {
      bindsTo = lib.mkAfter [ "korri-compositor.service" ];
      requires = lib.mkAfter [ "korri-compositor.service" ];
      after = lib.mkAfter [ "korri-compositor.service" ];
    };
  };
}
