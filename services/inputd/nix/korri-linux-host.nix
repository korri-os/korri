{ korri }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korriLinuxHost;
  system = pkgs.stdenv.hostPlatform.system;
  sunshineApproved = import ../../sunshine/approved-patches.nix;
  gameplayHome = config.users.users.${cfg.gameplayUser}.home or "/home/${cfg.gameplayUser}";
  gameplayRuntimeDir = "/run/user/${toString cfg.gameplayUid}";
  compositorControlDirectory = "/run/korri-compositor";
  compositorControlSocket = "${compositorControlDirectory}/sway-ipc.sock";
  compositorWorkspace = "korri:game:active";
  waylandDisplay = "korri-wayland";
  xwaylandDisplay = ":0";
  xwaylandSocket = "/tmp/.X11-unix/X0";
  xwaylandLock = "/tmp/.X0-lock";
  inputSeatRuntimeDirectory = "/run/korri-input-seat";
  inputSeatControlSocket = "${inputSeatRuntimeDirectory}/control.sock";
  inputSeatMirrorSocket = "${inputSeatRuntimeDirectory}/sunshine-input-seat.sock";
  certificateControlDirectory = "/run/korri-certificate-control";
  certificateControlSocket = "${certificateControlDirectory}/control.sock";
  certificateControlMode = "0660";
  compositorMode = builtins.match "^([1-9][0-9]*)x([1-9][0-9]*)@([1-9][0-9]*)Hz$" cfg.compositor.mode;
  compositorWidth = builtins.elemAt compositorMode 0;
  compositorHeight = builtins.elemAt compositorMode 1;
  compositorRefreshRate = builtins.elemAt compositorMode 2;
  highRefreshPerformance = lib.toInt compositorRefreshRate >= 120;
  sunshineConfig =
    if cfg.sunshine.configDirectory == null then
      "${gameplayHome}/.config/sunshine"
    else
      cfg.sunshine.configDirectory;
  streamingValidationMotion = pkgs.stdenv.mkDerivation {
    pname = "korri-streaming-validation-motion";
    version = "0.0.0";
    src = ../validation/x11-native-motion.c;
    dontUnpack = true;
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.xorg.libX11 ];
    buildPhase = ''
      runHook preBuild
      $CC -std=c11 -O2 -Wall -Wextra -Werror "$src" \
        $(${pkgs.pkg-config}/bin/pkg-config --cflags --libs x11) \
        -o korri-streaming-validation-motion
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      install -Dm755 korri-streaming-validation-motion \
        "$out/bin/korri-streaming-validation-motion"
      runHook postInstall
    '';
    meta.mainProgram = "korri-streaming-validation-motion";
  };
  streamingPerformanceProfile = pkgs.writeShellScript "korri-streaming-performance-profile" ''
    set -eu
    profile="''${KORRI_PLATFORM_PROFILE_PATH:-/sys/firmware/acpi/platform_profile}"
    choices="''${KORRI_PLATFORM_PROFILE_CHOICES_PATH:-/sys/firmware/acpi/platform_profile_choices}"
    min_perf="''${KORRI_INTEL_PSTATE_MIN_PATH:-/sys/devices/system/cpu/intel_pstate/min_perf_pct}"
    max_perf="''${KORRI_INTEL_PSTATE_MAX_PATH:-/sys/devices/system/cpu/intel_pstate/max_perf_pct}"
    for path in "$profile" "$choices" "$min_perf" "$max_perf"; do
      [ -f "$path" ] || {
        echo "required streaming performance control is absent: $path" >&2
        exit 1
      }
    done
    restore_on_failure() {
      status=$?
      trap - EXIT
      if [ "$committed" != true ]; then
        printf '%s\n' "$original_min" >"$min_perf" || status=1
        printf '%s\n' "$original_max" >"$max_perf" || status=1
        printf '%s\n' "$original_profile" >"$profile" || status=1
      fi
      exit "$status"
    }
    begin_transaction() {
      original_profile="$(cat "$profile")"
      original_max="$(cat "$max_perf")"
      original_min="$(cat "$min_perf")"
      committed=false
      trap restore_on_failure EXIT
    }
    commit_transaction() {
      committed=true
      trap - EXIT
    }
    case "''${1-}" in
      start)
        begin_transaction
        ${pkgs.gnugrep}/bin/grep -qw performance "$choices"
        printf 'performance\n' >"$profile"
        printf '60\n' >"$max_perf"
        printf '40\n' >"$min_perf"
        [ "$(cat "$profile")" = performance ]
        [ "$(cat "$max_perf")" = 60 ]
        [ "$(cat "$min_perf")" = 40 ]
        commit_transaction
        ;;
      stop)
        begin_transaction
        printf '100\n' >"$max_perf"
        printf '16\n' >"$min_perf"
        printf 'balanced\n' >"$profile"
        [ "$(cat "$profile")" = balanced ]
        [ "$(cat "$max_perf")" = 100 ]
        [ "$(cat "$min_perf")" = 16 ]
        commit_transaction
        ;;
      *)
        echo 'expected start or stop' >&2
        exit 2
        ;;
    esac
  '';
  generatedDeviceConfig = pkgs.writeText "korrid-${cfg.label}-host.toml" ''
    label = "${cfg.label}"

    [environment]
    DISPLAY = "${xwaylandDisplay}"
    XDG_SESSION_TYPE = "x11"

    ${lib.optionalString cfg.validation.enable ''
      [[games]]
      id = "inputd-gate"
      title = "Streaming gate"
      command = [
        "${lib.getExe pkgs.tini}",
        "--",
        "${lib.getExe' pkgs.coreutils "timeout"}",
        "--signal=TERM",
        "--kill-after=5s",
        "600",
        "${lib.getExe streamingValidationMotion}",
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
    output ${cfg.compositor.outputName} mode ${cfg.compositor.mode}
    output ${cfg.compositor.outputName} bg #000000 solid_color
    workspace "${compositorWorkspace}" output ${cfg.compositor.outputName}
    workspace "${compositorWorkspace}"
  '';
  publishWaylandSocket = pkgs.writeShellScript "korri-publish-wayland-socket" ''
    set -eu
    destination="$XDG_RUNTIME_DIR/${waylandDisplay}"
    attempt=0
    while [ "$attempt" -lt 60 ]; do
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
    stable_wayland=${lib.escapeShellArg "${gameplayRuntimeDir}/${waylandDisplay}"}
    if [ -e "$stable_wayland" ] && [ ! -L "$stable_wayland" ]; then
      echo "stable Wayland path is not a symbolic link" >&2
      exit 1
    fi
    ${pkgs.coreutils}/bin/rm -f -- "$stable_wayland" "$stable_wayland".next.*
    for socket in ${lib.escapeShellArg gameplayRuntimeDir}/wayland-[0-9]*; do
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
  waitForCompositor = pkgs.writeShellScript "korri-wait-for-compositor" ''
    set -eu
    attempt=0
    while [ "$attempt" -lt 60 ]; do
      if [ -S ${lib.escapeShellArg "${gameplayRuntimeDir}/${waylandDisplay}"} ] \
        && [ -S ${lib.escapeShellArg xwaylandSocket} ]; then
        exit 0
      fi
      attempt=$((attempt + 1))
      ${pkgs.coreutils}/bin/sleep 0.25
    done
    echo "Sway displays ${waylandDisplay} and ${xwaylandDisplay} did not become ready" >&2
    exit 1
  '';
  requireNvencRuntime = pkgs.writeShellScript "korri-require-nvenc-runtime" ''
    set -eu
    test -r /run/opengl-driver/lib/libcuda.so.1
    test -r /run/opengl-driver/lib/libnvidia-encode.so.1
  '';
  validationActions = lib.optionalAttrs cfg.validation.enable {
    workspace-next.command = [
      "${pkgs.sway-unwrapped}/bin/swaymsg"
      "-s"
      compositorControlSocket
      ''workspace "${compositorWorkspace}"; focus child; fullscreen enable; border none''
    ];
  };
  validAbsolutePath =
    path:
    lib.hasPrefix "/" path
    && path != "/"
    && !(lib.hasInfix "//" path)
    && !(lib.hasInfix "/./" path)
    && !(lib.hasSuffix "/." path)
    && !(lib.hasInfix "/../" path)
    && !(lib.hasSuffix "/.." path)
    && builtins.match ".*[[:space:]].*" path == null;
in
{
  imports = [
    (import ./korri-bundle-module.nix { inherit korri; })
    (import ./korri-input.nix { inherit korri; })
    (import ../../korrid/nixos-module.nix { inherit korri; })
  ];

  options.services.korriLinuxHost = {
    enable = lib.mkEnableOption "isolated Korri Linux input and streaming host";

    label = lib.mkOption {
      type = lib.types.strMatching "^[A-Za-z0-9._-]+$";
      default = config.networking.hostName;
      description = "Non-secret host label published by the local korrid service.";
    };

    gameplayUser = lib.mkOption {
      type = lib.types.str;
      description = "Existing untrusted gameplay user.";
    };
    gameplayUid = lib.mkOption {
      type = lib.types.ints.positive;
      description = "Exact UID of the gameplay user.";
    };
    gameplayGroup = lib.mkOption {
      type = lib.types.str;
      default = "users";
      description = "Existing primary group of the gameplay user.";
    };
    gameplayGid = lib.mkOption {
      type = lib.types.ints.positive;
      description = "Exact primary GID of the gameplay user.";
    };

    apiPort = lib.mkOption {
      type = lib.types.port;
      default = 39217;
      description = "LAN and tailnet korrid API port.";
    };
    firewallInterfaces = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = "Interfaces that can reach the korrid API port.";
    };

    deviceConfig = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Optional immutable korrid device configuration.";
    };
    storageRoot = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/korri";
    };
    privateStateRoot = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/korrid";
    };
    relays = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      description = "Ordered korrid relay URLs with no built-in public relay.";
    };
    nativePeers = lib.mkOption {
      default = [ ];
      description = "Native korrid peers used by device-side clients.";
      type = lib.types.listOf (
        lib.types.submodule {
          options = {
            label = lib.mkOption { type = lib.types.strMatching "^[A-Za-z0-9._-]+$"; };
            baseUrl = lib.mkOption { type = lib.types.str; };
            devicePublicKey = lib.mkOption { type = lib.types.str; };
            moonlightAddress = lib.mkOption { type = lib.types.str; };
          };
        }
      );
    };
    ownerBindingFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Optional public pre-signed owner binding for this Linux device.";
    };

    compositor = {
      renderDevice = lib.mkOption {
        type = lib.types.str;
        description = "Exact DRM render node used by the headless compositor.";
      };
      outputName = lib.mkOption {
        type = lib.types.strMatching "^[A-Za-z0-9._-]+$";
        default = "HEADLESS-1";
      };
      mode = lib.mkOption {
        type = lib.types.strMatching "^[1-9][0-9]*x[1-9][0-9]*@[1-9][0-9]*Hz$";
        default = "1920x1080@60Hz";
      };
      renderer = lib.mkOption {
        type = lib.types.enum [
          "gles2"
          "pixman"
        ];
        default = "gles2";
        description = "wlroots renderer for the isolated headless compositor.";
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
      sunshineGid = lib.mkOption {
        type = lib.types.ints.positive;
        default = 979;
      };
      inputSeatGid = lib.mkOption {
        type = lib.types.ints.positive;
        default = 980;
      };
    };

    sunshine = {
      package = lib.mkOption {
        type = lib.types.package;
        default = korri.packages.${system}.sunshine-korri;
        defaultText = lib.literalExpression "korri.packages.${system}.sunshine-korri";
        description = "Korri-owned patched Sunshine package.";
      };
      configDirectory = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Existing private Sunshine configuration directory.";
      };
      openFirewall = lib.mkOption {
        type = lib.types.bool;
        default = true;
      };
      encoder = lib.mkOption {
        type = lib.types.enum [
          "auto"
          "vaapi"
          "nvenc"
        ];
        default = "auto";
        description = "Sunshine encoder selected through Korri's immutable service argv.";
      };
      runtimeSettings.enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Enable the Korri Sunshine live runtime-settings protocol.";
      };
      inputSeats.enable = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Enable protected launch-scoped Sunshine controller seats.";
      };
    };

    validation.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Install the bounded input-host validation game and direct action.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion =
          let
            user = config.users.users.${cfg.gameplayUser} or { };
            group = config.users.groups.${cfg.gameplayGroup} or { };
          in
          (user.isNormalUser or false)
          && (user.uid or null) == cfg.gameplayUid
          && (user.group or null) == cfg.gameplayGroup
          && (group.gid or null) == cfg.gameplayGid;
        message = "services.korriLinuxHost gameplay identity must match an existing user and primary group exactly.";
      }
      {
        assertion = lib.all (value: value != cfg.gameplayUid && value != cfg.gameplayGid) [
          cfg.serviceIdentities.inputdUid
          cfg.serviceIdentities.controlGid
          cfg.serviceIdentities.korridUid
          cfg.serviceIdentities.korridGid
          cfg.serviceIdentities.sunshineGid
          cfg.serviceIdentities.inputSeatGid
        ];
        message = "Korri service identities must differ from the gameplay identity.";
      }
      {
        assertion =
          cfg.serviceIdentities.inputdUid != cfg.serviceIdentities.korridUid
          && cfg.serviceIdentities.controlGid != cfg.serviceIdentities.korridGid
          && cfg.serviceIdentities.controlGid != cfg.serviceIdentities.sunshineGid
          && cfg.serviceIdentities.korridGid != cfg.serviceIdentities.sunshineGid
          && cfg.serviceIdentities.inputSeatGid != cfg.serviceIdentities.inputdUid
          && cfg.serviceIdentities.inputSeatGid != cfg.serviceIdentities.controlGid
          && cfg.serviceIdentities.inputSeatGid != cfg.serviceIdentities.korridUid
          && cfg.serviceIdentities.inputSeatGid != cfg.serviceIdentities.korridGid
          && cfg.serviceIdentities.inputSeatGid != cfg.serviceIdentities.sunshineGid;
        message = "Korri service identities must remain distinct.";
      }
      {
        assertion =
          lib.getName cfg.sunshine.package == "sunshine-korri"
          && (cfg.sunshine.package.drvPath or null) == korri.packages.${system}.sunshine-korri.drvPath
          && (cfg.sunshine.package.outPath or null) == korri.packages.${system}.sunshine-korri.outPath
          && (cfg.sunshine.package.korriPatchSetSha256 or null) == sunshineApproved.patchSetSha256
          && (cfg.sunshine.package.korriBaseSunshineVersion or null) == sunshineApproved.baseSunshineVersion
          &&
            (cfg.sunshine.package.korriApprovedBaseSunshineSourceHash or null)
            == sunshineApproved.approvedBaseSourceHash
          &&
            (cfg.sunshine.package.korriReviewedLibavcodecVersion or null)
            == sunshineApproved.reviewedLibavcodecVersion
          && (cfg.sunshine.package.korriReviewedFfmpegCommit or null) == sunshineApproved.reviewedFfmpegCommit
          &&
            (cfg.sunshine.package.korriReviewedFfmpegSourceHash or null)
            == sunshineApproved.reviewedFfmpegSourceHash
          &&
            (cfg.sunshine.package.korriReviewedNvencApiMajor or null) == sunshineApproved.reviewedNvencApiMajor
          &&
            (cfg.sunshine.package.korriReviewedNvencApiMinor or null) == sunshineApproved.reviewedNvencApiMinor
          && builtins.elem (cfg.sunshine.package.korriBaseSunshineDerivation or ""
          ) sunshineApproved.approvedBaseDerivations
          &&
            (cfg.sunshine.package.korriApprovedBaseSunshineDerivation or null)
            == (cfg.sunshine.package.korriBaseSunshineDerivation or null)
          &&
            (cfg.sunshine.package.korriProvenanceRelativePath or null)
            == "share/korri/sunshine-korri/provenance"
          && builtins.elem "0015-add-korri-input-seat-event-mirror.patch" (
            cfg.sunshine.package.korriPatchNames or [ ]
          )
          && builtins.elem "0016-add-seamless-nvenc-runtime-path.patch" (
            cfg.sunshine.package.korriPatchNames or [ ]
          )
          && builtins.elem "0020-add-korrid-certificate-control.patch" (
            cfg.sunshine.package.korriPatchNames or [ ]
          );
        message = "services.korriLinuxHost must use the exact approved sunshine-korri package and provenance contract.";
      }
      {
        assertion = lib.all validAbsolutePath [
          gameplayHome
          sunshineConfig
          cfg.storageRoot
          cfg.privateStateRoot
          cfg.compositor.renderDevice
        ];
        message = "services.korriLinuxHost paths must be normalized absolute paths without whitespace.";
      }
      {
        assertion =
          let
            socketConfig = config.systemd.sockets.korri-certificate-control.socketConfig or { };
            sunshineEnvironment = config.systemd.services.sunshine.environment or { };
          in
          (socketConfig.SocketUser or null) == "root"
          && (socketConfig.SocketGroup or null) == "korrid"
          &&
            (sunshineEnvironment.KORRI_CERTIFICATE_CONTROL_OWNER_GID or null)
            == toString cfg.serviceIdentities.korridGid;
        message = "services.korriLinuxHost certificate-control socket inode ownership must remain exact root:korrid.";
      }
      {
        assertion = lib.hasPrefix "/dev/dri/" cfg.compositor.renderDevice;
        message = "services.korriLinuxHost compositor renderDevice must be under /dev/dri/.";
      }
      {
        assertion = lib.all (name: builtins.match "[A-Za-z0-9_.:-]+" name != null) cfg.firewallInterfaces;
        message = "services.korriLinuxHost firewall interface names are invalid.";
      }
    ];

    services.korriBundle.enable = true;

    services.korriLinuxInput = {
      provider = {
        enable = true;
        # Keep source nodes in /dev/input so InputPlumber's upstream watcher can
        # preserve one persistent target across hotplug. InputPlumber removes
        # uaccess and sets mode 000; the device gate proves gameplay denial.
        sunshine = {
          enableUinputAccess = true;
          serviceName = "sunshine";
          gid = cfg.serviceIdentities.sunshineGid;
        };
      };
      inputd = {
        enable = true;
        requireProvider = true;
        uid = cfg.serviceIdentities.inputdUid;
        controlGid = cfg.serviceIdentities.controlGid;
        actionUser = cfg.gameplayUser;
        actionUid = cfg.gameplayUid;
        actionGid = cfg.gameplayGid;
        actions = validationActions;
      };
    };

    services.korridLinuxDevice = {
      enable = true;
      uid = cfg.serviceIdentities.korridUid;
      gid = cfg.serviceIdentities.korridGid;
      gameplayUser = cfg.gameplayUser;
      gameplayUid = cfg.gameplayUid;
      gameplayGid = cfg.gameplayGid;
      inputdUid = cfg.serviceIdentities.inputdUid;
      controlGid = cfg.serviceIdentities.controlGid;
      inherit deviceConfig;
      address = "0.0.0.0:${toString cfg.apiPort}";
      storageRoot = cfg.storageRoot;
      privateStateRoot = cfg.privateStateRoot;
      sunshinePrivateStateRoot = sunshineConfig;
      inherit (cfg) relays nativePeers ownerBindingFile;
      inherit compositorControlDirectory certificateControlDirectory;
    };

    services.sunshine = {
      enable = true;
      autoStart = false;
      openFirewall = cfg.sunshine.openFirewall;
      package = cfg.sunshine.package;
    };
    systemd.user.services.sunshine.enable = lib.mkForce false;

    hardware.graphics = {
      enable = true;
      extraPackages = lib.mkAfter [ pkgs.intel-media-driver ];
    };

    users.users.${cfg.gameplayUser} = {
      uid = lib.mkDefault cfg.gameplayUid;
      group = lib.mkDefault cfg.gameplayGroup;
      extraGroups = lib.mkAfter [
        "render"
        "video"
      ];
    };

    systemd.tmpfiles.rules = [
      "d ${cfg.storageRoot} 0700 korrid korrid -"
      "d ${certificateControlDirectory} 0751 root korrid -"
    ];

    systemd.sockets.korri-certificate-control = {
      description = "Private Korri Sunshine certificate control socket";
      wantedBy = [ "sockets.target" ];
      before = [ "sunshine.service" ];
      requires = [ "systemd-tmpfiles-setup.service" ];
      after = [
        "systemd-tmpfiles-setup.service"
        "systemd-tmpfiles-resetup.service"
      ];
      socketConfig = {
        Accept = false;
        ListenSequentialPacket = certificateControlSocket;
        FileDescriptorName = "korri-certificate-control";
        SocketUser = "root";
        SocketGroup = "korrid";
        SocketMode = certificateControlMode;
        DirectoryMode = "0751";
        RemoveOnStop = true;
        NonBlocking = true;
        Service = "sunshine.service";
      };
    };

    networking.firewall.interfaces = lib.genAttrs cfg.firewallInterfaces (_: {
      allowedTCPPorts = [ cfg.apiPort ];
    });

    users.groups.korri-sunshine-input-seat = lib.mkIf cfg.sunshine.inputSeats.enable {
      gid = cfg.serviceIdentities.inputSeatGid;
    };

    systemd.services.korri-input-seat-receiver = lib.mkIf cfg.sunshine.inputSeats.enable {
      description = "Protected Korri Sunshine input-seat receiver";
      wantedBy = [ "multi-user.target" ];
      requires = [ "korri-bundle-selector.service" ];
      after = [
        "systemd-tmpfiles-setup-dev.service"
        "systemd-tmpfiles-resetup.service"
        "korri-bundle-selector.service"
      ];
      before = [
        "korrid.service"
        "sunshine.service"
      ];
      serviceConfig = {
        Type = "simple";
        User = "root";
        Group = "root";
        SupplementaryGroups = [ ];
        RuntimeDirectory = "korri-input-seat";
        RuntimeDirectoryMode = "0711";
        ExecStart = "${config.services.korriBundle.launcherPackage}/bin/korri-bundle-launch input-seat-receiver --runtime-dir ${inputSeatRuntimeDirectory} --control-uid ${toString cfg.serviceIdentities.korridUid} --control-gid ${toString cfg.serviceIdentities.korridGid} --sunshine-uid ${toString cfg.gameplayUid} --sunshine-gid ${toString cfg.serviceIdentities.inputSeatGid} --event-gid ${toString cfg.gameplayGid}";
        Restart = "on-failure";
        RestartSec = 1;
        UMask = "0077";
        NoNewPrivileges = true;
        CapabilityBoundingSet = [ "CAP_CHOWN" ];
        AmbientCapabilities = [ ];
        RestrictAddressFamilies = [ "AF_UNIX" ];
        PrivateTmp = true;
        PrivatePIDs = true;
        PrivateDevices = false;
        DevicePolicy = "closed";
        DeviceAllow = [ "/dev/uinput rw" ];
        ProtectSystem = "strict";
        ProtectHome = true;
        ProtectProc = "invisible";
        ProcSubset = "pid";
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        ProtectClock = true;
        ProtectHostname = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        SystemCallArchitectures = "native";
        ReadWritePaths = [ inputSeatRuntimeDirectory ];
      };
    };

    systemd.services.korri-streaming-performance-profile = lib.mkIf highRefreshPerformance {
      description = "Korri high-refresh streaming performance profile";
      wantedBy = [ "multi-user.target" ];
      before = [ "korri-compositor.service" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = "+${streamingPerformanceProfile} start";
        ExecStop = "+${streamingPerformanceProfile} stop";
      };
    };

    systemd.services.korri-compositor = {
      description = "Korri headless Sway compositor";
      wantedBy = [ "multi-user.target" ];
      wants = [
        "korrid.service"
        "sunshine.service"
      ];
      requires = [
        "user-runtime-dir@${toString cfg.gameplayUid}.service"
        "user@${toString cfg.gameplayUid}.service"
      ]
      ++ lib.optional highRefreshPerformance "korri-streaming-performance-profile.service";
      after = [
        "systemd-tmpfiles-setup.service"
        "user-runtime-dir@${toString cfg.gameplayUid}.service"
        "user@${toString cfg.gameplayUid}.service"
      ]
      ++ lib.optional highRefreshPerformance "korri-streaming-performance-profile.service";
      before = [
        "korrid.service"
        "sunshine.service"
      ];
      path = [
        pkgs.dbus
        pkgs.sway
        pkgs.xwayland
      ];
      environment = {
        HOME = gameplayHome;
        XDG_RUNTIME_DIR = gameplayRuntimeDir;
        XDG_CONFIG_HOME = "${compositorControlDirectory}/config";
        XDG_STATE_HOME = "${compositorControlDirectory}/state";
        XDG_DATA_HOME = "${compositorControlDirectory}/data";
        DBUS_SESSION_BUS_ADDRESS = "unix:path=${gameplayRuntimeDir}/bus";
        XDG_CURRENT_DESKTOP = "sway";
        SWAYSOCK = compositorControlSocket;
        WLR_BACKENDS = "headless";
        WLR_LIBINPUT_NO_DEVICES = "1";
        WLR_RENDERER = cfg.compositor.renderer;
        WLR_RENDER_DRM_DEVICE = cfg.compositor.renderDevice;
        WLR_NO_HARDWARE_CURSORS = "1";
      }
      // lib.optionalAttrs (cfg.sunshine.encoder == "nvenc") {
        GBM_BACKEND = "nvidia-drm";
        __GLX_VENDOR_LIBRARY_NAME = "nvidia";
        LD_LIBRARY_PATH = "/run/opengl-driver/lib";
      };
      serviceConfig = {
        Type = "simple";
        User = cfg.gameplayUser;
        Group = cfg.gameplayGroup;
        SupplementaryGroups = [
          "video"
          "render"
        ];
        RuntimeDirectory = "korri-compositor";
        RuntimeDirectoryMode = "0700";
        ExecStartPre = "+${cleanupCompositorSockets}";
        ExecStart = "${pkgs.sway}/bin/sway --unsupported-gpu --config ${swayConfig}";
        ExecStartPost = [
          publishWaylandSocket
          waitForCompositor
        ];
        ExecStopPost = "+${cleanupCompositorSockets}";
        Restart = "on-failure";
        RestartSec = 2;
        UMask = "0077";
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
          gameplayRuntimeDir
          "/tmp"
        ];
      };
    };

    systemd.services.korrid = {
      bindsTo = lib.mkAfter [ "korri-compositor.service" ];
      requires = lib.mkAfter (
        [ "korri-compositor.service" ]
        ++ lib.optional cfg.sunshine.inputSeats.enable "korri-input-seat-receiver.service"
      );
      after = lib.mkAfter (
        [ "korri-compositor.service" ]
        ++ lib.optional cfg.sunshine.inputSeats.enable "korri-input-seat-receiver.service"
      );
      environment = {
        KORRID_SUNSHINE_CERTIFICATE_CONTROL_SOCKET = certificateControlSocket;
        KORRID_SUNSHINE_CERTIFICATE_CONTROL_GID = toString cfg.serviceIdentities.korridGid;
        # systemd creates and listens on the activated socket. Linux therefore
        # reports root, not Sunshine's later service identity, through
        # SO_PEERCRED on the connecting side.
        KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_UID = "0";
        KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_GID = "0";
      }
      // lib.optionalAttrs cfg.sunshine.inputSeats.enable {
        KORRID_INPUT_SEAT_CONTROL_SOCKET = inputSeatControlSocket;
      };
    };

    systemd.services.sunshine = {
      description = "Sunshine stream host for Korri";
      wantedBy = [ "multi-user.target" ];
      bindsTo = [ "korri-compositor.service" ];
      requires = [
        "korri-certificate-control.socket"
        "korri-input-source-guard.service"
        "korri-compositor.service"
      ]
      ++ lib.optional cfg.sunshine.inputSeats.enable "korri-input-seat-receiver.service";
      after = [
        "korri-certificate-control.socket"
        "korri-input-source-guard.service"
        "korri-compositor.service"
        "network-online.target"
      ]
      ++ lib.optional cfg.sunshine.inputSeats.enable "korri-input-seat-receiver.service";
      wants = [ "network-online.target" ];
      environment = {
        KORRI_CERTIFICATE_CONTROL_UID = toString cfg.serviceIdentities.korridUid;
        KORRI_CERTIFICATE_CONTROL_GID = toString cfg.serviceIdentities.korridGid;
        KORRI_CERTIFICATE_CONTROL_OWNER_GID = toString cfg.serviceIdentities.korridGid;
        KORRI_CERTIFICATE_CONTROL_MODE = certificateControlMode;
        KORRI_CERTIFICATE_CONTROL_PATH = certificateControlSocket;
        DISPLAY = xwaylandDisplay;
        WAYLAND_DISPLAY = waylandDisplay;
        XDG_RUNTIME_DIR = gameplayRuntimeDir;
        XDG_SESSION_TYPE = "wayland";
        HOME = gameplayHome;
        XDG_CONFIG_HOME = "${gameplayHome}/.config";
      }
      // lib.optionalAttrs cfg.sunshine.inputSeats.enable {
        KORRI_INPUT_SEAT_MIRROR_SOCKET = inputSeatMirrorSocket;
        KORRI_INPUT_SEAT_RUNTIME_DIR = inputSeatRuntimeDirectory;
      }
      // lib.optionalAttrs cfg.sunshine.runtimeSettings.enable {
        SUNSHINE_LIVE_SETTINGS_MVP = "1";
      }
      // lib.optionalAttrs (cfg.sunshine.encoder == "nvenc") {
        LD_LIBRARY_PATH = "/run/opengl-driver/lib";
        SUNSHINE_STRICT_ENCODER = "1";
      };
      serviceConfig = {
        Type = "simple";
        User = cfg.gameplayUser;
        Group = if cfg.sunshine.inputSeats.enable then "korri-sunshine-input-seat" else cfg.gameplayGroup;
        SupplementaryGroups = [
          "video"
          "render"
        ];
        WorkingDirectory = gameplayHome;
        ExecCondition = lib.optional (cfg.sunshine.encoder == "nvenc") requireNvencRuntime;
        ExecStartPre = waitForCompositor;
        Sockets = [ "korri-certificate-control.socket" ];
        ExecStart = "${lib.getExe cfg.sunshine.package} ${sunshineConfig}/sunshine.conf log_path=/dev/null${
          lib.optionalString (cfg.sunshine.encoder != "auto") " encoder=${cfg.sunshine.encoder}"
        }";
        Restart = "on-failure";
        RestartSec = 5;
        UMask = "0077";
        NoNewPrivileges = true;
        CapabilityBoundingSet = [ ];
        AmbientCapabilities = [ ];
        PrivateTmp = false;
        PrivatePIDs = true;
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
          sunshineConfig
          "/tmp"
        ];
        InaccessiblePaths = [
          "/dev/inputplumber/sources"
          compositorControlDirectory
        ];
      };
    };

    services.udev.extraRules = lib.mkAfter (
      ''
        KERNEL=="uinput", SUBSYSTEM=="misc", TAG-="uaccess", OWNER="root", GROUP="korri-sunshine-uinput", MODE="0660", OPTIONS+="static_node=uinput"
      ''
      + lib.optionalString cfg.sunshine.inputSeats.enable ''
        SUBSYSTEM=="input", KERNEL=="event*", ATTRS{name}=="Korri Seat P[1-4]", GROUP="${cfg.gameplayGroup}", MODE="0660"
      ''
    );
  };
}
