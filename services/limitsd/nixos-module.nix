{ korri }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korri.limits;
  percentRange = lib.types.submodule {
    options = {
      min = lib.mkOption {
        type = lib.types.ints.between 0 100;
        default = 0;
        description = "Minimum percent of the hardware's top clock.";
      };
      max = lib.mkOption {
        type = lib.types.ints.between 1 100;
        default = 100;
        description = "Maximum percent of the hardware's top clock.";
      };
    };
  };
in
{
  options.services.korri.limits = {
    enable = lib.mkEnableOption "Korri's narrow root launch-limits helper";
    socketPath = lib.mkOption {
      type = lib.types.str;
      default = "/run/korri-limits/limits.sock";
      readOnly = true;
      description = "Private JSON-lines socket for the local korrid client.";
    };
    defaults = {
      cpu = lib.mkOption {
        type = percentRange;
        default = { };
        description = "Device-layer CPU frequency limits, per policy.";
      };
      gpu = lib.mkOption {
        type = percentRange;
        default = { };
        description = "Device-layer GPU frequency limits.";
      };
      governor = lib.mkOption {
        type = lib.types.str;
        default = config.services.korri.clockGovernor.cpuGovernor;
        description = "Device-layer CPU governor; must be available on every CPU policy.";
      };
    };
    gpuDevfreqNodes = lib.mkOption {
      type = lib.types.listOf (lib.types.strMatching "^[A-Za-z0-9._:-]+$");
      default = config.services.korri.clockGovernor.gpuDevfreqNodes;
      description = "GPU devfreq nodes, from the same device fact as the GPU governor loop.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.defaults.cpu.min <= cfg.defaults.cpu.max;
        message = "Korri CPU min percent must not exceed max.";
      }
      {
        assertion = cfg.defaults.gpu.min <= cfg.defaults.gpu.max;
        message = "Korri GPU min percent must not exceed max.";
      }
    ];
    systemd.slices.korri-games = {
      description = "Korri game sessions";
      wantedBy = [ "multi-user.target" ];
      sliceConfig = {
        CPUAccounting = true;
        CPUWeight = 100;
      };
    };
    systemd.sockets.korri-limits = {
      description = "Korri launch-limits control socket";
      wantedBy = [ "sockets.target" ];
      socketConfig = {
        ListenStream = cfg.socketPath;
        SocketUser = "root";
        SocketGroup = "korrid";
        SocketMode = "0660";
        DirectoryMode = "0755";
        RemoveOnStop = true;
        Service = "korri-limits.service";
      };
    };
    systemd.services.korri-limits = {
      description = "Korri launch-limits root helper";
      wantedBy = [ "multi-user.target" ];
      requires = [
        "korri-limits.socket"
        "korri-games.slice"
      ];
      after = [
        "korri-limits.socket"
        "korri-games.slice"
        "systemd-udevd.service"
      ];
      before = [ "korrid.service" ];
      # Hardware facts stay in environment, not executable authority. The
      # product check compares the exact service implementation across devices.
      environment = {
        LIMITSD_DEFAULTS = builtins.toJSON cfg.defaults;
        LIMITSD_GPU_NODES = lib.concatStringsSep " " cfg.gpuDevfreqNodes;
        LIMITSD_PEER_UID = toString config.services.korridLinuxDevice.uid;
      };
      path = [ pkgs.util-linux ];
      serviceConfig = {
        Type = "simple";
        User = "root";
        ExecStart = "${korri.packages.${pkgs.stdenv.hostPlatform.system}.korri-limitsd}/bin/korri-limitsd";
        Restart = "on-failure";
        RestartSec = "2s";
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ReadWritePaths = [ "/sys" ];
        # Both sysfs and the game slice are the purpose of this service. Do
        # not set ProtectKernelTunables or ProtectControlGroups. CAP_SYS_ADMIN
        # is needed only to recover /sys from external read-only remounts,
        # following clock-governor's existing remount-and-retry posture.
        CapabilityBoundingSet = [
          "CAP_SYS_ADMIN"
          "CAP_DAC_OVERRIDE"
        ];
        NoNewPrivileges = true;
        RestrictAddressFamilies = [ "AF_UNIX" ];
        RestrictSUIDSGID = true;
        RestrictRealtime = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        SystemCallArchitectures = "native";
        UMask = "0077";
      };
    };
  };
}
