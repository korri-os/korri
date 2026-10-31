#!/usr/bin/env bash
# THROWAWAY SPIKE: run main.slint live in slint-viewer (software renderer,
# winit on X11) under Xvfb, at the start state, then:
#   - capture the window twice, 1.5 s apart (the dots and the nudge move);
#   - with --snap, capture a burst while the focus moves 1 -> 2 -> 1 (a
#     throwaway Timer in the wrapper stands in for the shell's presses);
#   - measure the viewer's CPU over 10 s from /proc.
#
#   SLINT_VIEWER=... preview/live.sh WxH [--snap]        (from surfaces/zone/)
#
# Run inside: nix shell nixpkgs#xorg-server nixpkgs#xorg.xwd nixpkgs#xdotool nixpkgs#imagemagick
# Run preview/render.sh first (it writes preview/start.slint).
set -euo pipefail
cd "$(dirname "$0")/.."
SIZE=${1:-640x480}
SNAP=${2:-}
W=${SIZE%x*}
H=${SIZE#*x}
VIEWER=${SLINT_VIEWER:-slint-viewer}
PKG=$(pwd)
TMP=$(mktemp -d)
export LD_LIBRARY_PATH="$(nix eval --raw --impure --expr 'let p = import <nixpkgs> {}; in p.lib.makeLibraryPath (with p; [ libx11 libxcursor libxrandr libxi libxcb libxkbcommon fontconfig.lib ])' 2>/dev/null)"

if [ "$SNAP" = "--snap" ]; then
  cat >"$TMP/live.slint" <<EOF
import { Preview } from "$PKG/preview/start.slint";
export component Live inherits Preview {
    width: ${W}px;
    height: ${H}px;
    Timer {
        interval: 1500ms;
        running: true;
        triggered => { root.focused-card = root.focused-card == 1 ? 2 : 1; }
    }
}
EOF
else
  cat >"$TMP/live.slint" <<EOF
import { Preview } from "$PKG/preview/start.slint";
export component Live inherits Preview { width: ${W}px; height: ${H}px; }
EOF
fi

DISPLAY_NO=:61
Xvfb $DISPLAY_NO -screen 0 "${SIZE}x24" -nolisten tcp >/dev/null 2>&1 &
XVFB=$!
trap 'kill $VIEWER_PID $XVFB 2>/dev/null || true; rm -rf "$TMP"' EXIT
sleep 1
export DISPLAY=$DISPLAY_NO
unset WAYLAND_DISPLAY
SLINT_BACKEND=winit-software "$VIEWER" "$TMP/live.slint" >/dev/null 2>&1 &
VIEWER_PID=$!

if [ "$SNAP" = "--snap" ]; then
  # The focus moves at 1.5 s and 3.0 s; the snap takes 220 ms.
  sleep 2.93
  for i in $(seq -w 1 16); do xwd -root -silent >"$TMP/$i.xwd"; sleep 0.012; done
  for f in "$TMP"/*.xwd; do magick "$f" "${f%.xwd}.png"; done
  magick montage -depth 8 "$TMP"/*.png -crop "${W}x$((H * 52 / 100))+0+$((H * 17 / 100))" -tile 4x -geometry "$((W / 2))x$((H * 26 / 100))+2+2" "preview/snap-$SIZE.png"
  exit 0
fi

sleep 3
import -window root "preview/motion-$SIZE-a.png"
sleep 1.5
import -window root "preview/motion-$SIZE-b.png"
magick compare -fuzz 2% "preview/motion-$SIZE-a.png" "preview/motion-$SIZE-b.png" -compose src "preview/motion-$SIZE-diff.png" || true
TCK=$(getconf CLK_TCK)
T0=$(awk '{print $14 + $15}' /proc/$VIEWER_PID/stat)
sleep 10
T1=$(awk '{print $14 + $15}' /proc/$VIEWER_PID/stat)
echo "$SIZE: $(( (T1 - T0) * 1000 / (10 * TCK) / 10 )) % of one core over 10 s ($((T1 - T0)) ticks of $TCK/s)"
