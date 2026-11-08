#!/usr/bin/env bash
# Spike #382: capture window for the physical Guide test (needs adb on PATH).
# 0-50 s: Chrome in front. 50-110 s: the probe app in front.
set -u
A="adb -s ${SERIAL:?set SERIAL}"
D="${DISPLAY_ID:?set DISPLAY_ID}"
ev="$(cd "$(dirname "$0")" && pwd)/evidence/physical"
mkdir -p "$ev"
$A shell am start -a android.intent.action.VIEW -d https://example.com -p com.android.chrome >/dev/null
sleep 1
$A logcat -c
$A logcat -v time > "$ev/logcat-full.txt" &
lp=$!
start=$(date +%s)
probe=0
while :; do
  t=$(( $(date +%s) - start ))
  [ "$t" -ge 110 ] && break
  if [ "$t" -ge 50 ] && [ "$probe" = 0 ]; then
    $A shell am start -n com.simonwjackson.korri.guidespike.probe/.ProbeActivity >/dev/null; probe=1
  fi
  $A exec-out screencap -p -d "$D" > "$ev/t$(printf %03d "$t").png"
  sleep 1.5
done
kill "$lp"
grep -E "KorriGuide" "$ev/logcat-full.txt" > "$ev/logcat-spike.txt"
echo done
