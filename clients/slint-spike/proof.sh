#!/usr/bin/env bash
# THROWAWAY SPIKE visual proof. Run through `nix run .#slint-spike-proof -- DIR`.
# Every image is an X11 capture of the real winit window (software renderer)
# on an Xvfb display. There is no window manager or compositor.
set -euo pipefail
out="${1:?usage: slint-spike-proof OUTPUT_DIR}"
mkdir -p "$out"
out="$(cd "$out" && pwd -P)"
manifest="$KORRI_ROOT/clients/slint-spike/Cargo.toml"
spike="$CARGO_TARGET_DIR/debug/korri-slint-spike"
cargo build --locked --manifest-path "$manifest"

work="$(mktemp -d)"
# The spike's scratch directories go under this run's own root.
export TMPDIR="$work/scratch"
mkdir -m 0700 "$TMPDIR"
xvfb_pid=""
app_pid=""
cleanup() {
  # Stop the app first and wait, so it removes its scratch storage while the
  # X server still exists. Losing the X connection ends the process without
  # its shutdown path.
  if [[ -n "$app_pid" ]]; then
    kill "$app_pid" 2>/dev/null || true
    for _ in $(seq 200); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.05; done
  fi
  [[ -n "$xvfb_pid" ]] && kill "$xvfb_pid" 2>/dev/null || true
  if compgen -G "$TMPDIR/korri-slint-spike-*" >/dev/null; then
    echo "WARNING: this run left scratch storage; removing it" >&2
  fi
  rm -rf "$work"
}
trap cleanup EXIT

exec 3<> "$work/displayfd"
Xvfb -displayfd 3 -screen 0 1920x1200x24 -nolisten tcp >"$work/xvfb.log" 2>&1 &
xvfb_pid=$!
for _ in $(seq 100); do
  display="$(head -n1 "$work/displayfd" 2>/dev/null || true)"
  [[ -n "$display" ]] && break
  sleep 0.05
done
[[ -n "$display" ]] || { echo "Xvfb did not start" >&2; exit 1; }
export DISPLAY=":$display"

# Wait until the app log has a line matching $1 (bounded).
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
  # No window manager: give the window real X input focus, then send keys
  # through XTest to whatever holds focus, as a keyboard would.
  xdotool windowfocus --sync "$wid"
  if [[ "$(xdotool getwindowfocus)" != "$wid" ]]; then
    echo "the spike window did not receive X input focus" >&2
    exit 1
  fi
}

size() {
  xdotool windowsize "$wid" "$1" "$2"
  sleep 0.6
}

shot() {
  local file="$out/$1.png"
  import -window "$wid" "$file"
  echo "$file $(identify -format '%wx%h' "$file")"
}

stop_app_with_keyboard() {
  # Focus starts on Refresh; Tab moves to Close; Return activates it.
  xdotool key Tab Return
  wait "$app_pid"
  app_pid=""
  grep -q 'scratch storage removed' "$work/app.log"
  cat "$work/app.log"
}

echo "== healthy run"
start_app
wait_for 'refresh 1 complete'
size 1440 900
shot 01-generous-1440x900-refresh-focused
xdotool key Tab
sleep 0.3
shot 02-generous-after-tab-key
xdotool key shift+Tab Return
wait_for 'refresh 2 complete'
sleep 0.3
shot 03-generous-return-key-refresh-check-2
# Refresh sits left of Close in the inline header: 24 px padding, two buttons.
geometry="$(xdotool getwindowgeometry --shell "$wid")"
eval "$geometry"
xdotool mousemove --window "$wid" $((WIDTH - 24 - 220 + 40)) $((24 + 22)) click 1
wait_for 'refresh 3 complete'
sleep 0.3
shot 04-generous-mouse-click-refresh-check-3
size 960 640
shot 05-medium-960x640
size 390 844
shot 06-narrow-tall-390x844
size 1280 300
shot 07-wide-short-1280x300
size 320 240
shot 08-tiny-320x240
stop_app_with_keyboard

echo "== rejected credential (real korrid, fresh random bearer)"
start_app --proof-failure=unauthorized
wait_for 'refresh 1 complete'
size 1100 700
shot 09-unauthorized-1100x700
size 390 844
shot 10-unauthorized-narrow-tall-390x844
stop_app_with_keyboard

echo "== closed port (real korrid stopped before the first refresh)"
start_app --proof-failure=offline
wait_for 'refresh 1 complete'
size 1100 700
shot 11-offline-1100x700
size 1280 300
shot 12-offline-wide-short-1280x300
stop_app_with_keyboard
if compgen -G "$TMPDIR/korri-slint-spike-*" >/dev/null; then
  echo "FAIL: this run's scratch directories remain" >&2
  exit 1
fi
echo "SLINT SPIKE PROOF DONE: $out"
