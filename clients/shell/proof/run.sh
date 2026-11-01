#!/usr/bin/env bash
# Workstation proof: the real shell window under Xvfb, over the fixture korrid.
#
#   proof/run.sh SHELL FIXTURE_KORRID OUT_DIR SCENARIO [GAMES]
#
# SHELL is a built korri-shell, FIXTURE_KORRID the fixture-korrid example.
# Needs Xvfb, xdotool and ImageMagick on PATH, and the X11 runtime libraries
# on LD_LIBRARY_PATH (the nix task sets both).
#
# Scenarios:
#   cpu      Zone idle on GAMES for 20 s at 640x480: CPU of one core, PSS, fps
#   switch   20 F2 switches between Zone and Pico: PSS and RSS after each
#   large    10,000 games (or GAMES): Zone and Pico, PSS and fps, a page of moves
#   tour     screenshots: Zone, right, left, F2 to Pico, gamepad press, the
#            overlay, back to resume, F2 to Zone
set -euo pipefail
shell=$(realpath "$1")
fixture=$(realpath "$2")
out=$(realpath -m "$3")
scenario=$4
games=${5:-12}
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$out"
work=$(mktemp -d /tmp/korri-shell-proof.XXXXXX)
display=${PROOF_DISPLAY:-:91}
pids=()
cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || :; done
  wait 2>/dev/null || :
  rm -rf "$work"
}
trap cleanup EXIT

