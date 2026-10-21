# Shared GPU frequency-governor and cpuidle policy.
#
# Some Korri devices boot with `performance` on their GPU devfreq node and
# reset it on wake. Keep the load-following GPU governor applied. CPU governor
# selection and CPU/GPU frequency limits belong exclusively to limitsd.
#
# Scope discipline:
#   - The loop never writes CPU governors or frequency limits.
#   - Re-asserting every poll makes external resets (suspend/resume hooks,
#     vendor scripts) self-healing within one interval, mirroring the fan
#     module's posture toward the same wake-path interference.
#   - It may also hold specific cpuidle states disabled (`cpuIdleDisable`) to
#     work around SoC errata where a deep CPU idle state stalls a co-processor.
#     On SM8550 this is load-bearing: cpu0's state1 (cpu-sleep-0-0) stalls the
#     Adreno GMU HFI vote handshake, freezing the GPU under devfreq scaling;
#     disabling it lets the GPU run `simple_ondemand` safely (ROCKNIX PR #2876).
#
# Sandboxing note: identical to korri-fan-control — ROCKNIX guests mount /sys
# read-only at the mount level, so the loop best-effort remounts it writable
# and retries on failure; the sandbox does not reliably isolate that remount,
# and external ro flips are healed on the next iteration.
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.korri.clockGovernor;

  pollIntervalSeconds = 30;

  governorLoopScript = pkgs.writeShellScript "korri-clock-governor-loop" ''
    set -u
    export PATH=${
      lib.makeBinPath [
        pkgs.coreutils
        pkgs.gnugrep
        pkgs.util-linux
      ]
    }

    sysfs_root="''${KORRI_CLOCK_SYSFS_ROOT:-/sys}"
    max_iterations="''${KORRI_CLOCK_MAX_ITERATIONS:-0}"
    gpu_governor=${lib.escapeShellArg cfg.gpuGovernor}

    log() { echo "korri-clock-governor: $*" >&2; }

    remount_rw() {
      if [ "$sysfs_root" = "/sys" ]; then
        mount -o remount,rw /sys 2>/dev/null || true
      fi
    }

    # write_governor <governor-file> <wanted>
    # Skips the write when already correct; on failure remounts and retries
    # once (external actors can flip /sys read-only at any time).
    write_governor() {
      [ -w "$1" ] || [ -e "$1" ] || return 0
      current="$(cat "$1" 2>/dev/null || true)"
      [ "$current" = "$2" ] && return 0
      if ! printf '%s\n' "$2" > "$1" 2>/dev/null; then
        remount_rw
        if ! printf '%s\n' "$2" > "$1" 2>/dev/null; then
          log "failed to write governor '$2' to $1"
          return 0
        fi
      fi
      parent="''${1%/*}"
      log "set ''${parent##*/} governor to '$2'"
    }

    remount_rw

    iteration=0
    while :; do
      iteration=$((iteration + 1))

      # Device facts arrive as space-separated unit environment. The option
      # types admit only sysfs-safe names, so word splitting is exact.
      for node in ''${KORRI_CLOCK_GPU_NODES:-}; do
        gpu_dir="$sysfs_root/class/devfreq/$node"
        if [ -e "$gpu_dir/governor" ] && grep -qw "$gpu_governor" "$gpu_dir/available_governors" 2>/dev/null; then
          write_governor "$gpu_dir/governor" "$gpu_governor"
        fi
      done

      # Hold configured cpuidle states disabled (write 1 to their `disable`).
      # Re-asserted every poll because suspend/resume re-enables them.
      for entry in ''${KORRI_CLOCK_CPUIDLE_DISABLE:-}; do
        idle_file="$sysfs_root/devices/system/cpu/$entry/disable"
        [ -e "$idle_file" ] && write_governor "$idle_file" "1"
      done

      if [ "$max_iterations" -gt 0 ] && [ "$iteration" -ge "$max_iterations" ]; then
        break
      fi
      sleep ${toString pollIntervalSeconds}
    done
  '';
in
{
  options.services.korri.clockGovernor = {
    enable = lib.mkEnableOption "Korri GPU frequency-governor and cpuidle policy loop";

    cpuGovernor = lib.mkOption {
      type = lib.types.str;
      default = "schedutil";
      description = ''
        Hardware's default CPU governor, consumed by the limits helper.
        This loop does not write CPU governors.
      '';
    };

    gpuGovernor = lib.mkOption {
      type = lib.types.str;
      default = "simple_ondemand";
      description = "devfreq governor asserted on each listed GPU node that offers it.";
    };

    gpuDevfreqNodes = lib.mkOption {
      type = lib.types.listOf (lib.types.strMatching "^[A-Za-z0-9._:-]+$");
      default = [ ];
      example = [ "3d00000.gpu" ];
      description = ''
        devfreq node names under /sys/class/devfreq to manage. Empty means
        CPU-only; platform adapters supply their SoC's GPU node identity.
      '';
    };

    cpuIdleDisable = lib.mkOption {
      type = lib.types.listOf (lib.types.strMatching "^cpu[0-9]+/cpuidle/state[0-9]+$");
      default = [ ];
      example = [ "cpu0/cpuidle/state1" ];
      description = ''
        cpuidle states to hold disabled, each as `cpuN/cpuidle/stateM` relative
        to /sys/devices/system/cpu. The loop writes 1 to each state's `disable`
        file every poll (suspend/resume re-enables them). Used to work around
        SoC errata where a deep CPU idle state stalls a co-processor -- on
        SM8550, cpu0's state1 stalls the Adreno GMU HFI handshake and freezes
        the GPU under devfreq scaling.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.korri-clock-governor = {
      description = "Korri GPU frequency-governor and cpuidle policy (${cfg.gpuGovernor})";
      wantedBy = [ "multi-user.target" ];
      after = [ "systemd-udevd.service" ];
      environment = {
        KORRI_CLOCK_GPU_NODES = lib.concatStringsSep " " cfg.gpuDevfreqNodes;
        KORRI_CLOCK_CPUIDLE_DISABLE = lib.concatStringsSep " " cfg.cpuIdleDisable;
      };
      unitConfig = {
        StartLimitIntervalSec = 300;
        StartLimitBurst = 5;
      };
      serviceConfig = {
        Type = "simple";
        ExecStart = governorLoopScript;
        Restart = "on-failure";
        RestartSec = "5s";
        # Root: governor selection files are root-owned hardware policy.
        PrivateTmp = true;
        ProtectSystem = "strict";
        # /sys writability is governed by ProtectKernelTunables, intentionally
        # omitted: writing governor files is this unit's whole job.
        NoNewPrivileges = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        RestrictRealtime = true;
        LockPersonality = true;
        SystemCallArchitectures = "native";
      };
    };
  };
}
