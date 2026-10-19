# cachyos-unpack <work-dir> <release.tar.xz>
#
# Unpack the verified Proton-CachyOS arm64 release that korrid downloaded,
# ready to link from Steam's compatibilitytools.d. plugin.nix substitutes the
# compatibilitytool.vdf path. korrid owns the download, the hash check, the
# work directory, the read-only flip, the sync and the rename to the ready
# directory. This program writes only under <work-dir>.
#
# Exit status: 0 ready, 1 failed. Diagnostics go to stderr; stdout carries
# nothing korrid reads.
set -Eeuo pipefail
umask 022

say() { printf 'cachyos-unpack: %s\n' "$*" >&2; }
fail() { say "$*"; exit 1; }
trap 'fail "failed at line $LINENO (status $?)"' ERR
trap 'exit 130' INT
trap 'exit 143' TERM

(( $# == 2 )) || fail 'usage: cachyos-unpack <work-dir> <release.tar.xz>'
work="$1"
release="$2"
[[ "$work" == /* && -d "$work" ]] || fail "work directory is not an absolute directory: $work"
[[ -f "$release" ]] || fail "not a file: $release"

say "unpacking Proton into $work"
# The release has one top-level directory: the tool's dist directory.
tar --xz --no-same-owner --strip-components=1 -xf "$release" -C "$work"
[[ -f "$work/proton" && -f "$work/toolmanifest.vdf" ]] \
  || fail 'unpacked release has no proton or toolmanifest.vdf'
# Serve as Steam's global default instead of chaining to an unregistered
# runtime tool.
grep -v '"require_tool_appid"' "$work/toolmanifest.vdf" >"$work/toolmanifest.vdf.tmp"
mv -f -- "$work/toolmanifest.vdf.tmp" "$work/toolmanifest.vdf"
# Register the tool as a compatibilitytools.d subdirectory under Korri's name.
install -m 0644 "@compatibilitytool@" "$work/compatibilitytool.vdf"