Xvfb "$display" -screen 0 1280x720x24 -nolisten tcp >"$out/xvfb.log" 2>&1 &
pids+=($!)
for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X${display#:} ]] && break; sleep 0.1; done
export DISPLAY=$display
unset WAYLAND_DISPLAY

mkdir -m 700 "$work/credentials" "$work/cache"
mkfifo "$work/control"
"$fixture" --games "$games" --credentials "$work/credentials" <"$work/control" >"$work/fixture.out" 2>"$out/fixture.log" &
pids+=($!)
exec 7>"$work/control"
for _ in $(seq 100); do grep -q KORRID_PORTAL_ORIGIN "$work/fixture.out" 2>/dev/null && break; sleep 0.05; done
export KORRID_ADDRESS KORRID_PORTAL_ORIGIN
KORRID_ADDRESS=$(sed -n 's/^KORRID_ADDRESS=//p' "$work/fixture.out")
KORRID_PORTAL_ORIGIN=$(sed -n 's/^KORRID_PORTAL_ORIGIN=//p' "$work/fixture.out")
export CREDENTIALS_DIRECTORY=$work/credentials

# Install time: verify both packages into the cache once.
"$shell" install --packages "$root/packages" --cache "$work/cache" >"$out/install.log"

start_shell() {
  "$shell" --packages "$root/packages" --cache "$work/cache" --surfaces "$1" --placement 640x480 --stats \
    2>"$out/shell-$2.log" &
  shell_pid=$!
  pids+=("$shell_pid")
}

window() {
  local id=''
  for _ in $(seq 300); do
    id=$(xdotool search --onlyvisible --name "^$1\$" 2>/dev/null | tail -1 || :)
    [[ -n $id ]] && { sleep 0.5; echo "$id"; return; }
    sleep 0.1
  done
  echo "no window $1" >&2
  return 1
}
shot() { import -window "$1" "$out/$2.png"; }
key() { xdotool windowfocus --sync "$1" key "$2"; sleep "${3:-0.4}"; }
cpu_ticks() { awk '{print $14 + $15}' "/proc/$1/stat"; }
pss() { awk '/^Pss:/ {print $2}' "/proc/$1/smaps_rollup"; }
rss() { awk '/^VmRSS:/ {print $2}' "/proc/$1/status"; }

measure() { # measure PID SECONDS LABEL
  local ticks hz before after
  hz=$(getconf CLK_TCK)
  before=$(cpu_ticks "$1")
  sleep "$2"
  after=$(cpu_ticks "$1")
  printf '%s cpu_one_core_percent=%s pss_kib=%s rss_kib=%s\n' "$3" \
    "$(awk -v a="$after" -v b="$before" -v hz="$hz" -v s="$2" 'BEGIN {printf "%.1f", (a - b) / hz / s * 100}')" \
    "$(pss "$1")" "$(rss "$1")" | tee -a "$out/measure.txt"
}

case $scenario in
  cpu)
    start_shell zone cpu
    zone=$(window Zone)
    shot "$zone" zone-idle
    sleep 3
    measure "$shell_pid" "${PROOF_SECONDS:-20}" zone-idle-640x480
    ;;
  perf)
    start_shell zone perf
    zone=$(window Zone)
    sleep 3
    perf record -F 299 --call-graph dwarf,16384 -o "$out/perf.data" -p "$shell_pid" -- sleep 10 2>"$out/perf-record.log"
    perf report -i "$out/perf.data" --children --stdio --percent-limit 3 -g none 2>/dev/null | grep -v '^$' | head -120 >"$out/perf-report.txt"
    ;;
  switch)
    start_shell zone,pico switch
    id=$(window Zone)
    sleep 2
    printf 'switch,surface,pss_kib,rss_kib\n0,zone,%s,%s\n' "$(pss "$shell_pid")" "$(rss "$shell_pid")" >"$out/switch.csv"
    for n in $(seq 1 20); do
      key "$id" F2 0.2
      if (( n % 2 )); then title=Pico; else title=Zone; fi
      id=$(window "$title")
      sleep 1
      printf '%s,%s,%s,%s\n' "$n" "$title" "$(pss "$shell_pid")" "$(rss "$shell_pid")" >>"$out/switch.csv"
    done
    shot "$id" after-20-switches
    ;;
  large)
    start_shell zone,pico large
    zone=$(window Zone)
    shot "$zone" zone-large
    sleep 3
    measure "$shell_pid" 10 zone-large-idle
    for _ in $(seq 30); do key "$zone" Right 0.15; done
    sleep 1
    shot "$zone" zone-large-after-30-right
    measure "$shell_pid" 10 zone-large-after-moves
    key "$zone" F2 0.5
    pico=$(window Pico)
    shot "$pico" pico-large
    for _ in $(seq 30); do key "$pico" Right 0.15; done
    sleep 1
    shot "$pico" pico-large-after-30-right
    measure "$shell_pid" 10 pico-large-after-moves
    ;;
  tour)
    start_shell zone,pico tour
    zone=$(window Zone)
    shot "$zone" 1-zone
    key "$zone" Right
    shot "$zone" 2-zone-right
    key "$zone" Left
    shot "$zone" 3-zone-left
    key "$zone" F2 1
    pico=$(window Pico)
    shot "$pico" 4-pico
    echo pad >&7
    sleep 0.5
    echo "press 0x223" >&7
    sleep 0.6
    shot "$pico" 5-pico-gamepad-right
    echo "overlay on" >&7
    overlay=$(window "Pico overlay")
    shot "$overlay" 6-pico-overlay
    key "$overlay" Down
    shot "$overlay" 7-pico-overlay-down
    key "$overlay" Escape 1
    echo requests >&7
    sleep 0.3
    pico=$(window Pico)
    shot "$pico" 8-pico-resumed
    key "$pico" o 1
    echo "stored pico" >&7
    sleep 0.3
    echo "hold 0x136" >&7
    echo "hold 0x137" >&7
    sleep 1
    echo "release 0x136" >&7
    echo "release 0x137" >&7
    zone=$(window Zone)
    shot "$zone" 9-zone-after-l1-r1
    cp "$work/fixture.out" "$out/fixture-requests.txt"
    ;;
  *)
    echo "unknown scenario $scenario" >&2
    exit 2
    ;;
esac
kill "$shell_pid" 2>/dev/null || :
wait "$shell_pid" 2>/dev/null || :
grep SHELL_STATS "$out"/shell-*.log >"$out/stats.txt" || :
echo "proof $scenario: done ($out)"
