#!/usr/bin/env bash
# THROWAWAY SPIKE preview renders, without the shell (README.md, "Proof").
#
#   SLINT_VIEWER=/path/to/slint-viewer-1.18.1 preview/render.sh     (from surfaces/zone/)
#
# Needs bun, slint-viewer 1.18.1 (software renderer) and ImageMagick
# (`nix shell nixpkgs#imagemagick`). Writes preview/*.png.
set -euo pipefail
cd "$(dirname "$0")/.."
BUN=${BUN:-bun}
VIEWER=${SLINT_VIEWER:-slint-viewer}
REF=../../../zone-native-spike/evidence

"$BUN" build logic.ts --format=iife --target=browser --outfile=logic.js >/dev/null
"$BUN" preview/run-logic.ts --write >/dev/null
"$BUN" preview/gen-preview.ts >/dev/null

for step in start right-1 right-2; do
  for size in 640x480 1280x720; do
    "$VIEWER" "preview/$step.slint" --screenshot "preview/$step-$size.png" --size "$size"
  done
done

# The cursor's other places, 640x480, in one sheet.
for step in up-right menu-menu down right-2-down-right; do
  "$VIEWER" "preview/$step.slint" --screenshot "/tmp/zone-focus-$step.png" --size 640x480
done
magick /tmp/zone-focus-up-right.png /tmp/zone-focus-menu-menu.png +append /tmp/zone-focus-top.png
magick /tmp/zone-focus-down.png /tmp/zone-focus-right-2-down-right.png +append /tmp/zone-focus-bottom.png
magick /tmp/zone-focus-top.png /tmp/zone-focus-bottom.png -append preview/focus-640x480.png

# Reference (Caliper, Zone in Chromium) left, this package right.
for size in 640x480 1280x720; do
  magick "$REF/caliper-reference-$size.png" "preview/start-$size.png" +append "preview/side-by-side-$size.png"
done
