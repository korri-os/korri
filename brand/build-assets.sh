#!/usr/bin/env bash
# Generate every PWA icon from the canonical SVGs in this folder.
# Needs python3, resvg, imagemagick on PATH: run as `nix run .#brand-assets`.
# Outputs are checked in.
#
# Canonical icon: dark field, two-tone leaf (brand/korri-mark.svg on #000).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
PORTAL="$ROOT/clients/portal/public"
DARK='#000000'

python3 "$HERE/build-leaf-mark.py"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# ImageMagick writes tIME and date chunks, which would dirty every PNG on
# each run. Write no metadata so unchanged pixels mean an unchanged file.
magick() { command magick "${@:1:$#-1}" -strip -define png:exclude-chunks=date,time "${!#}"; }

# --- helpers ---------------------------------------------------------------

# leaf_png <size> <out>: transparent leaf, centred, 62% of the canvas.
leaf_png() { resvg -w "$1" -h "$1" "$HERE/korri-mark.svg" "$2"; }

# field_png <size> <bg> <out>: leaf on a solid square field.
field_png() {
  leaf_png "$1" "$tmp/leaf-$1.png"
  magick "$tmp/leaf-$1.png" -background "$2" -flatten "$3"
}

# --- portal / PWA ----------------------------------------------------------

mkdir -p "$PORTAL"

# SVG favicon: leaf on transparent; the tab chrome supplies the field. Chrome
# and Firefox use it; Safari falls back to the PNGs below.
cp "$HERE/korri-mark.svg" "$PORTAL/favicon.svg"

# Manifest icons. `any` shows the full field; `maskable` is the same artwork
# because the leaf already sits inside the 80% safe circle.
for s in 192 512; do
  field_png "$s" "$DARK" "$PORTAL/icon-$s.png"
done
field_png 180 "$DARK" "$PORTAL/apple-touch-icon.png"

# Legacy favicon.ico: 16/32/48 from the dark field so the tab reads as Korri
# in browsers that ignore the SVG.
for s in 16 32 48; do field_png "$s" "$DARK" "$tmp/fav-$s.png"; done
magick "$tmp/fav-16.png" "$tmp/fav-32.png" "$tmp/fav-48.png" "$PORTAL/favicon.ico"

echo "brand assets written to $PORTAL"
