#!/usr/bin/env bash
# THROWAWAY SPIKE visual proof of the SDL adapter. Run through
# `nix run .#slint-spike-sdl-proof -- OUTPUT_DIR`. The window is an SDL window
# from upstream SDL 2.0.4 (X11 driver) on Xvfb, showing the real korrid.
# Input is XTest keys and a mouse click, as a keyboard and a pointer remote
# would send. There is no window manager or compositor.
set -euo pipefail
out="${1:?usage: slint-spike-sdl-proof OUTPUT_DIR}"
mkdir -p "$out"
out="$(cd "$out" && pwd -P)"
manifest="$KORRI_ROOT/clients/slint-spike/Cargo.toml"
spike="$CARGO_TARGET_DIR/debug/korri-slint-spike"
cargo build --locked --manifest-path "$manifest" --no-default-features --features webos
export LD_LIBRARY_PATH="$SDL_204_LIB${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

work="$(mktemp -d)"
export TMPDIR="$work/scratch"
mkdir -m 0700 "$TMPDIR"
xvfb_pid=""
app_pid=""
cleanup() {
  if [[ -n "$app_pid" ]]; then
    kill "$app_pid" 2>/dev/null || true
    for _ in $(seq 200); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.05; done
  fi
  [[ -n "$xvfb_pid" ]] && kill "$xvfb_pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

exec 3<> "$work/displayfd"
Xvfb -displayfd 3 -screen 0 1920x1080x24 -nolisten tcp >"$work/xvfb.log" 2>&1 &
xvfb_pid=$!
for _ in $(seq 100); do
  display="$(head -n1 "$work/displayfd" 2>/dev/null || true)"
  [[ -n "$display" ]] && break
  sleep 0.05
done
[[ -n "$display" ]] || { echo "Xvfb did not start" >&2; exit 1; }
export DISPLAY=":$display"

wait_for() {
  for _ in $(seq 200); do
    grep -q -- "$1" "$work/app.log" && return 0
    if ! kill -0 "$app_pid" 2>/dev/null; then
      echo "app exited while waiting for: $1" >&2
      cat "$work/app.log" >&2
      exit 1
    fi
    sleep 0.05
  done
  echo "timed out waiting for: $1" >&2
  cat "$work/app.log" >&2
  exit 1
}

start_app() {
  : >"$work/app.log"
  "$spike" "$@" >"$work/app.log" 2>&1 &
  app_pid=$!
  wid="$(xdotool search --sync --name '^Korri diagnostics$' | head -n1)"
  xdotool windowfocus --sync "$wid"
}

shot() {
  sleep 0.4
  local file="$out/$1.png"
  import -window "$wid" "$file"
  echo "$file $(identify -format '%wx%h' "$file")"
}

echo "== TV-sized window 1920x1080 (scale 1.5), upstream SDL 2.0.4"
start_app --windowed=1920x1080
wait_for 'refresh 1 complete'
shot sdl-01-1920x1080-refresh-focused
xdotool key Down
shot sdl-02-down-focuses-close
xdotool key Up Return
wait_for 'refresh 2 complete'
shot sdl-03-up-return-check-2
# Refresh centre at scale 1.5: 24 px padding, Refresh then Close (~220 logical px).
xdotool mousemove --window "$wid" $(((1280 - 24 - 220 + 52) * 3 / 2)) $(((24 + 22) * 3 / 2)) click 1
wait_for 'refresh 3 complete'
shot sdl-04-mouse-click-check-3
xdotool key Escape
wait "$app_pid"
app_pid=""
grep -q 'sdl: back: closing the inspector' "$work/app.log"
grep -q 'korrid stopped; scratch storage removed' "$work/app.log"
cat "$work/app.log"

echo "== 1280x720 window (scale 1.0)"
start_app --windowed=1280x720
wait_for 'refresh 1 complete'
shot sdl-05-1280x720
xdotool key Right Return
wait "$app_pid"
app_pid=""
grep -q 'korrid stopped; scratch storage removed' "$work/app.log"
cat "$work/app.log"

if compgen -G "$TMPDIR/korri-slint-spike-*" >/dev/null; then
  echo "FAIL: this run's scratch directories remain" >&2
  exit 1
fi
echo "SLINT SPIKE SDL PROOF DONE: $out"
