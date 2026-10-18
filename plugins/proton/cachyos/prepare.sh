# cachyos-prepare <cache-root>
#
# Download, verify and unpack the pinned Proton-CachyOS arm64 build into
# Korri's reconstructible cache, ready to link from Steam's
# compatibilitytools.d. plugin.nix substitutes the URL, SHA-256, version
# directory and compatibilitytool.vdf path from cachyos/pin.nix.
#
# Prints the ready directory as the last stdout line. Exit status: 0 ready,
# 1 download or verify failure, 2 usage. Tool output goes to stderr.
set -Eeuo pipefail
umask 022

say() { printf 'cachyos-prepare: %s\n' "$*" >&2; }
usage() { say "$*"; echo 'usage: cachyos-prepare <cache-root>' >&2; exit 2; }
fail() { say "$*"; exit 1; }
trap 'fail "failed at line $LINENO (status $?)"' ERR
trap 'exit 130' INT
trap 'exit 143' TERM

[[ "$#" == 1 && "$1" == /* ]] || usage 'expected one absolute cache root'
[[ -d "$1" ]] || usage "cache root is not a directory: $1"

base="$1/proton/cachyos"
ready="$base/@version@"
url="@url@"
sha="@sha256@"
downloads="$base/.downloads"
work=""

finish() { printf '%s\n' "$ready"; exit 0; }
discard() {
  [[ -e "$1" ]] || return 0
  chmod -R u+w -- "$1"
  rm -rf -- "$1"
}
# Remove this run's unfinished work directory on any exit.
trap '[[ -z "$work" ]] || discard "$work"' EXIT

# A ready directory with nothing left over needs no lock.
leftovers() { [[ -e "$downloads" ]] || compgen -G "$base/.tmp-*" >/dev/null; }
if [[ -d "$ready" ]] && ! leftovers; then finish; fi
mkdir -p "$base" || fail "cannot create $base"
exec 9>"$base/.lock"
flock 9
# Every writer holds the lock, so a work directory now is from an interrupted
# run. Leftovers waste space but do not stop preparation.
for stale in "$base"/.tmp-*; do discard "$stale" || say "could not remove $stale"; done
# A run killed after its rename leaves its downloads behind. Another run may
# also have finished while this one waited for the lock.
if [[ -d "$ready" ]]; then
  discard "$downloads" || say "could not remove $downloads"
  finish
fi
[[ ! -e "$ready" ]] || fail "$ready exists and is not a directory"

matches() { printf '%s  %s\n' "$1" "$2" | sha256sum --check --status; }
# Continue from any bytes already in the partial file. Abort a stalled
# connection rather than holding the game launch forever.
get() {
  curl --fail --location --silent --show-error --retry 5 --retry-delay 2 \
    --connect-timeout 30 --speed-limit 1024 --speed-time 60 \
    --continue-at - --output "$2" "$1"
}
# Leave a verified copy at $downloads/<sha256>. A failed transfer keeps its
# partial file so the next run resumes it; wrong bytes are always deleted.
fetch() {
  local sha="$1" url="$2" file="$downloads/$1" part="$downloads/$1.part" status=0
  if [[ -f "$file" ]] && matches "$sha" "$file"; then return 0; fi
  rm -f -- "$file"
  if [[ -s "$part" ]]; then
    if ! matches "$sha" "$part"; then
      say "resuming $url"
      get "$url" "$part" || status=$?
      case "$status" in
        0) matches "$sha" "$part" || { say "resumed bytes do not match; restarting $url"; rm -f -- "$part"; } ;;
        33 | 36) say "server cannot resume; restarting $url"; rm -f -- "$part" ;;
        *) fail "download failed: $url (curl status $status)" ;;
      esac
    fi
  fi
  if [[ ! -s "$part" ]]; then
    say "downloading $url"
    status=0
    get "$url" "$part" || status=$?
    (( status == 0 )) || fail "download failed: $url (curl status $status)"
    if ! matches "$sha" "$part"; then
      rm -f -- "$part"
      fail "hash mismatch: $url"
    fi
  fi
  mv -f -- "$part" "$file"
}

mkdir -p "$downloads"
fetch "$sha" "$url"

work=$(mktemp -d "$base/.tmp-XXXXXXXXXX")
chmod 755 "$work"
say "unpacking Proton into $work"
# The release has one top-level directory: the tool's dist directory.
tar --xz --no-same-owner --strip-components=1 -xf "$downloads/$sha" -C "$work"
[[ -f "$work/proton" && -f "$work/toolmanifest.vdf" ]] \
  || fail 'unpacked release has no proton or toolmanifest.vdf'
# Serve as Steam's global default instead of chaining to an unregistered
# runtime tool.
grep -v '"require_tool_appid"' "$work/toolmanifest.vdf" >"$work/toolmanifest.vdf.tmp"
mv -f -- "$work/toolmanifest.vdf.tmp" "$work/toolmanifest.vdf"
# Register the tool as a compatibilitytools.d subdirectory under Korri's name.
install -m 0644 "@compatibilitytool@" "$work/compatibilitytool.vdf"

# The ready directory is never edited in place: existence is the only check
# a later run makes. Flush the data before the rename makes it visible.
chmod -R a-w "$work"
sync --file-system "$work"
mv -T -- "$work" "$ready"
work=""
rm -rf -- "$downloads" || say "could not remove $downloads"
finish
