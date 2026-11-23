#!/usr/bin/env nix-shell
#!nix-shell -i bash -p "python3.withPackages (p: [ p.fonttools p.brotli ])" coreutils gnused
# Builds index.html from index.src.html: subsets EB Garamond to Latin and
# inlines it, so the mockup opens from a checkout with no server.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fonts=$(nix build --no-link --print-out-paths nixpkgs#eb-garamond)/share/fonts/opentype
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

face() { # file, style
  pyftsubset "$fonts/$1" --unicodes="U+0020-007E,U+00A0-00FF,U+2010-2027,U+2190-2193,U+21B5" \
    --layout-features='*' --flavor=woff2 --output-file="$tmp/$1.woff2"
  printf '@font-face{font-family:"EB Garamond";font-style:%s;font-weight:400;src:url(data:font/woff2;base64,%s) format("woff2")}\n' \
    "$2" "$(base64 -w0 "$tmp/$1.woff2")"
}
{ face EBGaramond12-Regular.otf normal; face EBGaramond12-Italic.otf italic; } > "$tmp/fonts.css"

html=$(<"$here/index.src.html")
case $html in *'/*FONTS*/'*) ;; *) echo "no font marker in index.src.html" >&2; exit 1 ;; esac
printf '%s\n' "${html/'/*FONTS*/'/$(<"$tmp/fonts.css")}" > "$here/index.html"
echo "built $here/index.html ($(stat -c %s "$here/index.html") bytes)"
