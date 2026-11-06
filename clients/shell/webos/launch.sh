#!/bin/sh
# Native shell with the real embedded brain. --fixture is the explicit M1 fallback.
set -eu
umask 077
# webOS uses BusyBox ash, which supports the core-size limit.
# shellcheck disable=SC3045
ulimit -c 0
if [ "$(id -u)" = 0 ]; then
    echo 'korri-shell: launcher refuses root' >&2
    exit 1
fi
# Pass through only the shell's existing surface-order CLI. AppManager's
# optional launch JSON remains ignored, not evaluated or sent to the shell.
brain=real
if [ "${1:-}" = --fixture ]; then
    brain=fixture
    shift
fi
surfaces=zone,pico
if [ "${1:-}" = --surfaces ]; then
    [ "$#" = 2 ] || { echo 'usage: launch.sh [--fixture] [--surfaces ID,ID]' >&2; exit 2; }
    case "$2" in
        zone|pico|zone,pico|pico,zone) surfaces=$2 ;;
        *) echo 'only packaged surfaces zone and pico are available' >&2; exit 2 ;;
    esac
fi
app=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)
cd "$app"
# On the B7, HOME is the root-owned app directory and is not writable.
# /tmp is private to this app's jail. Keep stderr there: AppManager sends it
# to /dev/null. Use a new directory for each launch: no stale port,
# credential or PID reuse.
tmp=${TMPDIR:-/tmp}
exec 2>>"$tmp/korri-shell-launch.log"
echo "korri-shell launch $(date)" >&2
work=$(mktemp -d "$tmp/korri-shell.XXXXXXXX")
brain_pid=''
shell_pid=''
# Called by the EXIT trap.
# shellcheck disable=SC2329
cleanup() {
    trap - EXIT HUP INT TERM
    [ -z "$shell_pid" ] || kill "$shell_pid" 2>/dev/null || :
    [ -z "$brain_pid" ] || kill "$brain_pid" 2>/dev/null || :
    wait 2>/dev/null || :
    rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 143' HUP INT TERM
mkdir "$work/credentials"
export CREDENTIALS_DIRECTORY="$work/credentials"
export APPID=org.korri.shell
# Physical 1080p output, existing surface layout at the adapter's 720-line scale.
export SLINT_SCALE_FACTOR=1.5
if [ "$brain" = fixture ]; then
    ./bin/fixture-korrid --games 12 --credentials "$CREDENTIALS_DIRECTORY" \
        </dev/null >"$work/connection" &
else
    ./bin/webos-korrid --credentials "$CREDENTIALS_DIRECTORY" \
        </dev/null >"$work/connection" &
fi
brain_pid=$!
attempt=0
while ! grep -q '^KORRID_SHELL_ORIGIN=' "$work/connection"; do
    kill -0 "$brain_pid" 2>/dev/null || { echo 'korrid exited during startup' >&2; exit 1; }
    attempt=$((attempt + 1))
    [ "$attempt" -lt 100 ] || { echo 'korrid startup timed out' >&2; exit 1; }
    # BusyBox sleep on webOS takes whole seconds only.
    usleep 100000 2>/dev/null || sleep 1
done
# Read only these two public binding values; never source the output as code.
KORRID_ADDRESS=$(sed -n 's/^KORRID_ADDRESS=//p' "$work/connection")
KORRID_SHELL_ORIGIN=$(sed -n 's/^KORRID_SHELL_ORIGIN=//p' "$work/connection")
export KORRID_ADDRESS KORRID_SHELL_ORIGIN
# AppManager's optional launch JSON is deliberately not forwarded to the shell.
./bin/korri-shell --packages packages --cache cache --installed \
    --surfaces "$surfaces" --placement fullscreen &
shell_pid=$!
status=0
wait "$shell_pid" || status=$?
shell_pid=''
exit "$status"
