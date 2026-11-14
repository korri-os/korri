#!/usr/bin/env bash
# Byte comparison of the viewer's carried C files against patched Moonlight.
# Usage: compare.sh KORRI_PLUGINS_CHECKOUT [PLUGINS_REV]
# Run from the Korri repository root on a build machine. It reads the plugin
# patches from git at PLUGINS_REV, not from that checkout's working tree.
set -euo pipefail
plugins=$1
rev=${2:-2094c1e77471}
viewer=clients/viewer
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The same pinned source as clients/viewer/default.nix and the plugin package.
source=$(nix build --no-link --print-out-paths --impure --expr '
  (builtins.getFlake (toString ./.)).inputs.nixpkgs.legacyPackages.${builtins.currentSystem}.fetchFromGitHub {
    owner = "moonlight-stream";
    repo = "moonlight-embedded";
    rev = "775444287305849ebdf4736c75298ad0713e2d5d";
    hash = "sha256-h+hI9TQUGMI8VDzvnuRGDjxm8D7GuC/L4/0rwTFocgA=";
    fetchSubmodules = true;
  }')
cp -r "$source" "$work/moonlight"
chmod -R u+w "$work/moonlight"
# Apply the patches in the order the plugin's package.nix lists them.
git -C "$plugins" show "$rev:services/moonlight/package.nix" |
  grep -o '\./patches/[0-9][^ ]*\.patch' |
  while read -r patch; do
    git -C "$plugins" show "$rev:services/moonlight/${patch#./}" |
      patch -d "$work/moonlight" -p1 --no-backup-if-mismatch -s
  done
git -C "$plugins" show "$rev:services/moonlight/patches/ffmpeg/0001-v4l2-request.patch" \
  > "$work/0001-v4l2-request.patch"

echo "Source: Moonlight Embedded 775444287305849ebdf4736c75298ad0713e2d5d"
echo "Plugin patch stack: $(git -C "$plugins" rev-parse "$rev"), in package.nix order"
echo "Candidate code: $(git rev-parse HEAD)$(git diff --quiet HEAD -- "$viewer"/{picture,io,ffmpeg} || echo ' (with uncommitted changes)')"
echo
# The hash precedes the name: the secret scan reads "vaapi.c sha256=..." as a key.
compare() {
  local carried=$1 original=$2 name=$3
  local sum
  sum=$(sha256sum "$carried" | cut -d' ' -f1)
  if cmp -s "$original" "$carried"; then
    echo "IDENTICAL sha256=$sum $name"
  else
    local removed added
    removed=$(diff "$original" "$carried" | grep -c '^<' || true)
    added=$(diff "$original" "$carried" | grep -c '^>' || true)
    echo "ADAPTED sha256=$sum $name removed=$removed added=$added"
    diff -u --label "patched-moonlight/$name" --label "viewer/$name" "$original" "$carried" || true
  fi
}
# Every C file in picture/ and io/ that has a Moonlight counterpart. The
# bridge files are Korri's own and have none.
(cd "$viewer" && find picture io -name '*.[ch]' ! -name 'bridge.*' | sed 's|^[a-z]*/||' | sort) |
  while read -r name; do
    for dir in picture io; do
      if [ -f "$viewer/$dir/$name" ]; then
        compare "$viewer/$dir/$name" "$work/moonlight/src/$name" "$name"
      fi
    done
  done
compare "$viewer/ffmpeg/0001-v4l2-request.patch" "$work/0001-v4l2-request.patch" \
  ffmpeg/0001-v4l2-request.patch
