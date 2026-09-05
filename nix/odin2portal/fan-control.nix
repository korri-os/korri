# Shared closed-loop fan control.
#
# Some Korri devices ship kernels whose thermal policy maps the maximum
# cooling state to a quiet PWM level (SM8550/Thor: max state = pwm 70/255),
# letting gaming loads reach ~90C while the OS believes cooling is maxed.
# This module owns a small root control loop that polls a temperature source
# and drives the fan PWM along a declarative curve. Platform adapters own the
# hardware identity facts and per-device curves; the module ships a
# conservative generic curve so a fan-equipped device is protected as soon as
# it declares its hardware identity.
#
# Safety posture:
#   - The module never touches kernel thermal zones or their critical trip
#     points; the kernel's own thermal shutdown remains the terminal backstop.
#   - `ExecStopPost` restores automatic fan mode (`pwm_enable=2`) on any stop,
#     including crashes and SIGKILL. Residual accepted gap: if the hwmon node
#     itself disappeared, restore is impossible and the last PWM holds until
#     reboot.
#   - Implausible sensor readings never lower the PWM: the loop holds the last
#     valid value and escalates to 100% after three consecutive bad reads.
#
# Sandboxing note: `ProtectSystem=strict` exempts the /sys API filesystem
# (writability of /sys is governed by ProtectKernelTunables, which this unit
# intentionally omits), so no /sys entry is needed in ReadWritePaths. ROCKNIX
# guests additionally mount /sys read-only at the mount level (verified on
# SM8550: `sysfs /sys sysfs ro`), which no unit hardening option can undo. The
# scripts therefore best-effort `mount -o remount,rw /sys` before writing.
# Observed on the SM8550 guest: the unit's sandbox does NOT reliably isolate
# this remount in a private mount namespace — an external `remount,ro /sys`
# from another shell flipped the loop's writes to EROFS mid-run. The loop
# therefore re-attempts the remount and retries the write on every failed
# iteration rather than trusting the startup remount.
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.korri.fanControl;

  inherit (lib)
    concatMapStringsSep
    mkEnableOption
    mkIf
    mkOption
    optional
    types
    ;

  # Internal constants (deliberately not options: no consumer needs to vary
  # them yet, and a small public surface is easier to keep stable).
  pollIntervalSeconds = 5;
  hysteresisMilliC = 2000;
  discoveryAttempts = 30;
  invalidReadingStrikes = 3;
  minPlausibleMilliC = 1000;
  maxPlausibleMilliC = 120000;
  statusDir = "/run/korri-fan-control";
  statusFile = "${statusDir}/status.json";

  pwmPercentToRaw = percent: ((percent * 255) + 50) / 100;

  curveTempsMilliC = map (point: toString (point.tempC * 1000)) cfg.curve;
  curvePwmsRaw = map (point: toString (pwmPercentToRaw point.pwmPercent)) cfg.curve;

  # Neutral fallback keeps warning evaluation lazy-safe when the curve is
  # empty; the non-empty assertion is the authoritative failure in that case.
  lastPoint =
    if cfg.curve == [ ] then
      {
        tempC = 85;
        pwmPercent = 100;
      }
    else
      lib.last cfg.curve;

  tempSourceKind = if cfg.tempSource == null then "" else cfg.tempSource.kind;

  restoreScript = pkgs.writeShellScript "korri-fan-control-restore" ''
    # Best-effort restore of automatic fan control. Runs from ExecStopPost on
    # every service stop (clean stop, crash, SIGKILL) and from the loop's own
    # exit trap. Must never fail the unit.
    set -u
    export PATH=${
      lib.makeBinPath [
        pkgs.coreutils
        pkgs.util-linux
      ]
    }

    sysfs_root="''${KORRI_FAN_SYSFS_ROOT:-/sys}"
    hwmon_name=${lib.escapeShellArg cfg.hwmonName}

    if [ "$sysfs_root" = "/sys" ]; then
      # Guest kernels can mount /sys read-only; remount inside this unit's
      # private mount namespace only. No-op when /sys is already writable.
      mount -o remount,rw /sys 2>/dev/null || true
    fi

    for dir in "$sysfs_root"/class/hwmon/hwmon*; do
      [ -r "$dir/name" ] || continue
      [ "$(cat "$dir/name")" = "$hwmon_name" ] || continue
      if [ -e "$dir/pwm1_enable" ]; then
        printf '2\n' > "$dir/pwm1_enable" 2>/dev/null || true
      fi
    done
    exit 0
  '';

  controlLoopScript = pkgs.writeShellScript "korri-fan-control-loop" ''
    # Closed-loop fan control. Test contract:
    #   KORRI_FAN_SYSFS_ROOT     sysfs root (default /sys) so checks can run
    #                            this script against a mock tree.
    #   KORRI_FAN_MAX_ITERATIONS run N loop iterations then exit 0 (default
    #                            0 = run forever). Iterations sleep only when
    #                            running unbounded.
    set -u
    export PATH=${
      lib.makeBinPath [
        pkgs.coreutils
        pkgs.util-linux
      ]
    }

    sysfs_root="''${KORRI_FAN_SYSFS_ROOT:-/sys}"
    max_iterations="''${KORRI_FAN_MAX_ITERATIONS:-0}"

    if [ "$sysfs_root" = "/sys" ]; then
      # Guest kernels can mount /sys read-only; remount inside this unit's
      # private mount namespace only. No-op when /sys is already writable.
      mount -o remount,rw /sys 2>/dev/null || true
    fi

    hwmon_name=${lib.escapeShellArg cfg.hwmonName}
    profile_name=${lib.escapeShellArg cfg.profileName}
    status_file="''${KORRI_FAN_STATUS_FILE:-${statusFile}}"
    idle_pwm=${toString (pwmPercentToRaw cfg.idlePwmPercent)}
    temp_source_kind=${lib.escapeShellArg tempSourceKind}
    temp_channel=${toString (if cfg.tempSource == null then 1 else cfg.tempSource.channel)}
    temp_zone_type=${lib.escapeShellArg (
      if cfg.tempSource == null || cfg.tempSource.zoneType == null then "" else cfg.tempSource.zoneType
    )}

    curve_temps=(${lib.concatStringsSep " " curveTempsMilliC})
    curve_pwms=(${lib.concatStringsSep " " curvePwmsRaw})
    curve_len=''${#curve_temps[@]}

    log() { echo "korri-fan-control: $*" >&2; }

    write_status() {
      # write_status <state> <temp-milli-or-null> <pwm-or-null>
      rpm=null
      if [ -n "''${fan_dir:-}" ] && [ -r "$fan_dir/fan1_input" ]; then
        rpm_raw="$(cat "$fan_dir/fan1_input" 2>/dev/null || true)"
        case "$rpm_raw" in
          "" | *[!0-9]*) rpm=null ;;
          *) rpm="$rpm_raw" ;;
        esac
      fi
      tmp="$status_file.tmp.$$"
      printf '{"profile":"%s","state":"%s","temperatureMilliC":%s,"pwm":%s,"rpm":%s}\n' \
        "$profile_name" "$1" "$2" "$3" "$rpm" > "$tmp" 2>/dev/null || return 0
      mv -f "$tmp" "$status_file" 2>/dev/null || rm -f "$tmp" 2>/dev/null || true
    }

    # --- Identity-based discovery (never by index) -------------------------
    find_by_identity() {
      # find_by_identity <class-glob-dir> <attr-file> <wanted> -> matches array
      matches=()
      for dir in "$sysfs_root"/class/$1/$2*; do
        [ -r "$dir/$3" ] || continue
        [ "$(cat "$dir/$3")" = "$4" ] || continue
        matches+=("$dir")
      done
    }

    fan_dir=""
    attempt=0
    while [ "$attempt" -lt ${toString discoveryAttempts} ]; do
      attempt=$((attempt + 1))
      find_by_identity hwmon hwmon name "$hwmon_name"
      if [ "''${#matches[@]}" -gt 1 ]; then
        log "multiple hwmon devices named '$hwmon_name': ''${matches[*]} — refusing to guess"
        write_status "ambiguous-hwmon" null null
        exit 1
      fi
      if [ "''${#matches[@]}" -eq 1 ]; then
        fan_dir="''${matches[0]}"
        break
      fi
      [ "$max_iterations" -gt 0 ] && break
      sleep 1
    done

    if [ -z "$fan_dir" ]; then
      log "no hwmon device named '$hwmon_name' found — treating as fanless hardware (no-op)"
      write_status "no-fan-hardware" null null
      exit 0
    fi

    if [ ! -e "$fan_dir/pwm1" ]; then
      log "hwmon '$hwmon_name' at $fan_dir has no pwm1 control — no-op"
      write_status "no-pwm-control" null null
      exit 0
    fi

    case "$temp_source_kind" in
      hwmon-channel)
        temp_path="$fan_dir/temp''${temp_channel}_input"
        ;;
      thermal-zone)
        find_by_identity thermal thermal_zone type "$temp_zone_type"
        if [ "''${#matches[@]}" -gt 1 ]; then
          log "multiple thermal zones of type '$temp_zone_type': ''${matches[*]} — refusing to guess"
          write_status "ambiguous-temp-source" null null
          exit 1
        fi
        if [ "''${#matches[@]}" -eq 0 ]; then
          log "no thermal zone of type '$temp_zone_type' found"
          write_status "missing-temp-source" null null
          exit 1
        fi
        temp_path="''${matches[0]}/temp"
        ;;
      *)
        log "unsupported temp source kind '$temp_source_kind'"
        exit 1
        ;;
    esac

    if [ ! -r "$temp_path" ]; then
      log "temperature source $temp_path is not readable"
      write_status "missing-temp-source" null null
      exit 1
    fi

    # --- Curve evaluation ---------------------------------------------------
    compute_pwm() {
      # compute_pwm <temp-milli> -> echoes raw pwm 0-255
      t="$1"
      if [ "$t" -lt "''${curve_temps[0]}" ]; then
        echo "$idle_pwm"
        return
      fi
      last=$((curve_len - 1))
      if [ "$t" -ge "''${curve_temps[$last]}" ]; then
        echo "''${curve_pwms[$last]}"
        return
      fi
      i=0
      while [ $((i + 1)) -le "$last" ]; do
        t0="''${curve_temps[$i]}"
        t1="''${curve_temps[$((i + 1))]}"
        if [ "$t" -ge "$t0" ] && [ "$t" -lt "$t1" ]; then
          p0="''${curve_pwms[$i]}"
          p1="''${curve_pwms[$((i + 1))]}"
          span=$((t1 - t0))
          # Linear interpolation with round-to-nearest integer PWM.
          echo $(((p0 * span + (p1 - p0) * (t - t0) + span / 2) / span))
          return
        fi
        i=$((i + 1))
      done
      echo "''${curve_pwms[$last]}"
    }

    # Restore automatic mode when an unbounded loop exits for any reason.
    # Bounded (test-mode) runs skip the trap so checks can observe the loop's
    # end state; production stops are additionally covered by ExecStopPost.
    if [ "$max_iterations" -eq 0 ]; then
      trap '${restoreScript} || true' EXIT
    fi

    # --- Control loop ---------------------------------------------------------
    last_pwm=""
    anchor_temp=""
    invalid_count=0
    iteration=0
    while :; do
      iteration=$((iteration + 1))

      # Re-assert manual mode every iteration: any external actor (host
      # thermal policy, suspend/resume hooks) that flips the fan back to
      # automatic is corrected within one poll.
      if [ -e "$fan_dir/pwm1_enable" ]; then
        printf '1\n' > "$fan_dir/pwm1_enable" 2>/dev/null || true
      fi

      raw="$(cat "$temp_path" 2>/dev/null || true)"
      valid=1
      case "$raw" in
        "" | *[!0-9-]*) valid=0 ;;
        *)
          if [ "$raw" -lt ${toString minPlausibleMilliC} ] || [ "$raw" -gt ${toString maxPlausibleMilliC} ]; then
            valid=0
          fi
          ;;
      esac

      if [ "$valid" -eq 0 ]; then
        invalid_count=$((invalid_count + 1))
        log "implausible temperature reading '$raw' ($invalid_count consecutive)"
        if [ "$invalid_count" -ge ${toString invalidReadingStrikes} ]; then
          # Safety escalation: never leave the fan slow on garbage input.
          target=255
        else
          # Hold the last valid PWM; skip the write entirely if we have none.
          target="$last_pwm"
        fi
        temp_json=null
      else
        invalid_count=0
        temp_json="$raw"
        # Hysteresis: recompute only when the temperature moved far enough
        # from the reading that set the current PWM, to stop audible
        # oscillation at curve knees.
        recompute=1
        if [ -n "$anchor_temp" ]; then
          delta=$((raw - anchor_temp))
          [ "$delta" -lt 0 ] && delta=$((-delta))
          [ "$delta" -lt ${toString hysteresisMilliC} ] && recompute=0
        fi
        if [ "$recompute" -eq 1 ] || [ -z "$last_pwm" ]; then
          target="$(compute_pwm "$raw")"
          anchor_temp="$raw"
        else
          target="$last_pwm"
        fi
      fi

      if [ -n "$target" ]; then
        if printf '%s\n' "$target" > "$fan_dir/pwm1" 2>/dev/null; then
          last_pwm="$target"
        else
          # Self-heal: an external actor can flip /sys back to read-only at
          # any time (observed live: an interactive `mount -o remount,ro
          # /sys` froze the loop's writes). Re-attempt the remount and retry
          # once per iteration instead of trusting the startup remount.
          if [ "$sysfs_root" = "/sys" ]; then
            mount -o remount,rw /sys 2>/dev/null || true
          fi
          if printf '%s\n' "$target" > "$fan_dir/pwm1" 2>/dev/null; then
            printf '1\n' > "$fan_dir/pwm1_enable" 2>/dev/null || true
            last_pwm="$target"
          else
            log "failed to write pwm value $target to $fan_dir/pwm1"
          fi
        fi
      fi

      write_status "controlling" "$temp_json" "''${last_pwm:-null}"

      if [ "$max_iterations" -gt 0 ] && [ "$iteration" -ge "$max_iterations" ]; then
        exit 0
      fi
      [ "$max_iterations" -gt 0 ] || sleep ${toString pollIntervalSeconds}
    done
  '';
