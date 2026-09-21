# Temporary native Sunshine integration. This module is not part of the Korri
# product composition; a removable streaming plugin replaces it.
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
  sunshinePackages = korri.packages.${system};
  approvedSunshinePackages = [
    sunshinePackages.sunshine-korri
  ]
  ++ lib.optional (builtins.hasAttr "sunshine-korri-v4l2m2m" sunshinePackages) sunshinePackages.sunshine-korri-v4l2m2m
  ++ lib.optional (builtins.hasAttr "sunshine-korri-rkmpp" sunshinePackages) sunshinePackages.sunshine-korri-rkmpp;
  sunshinePackageIsApproved = builtins.any (
    package:
    (cfg.sunshine.package.drvPath or null) == package.drvPath
    && (cfg.sunshine.package.outPath or null) == package.outPath
  ) approvedSunshinePackages;
  sunshineBaseBuildProfile = cfg.sunshine.package.korriBaseBuildProfile or "";
  sunshineRkmppEnabled = cfg.sunshine.package.korriRkmppEnabled or false;
  sunshineExpectedBuildProfile =
    if sunshineRkmppEnabled then
      "${system}-rkmpp"
    else if cfg.sunshine.package.korriV4l2m2mEnabled or false then
      "${system}-v4l2m2m"
    else
      sunshineBaseBuildProfile;
  sunshineExpectedPatchSetSha256 =
    if sunshineRkmppEnabled then
      sunshineApproved.rkmppPatchSetSha256
    else
      sunshineApproved.patchSetSha256;
  sunshineApprovedBaseDerivations =
    sunshineApproved.approvedBaseDerivationsByProfile.${sunshineBaseBuildProfile} or [ ];
  runtimeHome = config.users.users.${cfg.runtimeUser}.home or "/home/${cfg.runtimeUser}";
  runtimeDir = "/run/user/${toString cfg.runtimeUid}";
  inputSeatRuntimeDirectory = "/run/korri-input-seat";
  inputSeatControlSocket = "${inputSeatRuntimeDirectory}/control.sock";
  inputSeatMirrorSocket = "${inputSeatRuntimeDirectory}/sunshine-input-seat.sock";
  certificateControlDirectory = "/run/korri-certificate-control";
  certificateControlSocket = "${certificateControlDirectory}/control.sock";
  certificateControlMode = "0660";
  compositorMode = builtins.match "^([1-9][0-9]*)x([1-9][0-9]*)@([1-9][0-9]*)Hz$" cfg.compositor.mode;
  compositorWidth = builtins.elemAt compositorMode 0;
  compositorHeight = builtins.elemAt compositorMode 1;
  compositorReadiness = import ./korri-compositor-readiness.nix {
    inherit lib pkgs runtimeDir;
  };
  inherit (compositorReadiness)
    compositorControlDirectory
    compositorControlSocket
    validAbsolutePath
    waitForCompositor
    waylandDisplay
    xwaylandDisplay
    ;
  highRefreshPerformance =
    pkgs.stdenv.hostPlatform.isx86_64 && lib.toInt (builtins.elemAt compositorMode 2) >= 120;
  sunshineConfig =
    if cfg.sunshine.configDirectory == null then
      "${runtimeHome}/.config/sunshine"
    else
      cfg.sunshine.configDirectory;
  sunshineExecutable =
    if cfg.sunshine.capture == "kms" then
      "${config.security.wrapperDir}/sunshine"
    else
      lib.getExe cfg.sunshine.package;
  remoteInputIdentifiers = [
    "48879:57005:Mouse_passthrough"
    "48879:57005:Mouse_passthrough_(absolute)"
    "48879:57005:Keyboard_passthrough"
    "48879:57005:Touch_passthrough"
    "48879:57005:Pen_passthrough"
  ];
  waitForAudio = pkgs.writeShellScript "korri-wait-for-audio" ''
    set -eu
    for attempt in $(${pkgs.coreutils}/bin/seq 1 20); do
      if [ -S "${runtimeDir}/pulse/native" ] &&
        ${pkgs.coreutils}/bin/timeout 1 ${config.services.pipewire.package}/bin/pw-metadata -n default \
          | ${pkgs.gnugrep}/bin/grep -q 'Found "default" metadata'; then
        exit 0
      fi
      ${pkgs.coreutils}/bin/sleep 0.25
    done
    echo "Gameplay audio server or WirePlumber default metadata is not ready" >&2
    exit 1
  '';
  requireNvencRuntime = pkgs.writeShellScript "korri-require-nvenc-runtime" ''
    set -eu
    test -r /run/opengl-driver/lib/libcuda.so.1
    test -r /run/opengl-driver/lib/libnvidia-encode.so.1
  '';
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
in
{
  options.services.korriLinuxHost = {
    moonlightAddress = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Optional Moonlight address in authenticated endpoint announcements.";
    };
    nativePeers = lib.mkOption {
      default = [ ];
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
    compositor.remoteInput.enable = lib.mkEnableOption "Sunshine virtual pointer, keyboard and touch input on DRM";
    serviceIdentities = {
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
        default =
          if cfg.sunshine.encoder == "rkmpp" && builtins.hasAttr "sunshine-korri-rkmpp" sunshinePackages then
            sunshinePackages.sunshine-korri-rkmpp
          else if
            cfg.sunshine.encoder == "v4l2m2m" && builtins.hasAttr "sunshine-korri-v4l2m2m" sunshinePackages
          then
            sunshinePackages.sunshine-korri-v4l2m2m
          else
            sunshinePackages.sunshine-korri;
      };
      configDirectory = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
      };
      openFirewall = lib.mkOption {
        type = lib.types.bool;
        default = true;
      };
      capture = lib.mkOption {
        type = lib.types.enum [
          "auto"
          "wlr"
          "kms"
          "x11"
        ];
        default = "auto";
      };
      encoder = lib.mkOption {
        type = lib.types.enum [
          "auto"
          "vaapi"
          "nvenc"
          "v4l2m2m"
          "rkmpp"
          "software"
        ];
        default = "auto";
      };
      runtimeSettings.enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
      };
      inputSeats.enable = lib.mkOption {
        type = lib.types.bool;
        default = false;
      };
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = lib.all (value: value != cfg.runtimeUid && value != cfg.runtimeGid) [
          cfg.serviceIdentities.sunshineGid
          cfg.serviceIdentities.inputSeatGid
        ];
        message = "Korri streaming identities must differ from the runtime identity.";
      }
      {
        assertion =
          cfg.serviceIdentities.controlGid != cfg.serviceIdentities.sunshineGid
          && cfg.serviceIdentities.korridGid != cfg.serviceIdentities.sunshineGid
          && cfg.serviceIdentities.inputSeatGid != cfg.serviceIdentities.inputdUid
          && cfg.serviceIdentities.inputSeatGid != cfg.serviceIdentities.controlGid
          && cfg.serviceIdentities.inputSeatGid != cfg.serviceIdentities.korridUid
          && cfg.serviceIdentities.inputSeatGid != cfg.serviceIdentities.korridGid
          && cfg.serviceIdentities.inputSeatGid != cfg.serviceIdentities.sunshineGid;
        message = "Korri streaming identities must remain distinct.";
      }
      {
        assertion =
          lib.getName cfg.sunshine.package == "sunshine-korri"
          && sunshinePackageIsApproved
          && (cfg.sunshine.package.korriPatchSetSha256 or null) == sunshineExpectedPatchSetSha256
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
          ) sunshineApprovedBaseDerivations
          &&
            sunshineBaseBuildProfile
            == "${system}-${if cfg.sunshine.package.korriCudaEnabled or false then "cuda" else "software"}"
          && (cfg.sunshine.package.korriBuildProfile or null) == sunshineExpectedBuildProfile
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
          )
          && builtins.elem "0021-add-v4l2m2m-encoder.patch" (cfg.sunshine.package.korriPatchNames or [ ]);
        message = "services.korriLinuxHost must use the exact approved sunshine-korri package and provenance contract.";
      }
      {
        assertion = cfg.sunshine.encoder != "nvenc" || (cfg.sunshine.package.korriCudaEnabled or false);
        message = "services.korriLinuxHost sunshine encoder nvenc requires a CUDA-enabled sunshine package.";
      }
      {
        assertion =
          cfg.sunshine.encoder != "v4l2m2m" || (cfg.sunshine.package.korriV4l2m2mEnabled or false);
        message = "services.korriLinuxHost sunshine encoder v4l2m2m requires the approved V4L2 M2M Sunshine package.";
      }
      {
        assertion = cfg.sunshine.encoder != "rkmpp" || sunshineRkmppEnabled;
        message = "services.korriLinuxHost sunshine encoder rkmpp requires an RKMPP-enabled sunshine package.";
      }
      {
        assertion = lib.all validAbsolutePath [
          runtimeHome
          sunshineConfig
        ];
        message = "services.korriLinuxHost streaming paths must be normalized absolute paths without whitespace.";
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
        assertion = !cfg.compositor.remoteInput.enable || cfg.compositor.backend == "drm";
        message = "services.korriLinuxHost compositor remote input requires the DRM backend.";
      }
      {
        assertion = cfg.sunshine.capture != "kms" || cfg.compositor.backend == "drm";
        message = "services.korriLinuxHost KMS capture requires the physical DRM compositor backend.";
      }
    ];

    services.korriLinuxHost.compositor.allowedInputIdentifiers = lib.optionals cfg.compositor.remoteInput.enable remoteInputIdentifiers;
    services.korriLinuxInput.provider.sunshine = {
      enableUinputAccess = true;
      serviceName = "sunshine";
      gid = cfg.serviceIdentities.sunshineGid;
    };
    services.korridLinuxDevice = {
      sunshinePrivateStateRoot = sunshineConfig;
      certificateControlDirectory = certificateControlDirectory;
      inherit (cfg) nativePeers moonlightAddress;
    };
    services.sunshine = {
      enable = true;
      autoStart = false;
      openFirewall = cfg.sunshine.openFirewall;
      package = cfg.sunshine.package;
      capSysAdmin = cfg.sunshine.capture == "kms";
    };
    systemd.user.services.sunshine.enable = lib.mkForce false;
    hardware.graphics.extraPackages = lib.mkAfter (
      lib.optionals (
        pkgs.stdenv.hostPlatform.isx86_64
        && builtins.elem cfg.sunshine.encoder [
          "auto"
          "vaapi"
        ]
      ) [ pkgs.intel-media-driver ]
    );
    systemd.tmpfiles.rules = [
      "d ${certificateControlDirectory} 0751 root korrid -"
      "d ${sunshineConfig} 0700 ${cfg.runtimeUser} ${cfg.runtimeGroup} -"
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
        ExecStart = "${config.services.korriBundle.launcherPackage}/bin/korri-bundle-launch input-seat-receiver --runtime-dir ${inputSeatRuntimeDirectory} --control-uid ${toString cfg.serviceIdentities.korridUid} --control-gid ${toString cfg.serviceIdentities.korridGid} --sunshine-uid ${toString cfg.runtimeUid} --sunshine-gid ${toString cfg.serviceIdentities.inputSeatGid} --event-gid ${toString cfg.runtimeGid}";
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
      wants = lib.mkAfter [ "sunshine.service" ];
      requires = lib.mkAfter (
        lib.optional highRefreshPerformance "korri-streaming-performance-profile.service"
      );
      after = lib.mkAfter (
        lib.optional highRefreshPerformance "korri-streaming-performance-profile.service"
      );
      before = lib.mkAfter [ "sunshine.service" ];
      environment = lib.optionalAttrs (cfg.sunshine.encoder == "nvenc") {
        GBM_BACKEND = "nvidia-drm";
        __GLX_VENDOR_LIBRARY_NAME = "nvidia";
        LD_LIBRARY_PATH = "/run/opengl-driver/lib";
      };
    };
    systemd.services.korrid = {
      requires = lib.mkAfter (
        lib.optional cfg.sunshine.inputSeats.enable "korri-input-seat-receiver.service"
      );
      after = lib.mkAfter (
        lib.optional cfg.sunshine.inputSeats.enable "korri-input-seat-receiver.service"
      );
      environment = {
        KORRID_SUNSHINE_CERTIFICATE_CONTROL_SOCKET = certificateControlSocket;
        KORRID_SUNSHINE_CERTIFICATE_CONTROL_GID = toString cfg.serviceIdentities.korridGid;
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
      ++ lib.optional (cfg.sunshine.capture == "kms") "suid-sgid-wrappers.service"
      ++ lib.optional cfg.sunshine.inputSeats.enable "korri-input-seat-receiver.service";
      after = [
        "korri-certificate-control.socket"
        "korri-input-source-guard.service"
        "korri-compositor.service"
        "network-online.target"
      ]
      ++ lib.optional (cfg.sunshine.capture == "kms") "suid-sgid-wrappers.service"
      ++ lib.optional cfg.sunshine.inputSeats.enable "korri-input-seat-receiver.service";
      wants = [ "network-online.target" ];
      environment = {
        KORRI_CERTIFICATE_CONTROL_UID = toString cfg.serviceIdentities.korridUid;
        KORRI_CERTIFICATE_CONTROL_GID = toString cfg.serviceIdentities.korridGid;
        KORRI_CERTIFICATE_CONTROL_OWNER_GID = toString cfg.serviceIdentities.korridGid;
        KORRI_CERTIFICATE_CONTROL_MODE = certificateControlMode;
        KORRI_CERTIFICATE_CONTROL_PATH = certificateControlSocket;
        KORRI_COMPOSITOR_OUTPUT_NAME = cfg.compositor.outputName;
        KORRI_COMPOSITOR_OUTPUT_WIDTH = compositorWidth;
        KORRI_COMPOSITOR_OUTPUT_HEIGHT = compositorHeight;
        DISPLAY = xwaylandDisplay;
        WAYLAND_DISPLAY = waylandDisplay;
        XDG_RUNTIME_DIR = runtimeDir;
        XDG_SESSION_TYPE = "wayland";
        HOME = runtimeHome;
        XDG_CONFIG_HOME = "${runtimeHome}/.config";
      }
      // lib.optionalAttrs cfg.audio.enable {
        PULSE_SERVER = "unix:${runtimeDir}/pulse/native";
        DBUS_SESSION_BUS_ADDRESS = "unix:path=${runtimeDir}/bus";
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
      }
      // lib.optionalAttrs (cfg.sunshine.encoder == "rkmpp" && cfg.compositor.drmDevice != null) {
        mpp_drm_dev = cfg.compositor.drmDevice;
      }
      //
        lib.optionalAttrs
          (builtins.elem cfg.sunshine.encoder [
            "nvenc"
            "v4l2m2m"
            "rkmpp"
          ])
          {
            SUNSHINE_STRICT_ENCODER = "1";
          };
      serviceConfig = {
        Type = "simple";
        User = cfg.runtimeUser;
        Group = if cfg.sunshine.inputSeats.enable then "korri-sunshine-input-seat" else cfg.runtimeGroup;
        SupplementaryGroups = [
          "video"
          "render"
        ];
        WorkingDirectory = runtimeHome;
        ExecCondition = lib.optional (cfg.sunshine.encoder == "nvenc") requireNvencRuntime;
        ExecStartPre =
          if cfg.audio.enable then
            [
              "+${waitForCompositor}"
              waitForAudio
            ]
          else
            "+${waitForCompositor}";
        Sockets = [ "korri-certificate-control.socket" ];
        ExecStart = "${sunshineExecutable} ${sunshineConfig}/sunshine.conf log_path=/dev/null${
          lib.optionalString (cfg.sunshine.capture != "auto") " capture=${cfg.sunshine.capture}"
        }${lib.optionalString (cfg.sunshine.encoder != "auto") " encoder=${cfg.sunshine.encoder}"}";
        Restart = "on-failure";
        RestartSec = 5;
        UMask = "0077";
        NoNewPrivileges = cfg.sunshine.capture != "kms";
        CapabilityBoundingSet = lib.optionals (cfg.sunshine.capture == "kms") [
          "CAP_SETPCAP"
          "CAP_SYS_ADMIN"
        ];
        AmbientCapabilities = [ ];
        PrivateTmp = false;
        PrivatePIDs = cfg.sunshine.capture != "kms";
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
        KERNEL=="uhid", SUBSYSTEM=="misc", TAG-="uaccess", OWNER="root", GROUP="korri-sunshine-uinput", MODE="0660", OPTIONS+="static_node=uhid"
      ''
      + lib.optionalString cfg.sunshine.inputSeats.enable ''
        SUBSYSTEM=="input", KERNEL=="event*", ATTRS{name}=="Korri Seat P[1-4]", GROUP="${cfg.runtimeGroup}", MODE="0660"
      ''
    );
  };
}
