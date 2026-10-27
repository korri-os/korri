#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash android-tools bun cargo-ndk clang coreutils gawk git gnugrep gnused util-linux
# shellcheck shell=bash
set -euo pipefail
ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
# This sources only harness definitions. The original gate remains its own entry point.
source "$ROOT/clients/android/bridge-contract-check.sh"
FIXTURE_PID=""
REVERSED_PORTS=()
cleanup_acceptance_fixture() {
  local failed=0
  local port
  # Only fixture logs and path policy are retained. Never copy private identities.
  if [[ -n "${DIAGNOSTICS_DIR:-}" ]]; then
    for name in fixture.log host.log host-restart.log host-environment.json; do
      if [[ -f "$RUN_DIR/$name" ]]; then cp "$RUN_DIR/$name" "$DIAGNOSTICS_DIR/"; fi
    done
    if [[ -f "$RUN_DIR/units/calls" ]]; then cp "$RUN_DIR/units/calls" "$DIAGNOSTICS_DIR/unit-calls.log"; fi
  fi
  for port in "${REVERSED_PORTS[@]}"; do
    timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" reverse --remove "tcp:$port" >/dev/null 2>&1 || failed=1
  done
  if [[ -n "$FIXTURE_PID" ]]; then
    # The TS controller joins its host first. Killing the whole group initially
    # would race its orderly child shutdown and turn SIGTERM into a false failure.
    kill -TERM "$FIXTURE_PID" 2>/dev/null || true
    for _ in $(seq 1 60); do
      if ! kill -0 "$FIXTURE_PID" 2>/dev/null; then break; fi
      sleep 0.25
    done
    if kill -0 -- "-$FIXTURE_PID" 2>/dev/null; then
      kill -KILL -- "-$FIXTURE_PID" 2>/dev/null || true
      failed=1
    fi
    wait "$FIXTURE_PID" || failed=1
    if [[ "$failed" == 0 ]]; then echo 'Federation fixture joined; process group and reverse mappings removed.'; fi
  fi
  return "$failed"
}

initialize_emulator_run
[[ "$AVD_PLATFORM/$AVD_ABI" == "android-34/x86_64" ]]
# Build and fixture tests must never inherit live host configuration.
for name in "${!KORRID_@}"; do unset "$name"; done
unset KORRI_LOCAL_STORAGE_ROOT
bash "$ROOT/clients/android/test/federation-systemd-fixture-test.sh"
(
  unset BINDGEN_EXTRA_CLANG_ARGS
  cd "$ROOT/services/korrid"
  timeout --kill-after=30s 900 cargo build --example federation_fixture
)
FEDERATION_FIXTURE_EXECUTABLE="$CARGO_TARGET_DIR/debug/examples/federation_fixture" \
  bun test "$ROOT/clients/android/test/nostr-relay-fixture.test.ts"
setsid bun "$ROOT/clients/android/test/nostr-relay-fixture.ts" \
  "$RUN_DIR" "$ROOT" "$CARGO_TARGET_DIR/debug/examples/federation_fixture" >"$RUN_DIR/fixture.log" 2>&1 &
FIXTURE_PID=$!
deadline=$((SECONDS + 40))
while [[ ! -f "$RUN_DIR/fixture-ready" ]]; do
  if ! kill -0 "$FIXTURE_PID" 2>/dev/null || (( SECONDS >= deadline )); then
    cat "$RUN_DIR/fixture.log" >&2
    fail_with_emulator_diagnostics 'Federation fixtures did not become ready within 40 seconds'
  fi
  sleep 0.1
done
read -r HOST_PORT HOST_KEY RELAY_PORT UNAVAILABLE_PORT CONTROL_PORT CONTROL_TOKEN <"$RUN_DIR/fixture-ready"
[[ "$HOST_PORT" =~ ^[0-9]+$ && "$HOST_KEY" =~ ^[a-f0-9]{64}$ ]]
bundle_portal_assets
build_x86_64_korrid
create_avd
boot_emulator
for port in "$HOST_PORT" "$RELAY_PORT" "$UNAVAILABLE_PORT" "$CONTROL_PORT"; do
  [[ "$port" =~ ^[0-9]+$ ]]
  timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" reverse "tcp:$port" "tcp:$port"
  REVERSED_PORTS+=("$port")
done
use_build_sdk_env
export ANDROID_SERIAL="$SERIAL"
cd "$ANDROID_CLIENT"
timeout --kill-after=30s 600 ./gradlew :app:assembleDebug :app:assembleDebugAndroidTest :signer-test:assembleDebug
timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" install -r signer-test/build/outputs/apk/debug/signer-test-debug.apk
timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" install -r app/build/outputs/apk/debug/app-x86_64-debug.apk
timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" shell appops set com.simonwjackson.korri.debug MANAGE_EXTERNAL_STORAGE allow
status=0
# Discovery uses production 60-second cadence. Bound the whole ordered flow,
# including intentional outages and restarts; never retry instrumentation.
timeout --kill-after=30s 600 ./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.injected.androidTest.leaveApksInstalledAfterRun=true \
  -Pandroid.testInstrumentationRunnerArguments.class=com.limelight.KorriFederationAcceptanceTest \
  -Pandroid.testInstrumentationRunnerArguments.federationPort="$HOST_PORT" \
  -Pandroid.testInstrumentationRunnerArguments.federationDeviceKey="$HOST_KEY" \
  -Pandroid.testInstrumentationRunnerArguments.federationRelayPort="$RELAY_PORT" \
  -Pandroid.testInstrumentationRunnerArguments.federationUnavailablePort="$UNAVAILABLE_PORT" \
  -Pandroid.testInstrumentationRunnerArguments.federationControlPort="$CONTROL_PORT" \
  -Pandroid.testInstrumentationRunnerArguments.federationControlToken="$CONTROL_TOKEN" || status=$?
cat "$RUN_DIR/fixture.log"
if [[ "$status" != 0 ]]; then
  tail -100 "$RUN_DIR/host.log" >&2
  tail -100 "$RUN_DIR/units/calls" >&2 || true
  print_emulator_diagnostics
  exit "$status"
fi
timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" logcat -b all -d -s FederationAcceptance:I '*:S'
# Exact helper trace preserves all Bundle A session coverage, without real units.
[[ "$(grep -c -- '--unit=' "$RUN_DIR/units/calls")" == 1 ]]
[[ "$(grep -c -- ' stop ' "$RUN_DIR/units/calls")" == 1 ]]
[[ "$(grep -c -- ' freeze ' "$RUN_DIR/units/calls")" == 1 ]]
[[ "$(grep -c -- ' thaw ' "$RUN_DIR/units/calls")" == 1 ]]
verify_emulator_lifecycle
echo "federation acceptance passed on $SERIAL (relay discovery, remembered peers, exact session, one completed play)"