in
{
  key = "korri-fan-control";

  options.services.korri.fanControl = {
    enable = mkEnableOption "Korri closed-loop fan control";

    hwmonName = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "pwmfan";
      description = ''
        hwmon device identity to control, matched against
        /sys/class/hwmon/hwmonN/name. Discovery is identity-based because
        hwmon indexes shift across reboots. Required when enabled; if multiple
        devices share this name the service fails loudly rather than guessing.
      '';
    };

    tempSource = mkOption {
      type = types.nullOr (
        types.submodule {
          options = {
            kind = mkOption {
              type = types.enum [
                "hwmon-channel"
                "thermal-zone"
              ];
              description = ''
                Where temperature comes from: a tempN_input channel on the
                resolved fan hwmon, or a thermal zone matched by its type
                attribute. Both are identity-based; indexes are never used.
              '';
            };

            channel = mkOption {
              type = types.ints.positive;
              default = 1;
              description = "tempN_input channel number on the fan hwmon (hwmon-channel kind).";
            };

            zoneType = mkOption {
              type = types.nullOr types.str;
              default = null;
              example = "cpu-thermal";
              description = "Thermal zone type to match against /sys/class/thermal/*/type (thermal-zone kind).";
            };
          };
        }
      );
      default = null;
      description = "Identity-based temperature source. Required when enabled.";
    };

    curve = mkOption {
      type = types.listOf (
        types.submodule {
          options = {
            tempC = mkOption {
              type = types.int;
              description = "Temperature threshold in degrees Celsius.";
            };
            pwmPercent = mkOption {
              type = types.ints.between 0 100;
              description = "Fan duty cycle at this temperature, as a percentage.";
            };
          };
        }
      );
      default = [
        {
          tempC = 45;
          pwmPercent = 30;
        }
        {
          tempC = 60;
          pwmPercent = 50;
        }
        {
          tempC = 75;
          pwmPercent = 75;
        }
        {
          tempC = 85;
          pwmPercent = 100;
        }
      ];
      description = ''
        Temperature-to-PWM curve, sorted strictly ascending by tempC. Values
        between points are linearly interpolated; below the first point the
        idle floor applies; above the last point the last PWM is held. The
        default is a conservative generic curve so any fan-equipped device is
        protected once it declares hardware identity; devices override with
        their own tuned curve.
      '';
    };

    idlePwmPercent = mkOption {
      type = types.ints.between 0 100;
      default = 0;
      description = "Fan duty below the first curve point (idle floor).";
    };

    profileName = mkOption {
      type = types.str;
      default = "generic";
      description = "Profile label reported in runtime telemetry (generic vs a device-specific override).";
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.hwmonName != null && cfg.hwmonName != "";
        message = "services.korri.fanControl.hwmonName must be set when fan control is enabled.";
      }
      {
        assertion = cfg.tempSource != null;
        message = "services.korri.fanControl.tempSource must be set when fan control is enabled.";
      }
      {
        assertion =
          cfg.tempSource == null
          || cfg.tempSource.kind != "thermal-zone"
          || (cfg.tempSource.zoneType != null && cfg.tempSource.zoneType != "");
        message = "services.korri.fanControl.tempSource.zoneType must be set for thermal-zone temp sources.";
      }
      {
        assertion = cfg.curve != [ ];
        message = "services.korri.fanControl.curve must contain at least one point when enabled.";
      }
      {
        assertion =
          let
            temps = map (point: point.tempC) cfg.curve;
            pairs = lib.zipLists temps (lib.drop 1 temps);
          in
          lib.all (pair: pair.fst < pair.snd) pairs;
        message = "services.korri.fanControl.curve must be sorted strictly ascending by tempC (no duplicates).";
      }
    ];

    warnings =
      optional (lastPoint.pwmPercent != 100)
        "services.korri.fanControl.curve never reaches 100% PWM; the fan cannot fully engage under worst-case load."
      ++ optional (lastPoint.tempC < 85)
        "services.korri.fanControl.curve tops out below 85C; verify the device cannot exceed that under load.";

    systemd.tmpfiles.rules = [ "d ${statusDir} 0755 root root -" ];

    systemd.services.korri-fan-control = {
      description = "Korri closed-loop fan control (${cfg.profileName})";
      wantedBy = [ "multi-user.target" ];
      after = [ "systemd-udevd.service" ];
      unitConfig = {
        # Bounded restart budget: a persistently failing loop must hand
        # control back to the kernel (via ExecStopPost) instead of flapping.
        StartLimitIntervalSec = 300;
        StartLimitBurst = 5;
      };
      serviceConfig = {
        Type = "simple";
        ExecStart = controlLoopScript;
        # ExecStopPost runs on every stop path, including crashes and SIGKILL:
        # the fan must never stay pinned at a manual level without a live loop.
        ExecStopPost = restoreScript;
        Restart = "on-failure";
        RestartSec = "5s";
        # Runs as root: hwmon PWM nodes are root-owned hardware controls.
        PrivateTmp = true;
        ProtectSystem = "strict";
        # /sys is exempt from ProtectSystem=strict; its writability is
        # governed by ProtectKernelTunables, which is intentionally omitted
        # because writing hwmon PWM controls is this unit's whole job.
        ReadWritePaths = [ statusDir ];
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
