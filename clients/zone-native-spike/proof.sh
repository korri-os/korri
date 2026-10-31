#!/usr/bin/env bash
# THROWAWAY SPIKE proof: the real X11 window under Xvfb, captured with
# ImageMagick's `import -window`. Run through `nix shell` (README, "Proof").
#
#   proof.sh BINARY OUT_DIR
#
# Writes, at 640x480 unless noted:
#   native-640x480-t0.png, native-640x480-t1.png   ~1 s apart (the nudge)
#   native-640x480-drift-a.png, -drift-b.png       ZONE_SPIKE_TIME_SCALE=20, ~1 s apart
#   native-1280x720.png                            the 720p layout
#   stats-640x480.log                              ZONE_SPIKE_STATS=1 output
set -euo pipefail
bin="$1"
out="$2"
mkdir -p "$out"
display=":97"

Xvfb "$display" -screen 0 1280x720x24 -nolisten tcp &
xvfb=$!
trap 'kill "$xvfb" 2>/dev/null || true' EXIT
for _ in $(seq 50); do
  [[ -e /tmp/.X11-unix/X${display#:} ]] && break
  sleep 0.1
done
export DISPLAY="$display"
unset WAYLAND_DISPLAY

shot() {
  # shot SIZE NAME... : start the window, wait, capture each name ~1 s apart.
  local size="$1"
  shift
  ZONE_SPIKE_SIZE="$size" WINIT_UNIX_BACKEND=x11 "$bin" &
  local pid=$!
  local id=""
  for _ in $(seq 100); do
    id="$(xdotool search --name 'Zone \(native spike\)' 2>/dev/null | head -1 || true)"
    [[ -n "$id" ]] && break
    sleep 0.1
  done
  [[ -n "$id" ]] || { echo "no window" >&2; kill "$pid"; exit 1; }
  sleep 2
  local first=1
  for name in "$@"; do
    [[ $first -eq 1 ]] || sleep 1
    first=0
    import -window "$id" "$out/$name"
  done
  # Escape quits (the one input the spike reads).
  xdotool windowfocus --sync "$id" key Escape || true
  for _ in $(seq 30); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
  kill "$pid" 2>/dev/null && echo "Escape did not quit; killed" >&2 || true
  wait "$pid" 2>/dev/null || true
}

shot 640x480 native-640x480-t0.png native-640x480-t1.png
ZONE_SPIKE_TIME_SCALE=20 shot 640x480 native-640x480-drift-a.png native-640x480-drift-b.png
shot 1280x720 native-1280x720.png
ZONE_SPIKE_STATS=1 ZONE_SPIKE_SIZE=640x480 WINIT_UNIX_BACKEND=x11 timeout 12 "$bin" 2>"$out/stats-640x480.log" || true
