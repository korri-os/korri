#!/usr/bin/env bash
# Spike #382: capture (DURATION s, default 60) with the probe app in front, for the held-input test.
set -u
A="adb -s ${SERIAL:?}"; D="${DISPLAY_ID:?}"
ev="$(cd "$(dirname "$0")" && pwd)/evidence/held"; mkdir -p "$ev"
$A shell am start -n com.simonwjackson.korri.guidespike.probe/.ProbeActivity >/dev/null
$A logcat -c; $A logcat -v time -s KorriGuideSpike KorriGuideProbe > "$ev/logcat.txt" & lp=$!
start=$(date +%s)
while [ $(( $(date +%s) - start )) -lt ${DURATION:-60} ]; do
  $A exec-out screencap -p -d "$D" > "$ev/t$(printf %03d $(( $(date +%s) - start ))).png"; sleep 1.5
done
kill $lp; echo done
