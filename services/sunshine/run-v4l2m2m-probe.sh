#!/usr/bin/env nix-shell
#!nix-shell -i bash -p bash coreutils
# Run on the Portal with the patched Iris module already loaded.
# This test does not load modules or change boot files.
set -euo pipefail
probe=${1:?usage: run-v4l2m2m-probe.sh PROBE_BINARY}
out=$(mktemp -d /tmp/sunshine-v4l2m2m.XXXXXX)
for codec in h264 hevc; do
  for size in 1280x720 1920x1080; do
    file="$out/$size.$codec"
    timeout -s KILL 60 "$probe" "${codec}_v4l2m2m" "${size%x*}" "${size#*x}" "$file" >"$file.log" 2>&1
    tail -n 1 "$file.log"
  done
done
printf 'Output: %s\n' "$out"
