#!/usr/bin/env nix-shell
#! nix-shell -i bash -p openssh coreutils
# THROWAWAY: bundle the aarch64 benchmark, run it three times on the RG35XX SP
# from /tmp (nothing installed, kiosk left running), copy results back, clean up.
# Usage: run-on-sp.sh STORE_PATH OUTPUT_DIR [STEPS]
set -euo pipefail
PKG=$1
OUT=$2
STEPS=${3:-20000}
HERE=$(cd "$(dirname "$0")" && pwd)
HOST=root@10.42.5.1
SSH=(ssh -p 2222 -o BatchMode=yes -o ConnectTimeout=8)
SCP=(scp -P 2222 -o BatchMode=yes -o ConnectTimeout=8)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

BUNDLE_DLOPEN= "$HERE/../device/bundle.sh" "$PKG" zone-logic-bench "$WORK/zone-logic-bench"
"${SCP[@]}" "$WORK/zone-logic-bench.tar.gz" "$HOST:/tmp/"
mkdir -p "$OUT"
"${SSH[@]}" "$HOST" 'tar xf /tmp/zone-logic-bench.tar.gz -C /tmp && /tmp/zone-logic-bench/lib/ld-linux-aarch64.so.1 --inhibit-cache --library-path /tmp/zone-logic-bench/lib --list /tmp/zone-logic-bench/bin/zone-logic-bench' >"$OUT/list.txt"
"${SSH[@]}" "$HOST" 'cat /proc/loadavg; systemctl is-active korri-chromium-kiosk.service' >"$OUT/device-state.txt"
for run in 1 2 3; do
  "${SSH[@]}" "$HOST" "/tmp/zone-logic-bench/run.sh $STEPS" >"$OUT/run-$run.json"
done
"${SSH[@]}" "$HOST" 'rm -rf /tmp/zone-logic-bench /tmp/zone-logic-bench.tar.gz'
ls -la "$OUT"
