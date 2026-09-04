#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash curl coreutils gcc
# shellcheck shell=bash
# THROWAWAY PROTOTYPE implementation; invoked by run-spike.sh in its devshell.
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
CRATE="$ROOT/services/korrid"
GENERATED_TS="$ROOT/contracts/generated/korrid.ts"
ANDROID_LIBS="$ROOT/clients/android/app/src/main/jniLibs"

cd "$CRATE"
cargo fmt --check
cargo check
cargo test
KORRI_CONFIG_REVIEW_IN_SHELL=1 "$CRATE/config-snapshot-review.sh"
KORRI_PLUGIN_REVIEW_IN_SHELL=1 "$CRATE/plugin-registry-review.sh"
KORRI_PLUGIN_ROUTE_REVIEW_IN_SHELL=1 "$CRATE/plugin-route-review.sh"
(
  hostile_android_review_tmp="$(mktemp -d)"
  trap 'rm -rf "$hostile_android_review_tmp"' EXIT
  hostile_checkpoint_library="$hostile_android_review_tmp/ambient-library.yaml"
  printf 'hostile ambient checkpoint library\n' >"$hostile_checkpoint_library"
  KORRI_ANDROID_APP_PACKAGE=ambient.hostile.package \
  KORRI_ANDROID_APP_ROUTE_CHECKPOINT_LIBRARY="$hostile_checkpoint_library" \
  KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_MODE=always-fail \
  KORRI_JOURNEY_EXPECTED_TITLE='Ambient Hostile Title' \
    "$CRATE/android-device-script-review.sh"
)
"$CRATE/deploy/test-render-upstreams-android.sh"
typeshare . --lang=typescript --output-file="$GENERATED_TS"
# Typeshare 1.13 emits trailing spaces and an extra final blank line.
sed -i -e 's/[[:space:]]\+$//' -e '${/^$/d;}' "$GENERATED_TS"

cd "$ROOT/clients/portal"
bun install --frozen-lockfile --ignore-scripts
cd "$ROOT/surfaces/shift"
bun install --frozen-lockfile --ignore-scripts

# The portal compiles Shift from source. Both packages must resolve React to
# the portal-owned instance or ReactDOM rejects every Shift hook at runtime.
for package in react react-dom; do
  rm -rf "$ROOT/surfaces/shift/node_modules/$package"
  ln -s "$ROOT/clients/portal/node_modules/$package" \
    "$ROOT/surfaces/shift/node_modules/$package"
done

cd "$ROOT/clients/portal"
bun run typecheck
bun test

# Shift is its own package with its own toolchain: check it on its own terms so
# a surface break is reported as a surface break, not as a portal failure.
cd "$ROOT/surfaces/shift"
bun run typecheck
bun test

cd "$CRATE"
cargo build --release --bin korrid
export KORRID_MODE="brain"
export KORRID_RPC_CAPABILITY="check-capability"
export KORRID_ADDRESS="127.0.0.1:49117"
export KORRID_SPIKE_URL="http://$KORRID_ADDRESS"
local_storage_root="$(mktemp -d)"
cp "$ROOT/docs/research/retroarch-plugin-route/config.yaml" "$local_storage_root/config.yaml"
cp "$ROOT/docs/research/retroarch-plugin-route/library.yaml" "$local_storage_root/library.yaml"
export KORRI_LOCAL_STORAGE_ROOT="$local_storage_root"
if (exec 9<>/dev/tcp/127.0.0.1/49117) 2>/dev/null; then
  echo 'korrid check port 49117 is already occupied' >&2
  exit 1
fi
"$CARGO_TARGET_DIR/release/korrid" &
server_pid=$!
cleanup_server() {
  kill "$server_pid" 2>/dev/null || true
  rm -rf "$local_storage_root"
}
trap cleanup_server EXIT
server_ready=false
for _ in $(seq 1 20); do
  if ! kill -0 "$server_pid" 2>/dev/null; then
    wait "$server_pid" || true
    echo 'fresh korrid check server exited before becoming ready' >&2
    exit 1
  fi
  if curl --fail --silent "$KORRID_SPIKE_URL/rpc" \
      -H 'content-type: application/json' \
      -H "authorization: Bearer $KORRID_RPC_CAPABILITY" \
      -d '{"_tag":"system.health","payload":{}}' >/dev/null; then
    sleep 0.05
    if ! kill -0 "$server_pid" 2>/dev/null; then
      wait "$server_pid" || true
      echo 'fresh korrid check server exited after readiness probe' >&2
      exit 1
    fi
    server_ready=true
    break
  fi
  sleep 0.25
done
if [[ "$server_ready" != true ]]; then
  echo 'fresh korrid check server did not become ready' >&2
  exit 1
fi
local_games="$(curl --fail --silent "$KORRID_SPIKE_URL/rpc" \
  -H 'content-type: application/json' \
  -H "authorization: Bearer $KORRID_RPC_CAPABILITY" \
  -d '{"_tag":"app.local-games.list","payload":{}}')"
if [[ "$local_games" != *'"id":"wl4"'* ]]; then
  echo "korrid smoke did not exercise brain-only local games" >&2
  exit 1
fi

cd "$ROOT/clients/portal"
bun src/korrid/smoke.ts

# The installed proof is the whole app, not only its hidden RPC. Use the
# canonical cross-area task so Gradle cannot silently package no portal.
cd "$ROOT"
if [[ -n "${KORRI_PORTAL_BUNDLE:-}" ]]; then
  "$KORRI_PORTAL_BUNDLE"
else
  nix run "$ROOT#portal-bundle"
fi
test -f "$ROOT/clients/android/app/src/main/assets/portal/index.html"

cd "$CRATE"
cargo ndk -t arm64-v8a -o "$ANDROID_LIBS" build --release --lib

cd "$ROOT"
clients/android/test-sunshine-runtime-settings.sh

cd "$ROOT/clients/android"
./gradlew testDebugUnitTest
./gradlew assembleDebug
APK="$ROOT/clients/android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk"
# grep must drain the whole listing: with pipefail, `grep -q` exiting at
# the first match SIGPIPEs unzip and fails the pipeline spuriously.
unzip -l "$APK" | grep 'assets/portal/index.html' >/dev/null

printf 'Rust cdylib: '
du -h "$ANDROID_LIBS/arm64-v8a/libkorrid.so" | cut -f1
printf 'APK: '
du -h app/build/outputs/apk/debug/app-arm64-v8a-debug.apk | cut -f1

if [[ "${1:-}" == "--device" ]]; then
  shift
  "$CRATE/android-smoke.sh" "${1:-${KORRI_ANDROID_DEVICE:-}}"
fi
