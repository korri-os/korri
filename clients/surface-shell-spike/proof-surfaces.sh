#!/usr/bin/env bash
# THROWAWAY: both real packages, input, F2 round trips and 20-switch RSS trace.
set -euo pipefail
bin=$(realpath "$1")
out=$(realpath -m "$2")
root=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
display=${SHELL_PROOF_DISPLAY:-:94}
Xvfb "$display" -screen 0 1280x720x24 -nolisten tcp >"$out/xvfb.log" 2>&1 &
xvfb=$!
pid=''
cleanup() { [[ -z $pid ]] || kill "$pid" 2>/dev/null || :; kill "$xvfb" 2>/dev/null || :; }
trap cleanup EXIT
for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X${display#:} ]] && break; sleep .1; done
export DISPLAY=$display SHELL_ROOT=$root SHELL_SURFACES=zone,pico SHELL_STATS=1 SLINT_BACKEND=winit-software
unset WAYLAND_DISPLAY
window() {
  local id=''
  for _ in $(seq 200); do
    id=$(xdotool search --onlyvisible --name "^$1$" 2>/dev/null | head -1 || :)
    [[ -n $id ]] && { sleep .5; echo "$id"; return; }
    sleep .1
  done
  echo "no window $1" >&2; return 1
}
shot() { import -window "$1" "$out/$2.png"; }
key() { xdotool windowfocus --sync "$1" key "$2"; sleep .3; }
rss() { awk '/^VmRSS:/ { print $2 }' "/proc/$pid/status"; }
"$bin" 2>"$out/stats.log" & pid=$!
zone=$(window 'Zone')
shot "$zone" zone-start
sleep 10 # An interval with Zone alone, for real redraw FPS.
key "$zone" Right
shot "$zone" zone-right
key "$zone" Left
shot "$zone" zone-left
key "$zone" F2
pico=$(window Pico)
shot "$pico" pico-from-zone
key "$pico" F2
zone=$(window 'Zone')
shot "$zone" zone-from-pico
sleep 2
printf 'switch,rss_kib\n0,%s\n' "$(rss)" >"$out/rss.csv"
id=$zone
for n in $(seq 1 20); do
  key "$id" F2
  if (( n % 2 )); then title=Pico; else title=Zone; fi
  id=$(window "$title")
  printf '%s,%s\n' "$n" "$(rss)" >>"$out/rss.csv"
done
sleep 2
shot "$id" zone-after-20
printf 'settled,%s\n' "$(rss)" >>"$out/rss.csv"
kill "$pid"; wait "$pid" || :; pid=''
if grep -E 'shell: (property|QuickJS)|Error:|panicked' "$out/stats.log"; then exit 1; fi
echo 'Zone/Pico load, arrows, F2 round trip, 20 switches: PASS (RSS values in rss.csv)' | tee "$out/proof.txt"
