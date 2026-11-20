#!/usr/bin/env bash
# fold-window (#509): the fold proof, then the play proof, on aka's emulator.
# It waits for /var/tmp/korri-turns.lock to be free, for k10temp below 65 C
# and for no other emulator, holds the lock for the runs (a noclobber file
# "PID label time", and flock), and removes it. Runs on aka as root.
# Arguments: the fixture APK's store path, the federation fixture's.
set -u
export PATH=/run/wrappers/bin:/run/current-system/sw/bin
export HOME=/root
APK=$1
FIXTURE=$2
ROOT=/var/tmp/fold-509-window
RUN=$ROOT/run
SUNSHINE=/nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri
mkdir -p "$RUN/home" "$RUN/state" "$RUN/cache"
rm -rf "$RUN/fold" "$RUN/play"
HOST=$RUN/host.txt
: > "$HOST"
. "$ROOT/lock.sh"
if ss -ltn | grep -E ':(5684|5685|5184)\b'; then echo "a proof port is in use" >>"$HOST"; exit 4; fi
take_turn "fold-window #509 fold and play proofs" "$HOST" || exit 3
cd "$ROOT/src" || exit 5
echo "tree $(cat COMMIT)" >>"$HOST"
HOME=$RUN/home XDG_STATE_HOME=$RUN/state XDG_CACHE_HOME=$RUN/cache \
  nix run .#android-app-fold-proof -- --build-store auto \
  --sunshine "$SUNSHINE" --port 5684 --adb-port 5184 --out "$RUN/fold" \
  --apk "$APK/korri-android-app-unsigned.apk" --fixture "$FIXTURE/bin/federation_fixture" >"$RUN/fold.log" 2>&1
rc=$?
echo "$(date -u +%T) load=$(cut -d' ' -f1 /proc/loadavg) k10temp=$(tctl)C emulators=$(emulators) FOLD rc=$rc" >>"$HOST"
[ "${SKIP_PLAY:-0}" = 1 ] && exit $rc
# The play proof's turn: the emulator of the fold proof has stopped; wait for it to cool.
cool=$SECONDS; until { [ "$(tctl)" -lt 65 ] && [ "$(emulators)" -eq 0 ]; } || [ $((SECONDS - cool)) -ge 3600 ]; do sleep 30; done
echo "$(date -u +%T) load=$(cut -d' ' -f1 /proc/loadavg) k10temp=$(tctl)C emulators=$(emulators) PLAY starts" >>"$HOST"
HOME=$RUN/home XDG_STATE_HOME=$RUN/state XDG_CACHE_HOME=$RUN/cache \
  nix run .#android-app-play-proof -- --build-store auto \
  --sunshine "$SUNSHINE" --port 5684 --adb-port 5184 --out "$RUN/play" \
  --apk "$APK/korri-android-app-unsigned.apk" >"$RUN/play.log" 2>&1
prc=$?
echo "$(date -u +%T) load=$(cut -d' ' -f1 /proc/loadavg) k10temp=$(tctl)C emulators=$(emulators) PLAY rc=$prc" >>"$HOST"
exit $(( rc != 0 || prc != 0 ))
