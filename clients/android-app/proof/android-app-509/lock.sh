# aka's turn lock for fold-509's emulator runs. Sourced.
# Waits until: no lock, k10temp below 65 C, no other emulator. Then takes
# /var/tmp/korri-turns.lock as a noclobber file "PID label time" with flock,
# and removes it at exit. Gives up after MAX_WAIT seconds.
LOCK=/var/tmp/korri-turns.lock
tctl() { for h in /sys/class/hwmon/hwmon*; do [ "$(cat "$h/name")" = k10temp ] && echo $(( $(cat "$h/temp1_input") / 1000 )); done; }
# Running emulators: the launcher and its QEMU, by their program names.
emulators() { pgrep -c -x 'emulator|qemu-system-x86|qemu-system-x86_64|qemu-system-x86_64-headless' ; }
take_turn() {
  local label=$1 host=$2 max=${MAX_WAIT:-5400} began=$SECONDS t e line
  while true; do
    t=$(tctl); e=$(emulators)
    line="$(date -u +%T) load=$(cut -d' ' -f1 /proc/loadavg) k10temp=${t}C emulators=$e"
    if [ -e "$LOCK" ]; then
      echo "$line LOCK: $(cat "$LOCK" 2>/dev/null)" >>"$host"
    elif [ "$t" -ge 65 ] || [ "$e" -gt 0 ]; then
      echo "$line NOT READY" >>"$host"
    elif ( set -o noclobber; echo "$$ $label $(date -u +%FT%TZ)" > "$LOCK" ) 2>/dev/null; then
      exec 9<"$LOCK"
      if flock -n 9; then
        HELD=1
        trap 'release_turn' EXIT
        echo "$line TAKEN: $(cat "$LOCK")" >>"$host"
        return 0
      fi
      echo "$line the lock file is ours but its flock is taken" >>"$host"
      rm -f "$LOCK"
      return 1
    fi
    if [ $(( SECONDS - began )) -ge "$max" ]; then
      echo "$line GAVE UP after ${max}s" >>"$host"
      return 2
    fi
    sleep 30
  done
}
release_turn() {
  # Only our own file: its first word is this shell's PID.
  if [ "${HELD:-0}" = 1 ] && [ "$(cut -d' ' -f1 "$LOCK" 2>/dev/null)" = "$$" ]; then rm -f "$LOCK"; fi
  HELD=0
}
