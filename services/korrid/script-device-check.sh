#!/usr/bin/env bash
# Keep the arm64 plugin path honest: build the probe for Android, push it, run
# the example plugin on the device, and assert the declaration comes back.
#
# Costs the app nothing — this runs a standalone probe binary, not the APK.
# When korrid's plugin runtime is wired into the shell, this check gains an
# in-app counterpart; until then it is what proves the runtime still crosses
# the NDK cleanly.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CRATE="$ROOT/services/korrid"
PLUGIN="$CRATE/examples/catalog.plugin.ts"
SERIAL="${1:?usage: script-device-check.sh <adb-serial>}"
ADB_BIN="$(command -v adb)"
adb() {
  if ! timeout 15 "$ADB_BIN" "$@"; then
    echo "adb command failed or timed out: $*" >&2
    return 1
  fi
}
ADB=(adb -s "$SERIAL")

ensure_device() {
  if [[ "$SERIAL" == *:* ]]; then
    timeout 15 adb connect "$SERIAL" >/dev/null || true
  fi
  if ! timeout 15 "${ADB[@]}" wait-for-device; then
    echo "Android target is not reachable: $SERIAL" >&2
    exit 1
  fi
}

# Fail before the expensive build, then reconnect immediately before mutation.
ensure_device
cd "$CRATE"
cargo ndk -t arm64-v8a build --release --bin script_probe
PROBE="${CARGO_TARGET_DIR:-$CRATE/target}/aarch64-linux-android/release/script_probe"

ensure_device
"${ADB[@]}" push "$PROBE" /data/local/tmp/korri_script_probe >/dev/null
"${ADB[@]}" push "$PLUGIN" /data/local/tmp/korri_example.plugin.ts >/dev/null
"${ADB[@]}" shell chmod 755 /data/local/tmp/korri_script_probe

OUTPUT="$("${ADB[@]}" shell /data/local/tmp/korri_script_probe /data/local/tmp/korri_example.plugin.ts)"
echo "$OUTPUT"

grep -q '"name":"catalog"' <<<"$OUTPUT" || {
  echo "FAILED: device did not return a catalog declaration" >&2
  exit 1
}
grep -q '"id":"gba"' <<<"$OUTPUT" || {
  echo "FAILED: declaration lost its system record" >&2
  exit 1
}
echo "OK: TypeScript plugin transpiled and evaluated on device"
