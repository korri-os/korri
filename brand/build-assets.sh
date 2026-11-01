#!/usr/bin/env bash
# Generate the app icons from the canonical SVGs in this folder.
# Needs python3, resvg, imagemagick on PATH: run as `nix run .#brand-assets`.
# Outputs are checked in.
#
# Canonical icon: dark field, two-tone leaf (brand/korri-mark.svg on #000).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICONS="$HERE/icons"
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

# --- app icons -------------------------------------------------------------

mkdir -p "$ICONS"

# Launcher icons (the webOS package uses icon-192.png): the full dark field.
for s in 192 512; do
  field_png "$s" "$DARK" "$ICONS/icon-$s.png"
done

echo "brand assets written to $ICONS"
