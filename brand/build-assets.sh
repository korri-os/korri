#!/usr/bin/env bash
# Generate every PWA and Android icon from the canonical SVGs in this folder.
# Needs python3, resvg, imagemagick on PATH: run as `nix run .#brand-assets`.
# Outputs are checked in.
#
# Canonical icon: dark field, two-tone leaf (brand/korri-mark.svg on #000).
# Light variant: white field, same leaf; used only where a colour-scheme seam
# exists (SVG favicon media query, Android values-night splash).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
PORTAL="$ROOT/clients/portal/public"
RES="$ROOT/clients/android/app/src/main/res"
DARK='#000000'
LIGHT='#FDFDFD'
GREEN='#7EFD3B'

python3 "$HERE/build-leaf-mark.py"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

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

# --- android ---------------------------------------------------------------

# Adaptive icon foreground: 108dp canvas, leaf inside the 66dp safe square
# (61% of the canvas). korri-mark.svg places the leaf at 62%, so scale it to
# 56% here to leave air inside the safe zone.
fg_svg="$tmp/foreground.svg"
python3 - "$HERE/korri-mark.svg" "$fg_svg" <<'PY'
import re, sys
src = open(sys.argv[1]).read()
# Re-centre the 1024 mark on a 1024 canvas at 56/62 scale.
k = 0.56 / 0.62
inner = re.search(r"<g transform=\"([^\"]+)\">(.*)</g>", src, re.S)
open(sys.argv[2], "w").write(
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">'
    f'<g transform="translate({512 * (1 - k):.3f} {512 * (1 - k):.3f}) scale({k:.6f})">'
    f'<g transform="{inner.group(1)}">{inner.group(2)}</g></g></svg>')
PY
mono_svg="$tmp/monochrome.svg"
sed 's/currentColor/#000000/' "$HERE/korri-mark-mono.svg" > "$mono_svg"

declare -A DPI=( [mdpi]=1 [hdpi]=1.5 [xhdpi]=2 [xxhdpi]=3 [xxxhdpi]=4 )
for d in "${!DPI[@]}"; do
  f=${DPI[$d]}
  fg=$(python3 -c "print(round(108*$f))")
  legacy=$(python3 -c "print(round(48*$f))")
  mkdir -p "$RES/mipmap-$d"
  resvg -w "$fg" -h "$fg" "$fg_svg" "$RES/mipmap-$d/ic_launcher_foreground.png"
  resvg -w "$fg" -h "$fg" "$mono_svg" "$RES/mipmap-$d/ic_launcher_monochrome.png"
  # Pre-26 launchers: flat dark tile with rounded corners.
  field_png "$legacy" "$DARK" "$tmp/legacy-$legacy.png"
  r=$(python3 -c "print(round($legacy*0.18))")
  magick "$tmp/legacy-$legacy.png" \
    \( +clone -alpha extract -draw "fill black polygon 0,0 0,$r $r,0 fill white circle $r,$r $r,0" \
       \( +clone -flip \) -compose Multiply -composite \( +clone -flop \) -compose Multiply -composite \) \
    -alpha off -compose CopyOpacity -composite "$RES/mipmap-$d/ic_launcher.png"
done

# Play Store / web listing icon.
field_png 512 "$DARK" "$ROOT/clients/android/app/src/main/ic_launcher-web.png"

# Android TV banner 320x180 at xhdpi: dark field, wordmark centred.
resvg -w 240 "$HERE/korri-wordmark-dark.svg" "$tmp/banner-word.png"
magick -size 320x180 "xc:$DARK" "$tmp/banner-word.png" -gravity center -composite \
  "$RES/drawable-xhdpi/atv_banner.png"

# Splash-screen icon (Android 12+): 288dp canvas, icon inside the 192dp circle.
# Keep 62% (≈178dp) so it clears the circle with a little air. One drawable;
# the window background flips with values-night.
mkdir -p "$RES/drawable"
cp "$HERE/korri-mark.svg" "$tmp/splash.svg"
resvg -w 288 -h 288 "$tmp/splash.svg" "$RES/drawable/splash_icon.png"

echo "brand assets written to $PORTAL and $RES"
