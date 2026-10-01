#!/usr/bin/env nix-shell
#! nix-shell -i bash -p curl jq unzip coreutils
# Download the source files of Pico's faces into <repo>/.tmp/pico-font-sources
# and check each against its pinned checksum. Only extract-font-tables.py reads
# them; the repository keeps the extracted glyph tables, not these files.
#
#   ./scripts/fetch-font-sources.sh      # from surfaces/pico
#
# A checksum mismatch stops the script: a changed upstream file is a decision to
# review, not an update to take silently.
set -euo pipefail
repo="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
out="$repo/.tmp/pico-font-sources"
mkdir -p "$out"
cd "$out"

check() {
  local file="$1" sha="$2"
  echo "$sha  $file" | sha256sum --check --quiet || { echo "checksum mismatch: $file" >&2; exit 1; }
  echo "ok $file"
}

get() {
  local url="$1" file="$2" sha="$3"
  [ -s "$file" ] || curl -fsSL --max-time 60 -o "$file" "$url"
  check "$file" "$sha"
}

# itch.io's free download: page -> download page -> signed file URL.
itch() {
  local page="$1" want="$2" file="$3" sha="$4"
  if [ ! -s "$file" ]; then
    local jar csrf dl html url ids names id
    jar="$(mktemp)"
    csrf="$(curl -fsSL -c "$jar" -b "$jar" "$page" | grep -o 'name="csrf_token" value="[^"]*"' | head -1 | sed 's/.*value="//; s/"$//')"
    dl="$(curl -fsSL -c "$jar" -b "$jar" -X POST --data-urlencode "csrf_token=$csrf" "$page/download_url" | jq -r .url)"
    html="$(curl -fsSL -c "$jar" -b "$jar" "$dl")"
    mapfile -t ids < <(printf '%s' "$html" | grep -o 'data-upload_id="[0-9]*"' | grep -o '[0-9][0-9]*')
    mapfile -t names < <(printf '%s' "$html" | grep -o 'class="name"[^>]*>[^<]*' | sed 's/.*>//')
    id="${ids[0]}"
    for i in "${!names[@]}"; do [ "${names[$i]}" = "$want" ] && id="${ids[$i]}"; done
    url="$(curl -sS -c "$jar" -b "$jar" -X POST -H 'X-Requested-With: XMLHttpRequest' -H "Referer: $dl" \
      --data-urlencode "csrf_token=$csrf" \
      "$page/file/$id?source=game_download&as_props=1&after_download_lightbox=true" | jq -r '.url // "null"')"
    rm -f "$jar"
    [ "$url" != null ] || { echo "itch.io gave no URL for $file" >&2; exit 1; }
    curl -fsSL --max-time 60 -o "$file" "$url"
  fi
  check "$file" "$sha"
}

u8g2=https://raw.githubusercontent.com/olikraus/u8g2/master/tools/font/bdf
get "$u8g2/tom-thumb.bdf" tom-thumb.bdf d2c8c15de5ca83fcaef7cadc06d8db578c082507fa3da8a8f698d026fdda2b14
get "$u8g2/4x6.bdf" x11-4x6.bdf cc8318b75a92f6209245ac771e891fa1b51a5c64e6eea0e0c85349eb89e8ef8b
get "$u8g2/5x7.bdf" x11-5x7.bdf 6cdcaa87c2b22517a8265e3f5ea103a36d64642a9a727a7aeb6e1e9bc3961bdf
get https://raw.githubusercontent.com/fcambus/spleen/master/spleen-5x8.bdf spleen-5x8.bdf \
  40488184d075d0c752cdd239b441c5ece51e50b353156f2496c756c384ab01cb
get https://raw.githubusercontent.com/fcambus/spleen/master/LICENSE spleen-LICENSE \
  f33fe8679d5b2abecc4f1313ce6c6bfa58262964de5f7bca146596a7318047af
get https://raw.githubusercontent.com/google/fonts/main/ofl/tiny5/Tiny5-Regular.ttf Tiny5-Regular.ttf \
  cb8168f80cfee2f47f6db59f2a7afbde31cdcdcdcf262e7a993e4d468a5bf4b0
get https://raw.githubusercontent.com/google/fonts/main/ofl/tiny5/OFL.txt Tiny5-OFL.txt \
  6fe7d64407c69d187748206265977654747d3e2fe9e38e45a62cd03ec4770df6
get https://kenney.nl/media/pages/assets/kenney-fonts/8d5435c213-1677661710/kenney_kenney-fonts.zip kenney-fonts.zip \
  4e69a86eef3cd47e9d8207413868cd08bcddeb2dae4047dbd10362e2a7a16bac
[ -d kenney ] || unzip -q -o kenney-fonts.zip -d kenney
itch https://managore.itch.io/m3x6 m3x6.ttf m3x6.ttf 28c3c6a48d75a9e0b9c72b0ee99fbdc2ffc2cba669498fd5354da3dfee04c8e9
itch https://managore.itch.io/m5x7 m5x7.ttf m5x7.ttf 47c3f0b01f0fd417b3a44c2888a71d3072f750e76f3e6e946a6a9b188d988cbf
itch https://datagoblin.itch.io/monogram monogram.ttf monogram.ttf 13658c0d344553a6c272bf9e72e0f2b6edf23ac5ecf066837d9b61254b7452d6
echo "$out"
