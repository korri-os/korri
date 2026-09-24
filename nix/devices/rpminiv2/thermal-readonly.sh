#!/bin/sh
# One bounded snapshot. No writes to thermal, PWM, or power-supply sysfs.
set -u
sys_root=${1:-/sys}
proc_root=${2:-/proc}

show() {
  [ -r "$1" ] || return 0
  IFS= read -r value < "$1" || [ -n "${value:-}" ] || return 0
  printf '%s=%s\n' "$1" "$value"
}

printf 'rpminiv2 thermal snapshot (kernel %s)\n' "$(uname -r)"
show "$proc_root/uptime"
show "$proc_root/loadavg"
show "$proc_root/pressure/cpu"

zones=0
for zone in "$sys_root"/class/thermal/thermal_zone*; do
  [ -r "$zone/type" ] || continue
  zones=$((zones + 1))
  show "$zone/type"
  show "$zone/temp"
  for trip in "$zone"/trip_point_*_type "$zone"/trip_point_*_temp; do
    show "$trip"
  done
done
printf 'thermal_zones=%s\n' "$zones"

for cooler in "$sys_root"/class/thermal/cooling_device*; do
  [ -r "$cooler/type" ] || continue
  for field in type cur_state max_state; do show "$cooler/$field"; done
done
for monitor in "$sys_root"/class/hwmon/hwmon*; do
  [ -r "$monitor/name" ] || continue
  for field in name pwm1 pwm1_enable; do show "$monitor/$field"; done
done
for policy in "$sys_root"/devices/system/cpu/cpufreq/policy*; do
  [ -r "$policy/scaling_governor" ] || continue
  for field in scaling_governor scaling_cur_freq scaling_max_freq; do show "$policy/$field"; done
done
for supply in "$sys_root"/class/power_supply/*; do
  [ -r "$supply/type" ] || continue
  for field in type status online capacity temp; do show "$supply/$field"; done
done
# comm is the short executable name, not argv (which can hold credentials).
printf 'top process CPU (lifetime average, not instantaneous):\n'
ps -eo pid,comm,pcpu --sort=-pcpu | head -n 7
