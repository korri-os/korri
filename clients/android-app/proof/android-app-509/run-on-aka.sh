#!/usr/bin/env bash
# fold-509: #509's part of the Android app's resize proof on aka's emulator.
# It waits for /var/tmp/korri-turns.lock to be free, for k10temp below 65 C
# and for no other emulator, holds the lock for the run (a noclobber file
# "PID label time", and flock), and removes it. Runs on aka as root.
# Arguments: the fixture APK's store path, the federation fixture's.
set -u
export PATH=/run/wrappers/bin:/run/current-system/sw/bin
export HOME=/root
APK=$1
FIXTURE=$2
ROOT=/var/tmp/fold-509
RUN=$ROOT/run
SUNSHINE=/nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri
mkdir -p "$RUN/home" "$RUN/state" "$RUN/cache"
rm -rf "$RUN/proof"
HOST=$RUN/host.txt
: > "$HOST"
. "$ROOT/lock.sh"
if ss -ltn | grep -E ':(5684|5685|5184)\b'; then echo "a proof port is in use" >>"$HOST"; exit 4; fi
take_turn "fold-509 #509 resize proof" "$HOST" || exit 3
cd "$ROOT/src" || exit 5
echo "tree $(git rev-parse HEAD), $(git status --short | wc -l) changed files" >>"$HOST"
HOME=$RUN/home XDG_STATE_HOME=$RUN/state XDG_CACHE_HOME=$RUN/cache \
  nix run .#android-app-resize-proof -- --build-store auto \
  --sunshine "$SUNSHINE" --port 5684 --adb-port 5184 --out "$RUN/proof" \
  --apk "$APK/korri-android-app-unsigned.apk" --fixture "$FIXTURE/bin/federation_fixture" >"$RUN/proof.log" 2>&1
rc=$?
echo "$(date -u +%T) load=$(cut -d' ' -f1 /proc/loadavg) k10temp=$(tctl)C emulators=$(emulators) PROOF rc=$rc" >>"$HOST"
exit $rc
