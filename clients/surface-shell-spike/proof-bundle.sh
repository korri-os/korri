#!/usr/bin/env bash
# THROWAWAY: unpack and run the exact aarch64 archive on the workstation.
# PRoot transparently invokes QEMU for foreign ELF files; run.sh is unchanged.
set -euo pipefail
archive=$(realpath "$1")
out=$(realpath -m "$2")
mkdir -p "$out"
fresh=$(mktemp -d /tmp/surface-shell-unpack.XXXXXX)
bundle=$(basename "$archive" .tar.gz)
tar xzf "$archive" -C "$fresh"
sha256sum "$archive" "$fresh/$bundle/run.sh" >"$out/source-sha256.txt"
mkdir -m 700 "$fresh/runtime"
display=${SHELL_PROOF_DISPLAY:-:93}
Xvfb "$display" -screen 0 640x480x24 -nolisten tcp >"$out/xvfb.log" 2>&1 &
xvfb=$!
pid=''
zone=''
cleanup() {
  if [[ -n $pid ]]; then
    [[ -z $zone ]] || xdotool windowclose "$zone" 2>/dev/null || :
    # PRoot does not forward SIGTERM to its tracee. Stop the owned QEMU child.
    for child in $(pgrep -P "$pid" || :); do kill -KILL "$child" 2>/dev/null || :; done
    kill -KILL "$pid" 2>/dev/null || :
  fi
  kill "$xvfb" 2>/dev/null || :
  chmod -R u+w "$fresh"
  rm -rf "$fresh"
}
trap cleanup EXIT
for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X${display#:} ]] && break; sleep .1; done
# systemd-run's default working directory is /. The only display-edge changes
# are X11/Xvfb instead of the SP Wayland socket, and an owned private runtime dir.
export DISPLAY=$display XDG_RUNTIME_DIR="$fresh/runtime"
export SLINT_BACKEND=winit-software SHELL_FULLSCREEN=1 SHELL_STATS=1 SHELL_SURFACES=zone,pico
unset SHELL_ROOT SHELL_SIZE WAYLAND_DISPLAY LD_LIBRARY_PATH
(cd / && exec proot -q qemu-aarch64 "$fresh/$bundle/run.sh") 2>"$out/stats.log" & pid=$!
window() {
  local id=''
  for _ in $(seq 600); do
    id=$(xdotool search --onlyvisible --name "^$1$" 2>/dev/null | head -1 || :)
    [[ -n $id ]] && { sleep 2; echo "$id"; return; }
    kill -0 "$pid" || { cat "$out/stats.log" >&2; return 1; }
    sleep .1
  done
  echo "no window $1" >&2; return 1
}
zone=$(window Zone)
import -window "$zone" "$out/zone.png"
xdotool windowfocus --sync "$zone" key F2
pico=$(window Pico)
import -window "$pico" "$out/pico.png"
xdotool windowfocus --sync "$pico" key F2
zone=$(window Zone)
import -window "$zone" "$out/zone-return.png"
# The visual proof is complete. PRoot can stay in ptrace wait after SIGTERM;
# terminate its owned emulator explicitly, rather than leaving it running.
for child in $(pgrep -P "$pid" || :); do kill -KILL "$child" 2>/dev/null || :; done
kill -KILL "$pid" 2>/dev/null || :
wait "$pid" 2>/dev/null || :; pid=''
grep -q 'SHELL_STATS package=zone' "$out/stats.log"
grep -q 'SHELL_STATS package=pico' "$out/stats.log"
if grep -E 'Error:|panicked|shell: (property|QuickJS)' "$out/stats.log"; then exit 1; fi
printf '%s\n' 'Exact unpacked run.sh, caller cwd=/, SHELL_ROOT unset, QEMU/PRoot + Xvfb: PASS' >"$out/proof.txt"
cat "$out/proof.txt"
