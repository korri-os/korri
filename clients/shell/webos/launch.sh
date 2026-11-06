#!/bin/sh
# M1 only: real native shell, local sample-data korrid, no TV services.
set -eu
umask 077
# webOS uses BusyBox ash, which supports the core-size limit.
# shellcheck disable=SC3045
ulimit -c 0
if [ "$(id -u)" = 0 ]; then
    echo 'korri-shell: launcher refuses root' >&2
    exit 1
fi
app=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)
cd "$app"
# AppManager supplies a private writable HOME in the native application jail.
# Use a new directory for each launch: no stale port, credential or PID reuse.
: "${HOME:?AppManager must supply a private writable HOME}"
work=$(mktemp -d "$HOME/korri-shell.XXXXXXXX")
fixture_pid=''
shell_pid=''
# Called by the EXIT trap.
# shellcheck disable=SC2329
cleanup() {
    trap - EXIT HUP INT TERM
    [ -z "$shell_pid" ] || kill "$shell_pid" 2>/dev/null || :
    [ -z "$fixture_pid" ] || kill "$fixture_pid" 2>/dev/null || :
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
./bin/fixture-korrid --games 12 --credentials "$CREDENTIALS_DIRECTORY" \
    </dev/null >"$work/connection" &
fixture_pid=$!
attempt=0
while ! grep -q '^KORRID_SHELL_ORIGIN=' "$work/connection"; do
    kill -0 "$fixture_pid" 2>/dev/null || { echo 'fixture-korrid exited during startup' >&2; exit 1; }
    attempt=$((attempt + 1))
    [ "$attempt" -lt 100 ] || { echo 'fixture-korrid startup timed out' >&2; exit 1; }
    sleep 0.1
done
# Read only these two public binding values; never source the output as code.
KORRID_ADDRESS=$(sed -n 's/^KORRID_ADDRESS=//p' "$work/connection")
KORRID_SHELL_ORIGIN=$(sed -n 's/^KORRID_SHELL_ORIGIN=//p' "$work/connection")
export KORRID_ADDRESS KORRID_SHELL_ORIGIN
# AppManager's optional launch JSON is deliberately not forwarded to the shell.
./bin/korri-shell --packages packages --cache cache --installed \
    --surfaces zone,pico --placement fullscreen &
shell_pid=$!
status=0
wait "$shell_pid" || status=$?
shell_pid=''
exit "$status"
