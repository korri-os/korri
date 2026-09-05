#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash android-tools bun cargo-ndk clang coreutils gawk git gnugrep gnused util-linux
# shellcheck shell=bash
set -euo pipefail
ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
# This sources only harness definitions. The original gate remains its own entry point.
source "$ROOT/clients/android/bridge-contract-check.sh"
HOST_PID=""
HOST_PORT=""
REVERSE_INSTALLED=false
cleanup_acceptance_fixture() {
  local failed=0
  if [[ "$REVERSE_INSTALLED" == true ]]; then
    timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" reverse --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || failed=1
  fi
  if [[ -n "$HOST_PID" ]]; then
    kill -TERM -- "-$HOST_PID" 2>/dev/null || true
    for _ in $(seq 1 40); do
      if ! kill -0 -- "-$HOST_PID" 2>/dev/null; then break; fi
      sleep 0.25
    done
    if kill -0 -- "-$HOST_PID" 2>/dev/null; then
      kill -KILL -- "-$HOST_PID" 2>/dev/null || true
      failed=1
    fi
    wait "$HOST_PID" 2>/dev/null || true
  fi
  return "$failed"
}

initialize_emulator_run
# Do not inherit a caller's live host sockets, helper paths, or unit policy.
for name in "${!KORRID_@}"; do unset "$name"; done
bash "$ROOT/clients/android/test/federation-systemd-fixture-test.sh"
# Host compilation must not inherit Android bindgen target flags.
(
  unset BINDGEN_EXTRA_CLANG_ARGS
  cd "$ROOT/services/korrid"
  timeout --kill-after=30s 900 cargo build --example federation_fixture
)
mkdir "$RUN_DIR/units"
export KORRI_FEDERATION_UNIT_ROOT="$RUN_DIR/units"
# Existing production executable configuration seam, never a production test mode.
export KORRID_SYSTEMD_RUN="$ROOT/clients/android/test/federation-systemd-fixture.sh"
export KORRID_SYSTEMCTL="$KORRID_SYSTEMD_RUN"
export KORRID_PRIVATE_STATE_ROOT="$RUN_DIR/host/private"
export KORRID_SUNSHINE_PRIVATE_STATE_ROOT="$RUN_DIR/sunshine"
export KORRID_CONTROL_DIRECTORY="$RUN_DIR/control"
export KORRID_CONTROL_SOCKET="$RUN_DIR/control/control.sock"
export KORRID_COMPOSITOR_CONTROL_DIRECTORY="$RUN_DIR/compositor"
export KORRID_CERTIFICATE_CONTROL_DIRECTORY="$RUN_DIR/certificate"
# An absent socket is deliberate: streamControl must be disabled.
export KORRID_SUNSHINE_CERTIFICATE_CONTROL_SOCKET="$RUN_DIR/certificate/missing.sock"
KORRID_SUNSHINE_CERTIFICATE_CONTROL_GID="$(id -g)"
KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_UID="$(id -u)"
KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_GID="$KORRID_SUNSHINE_CERTIFICATE_CONTROL_GID"
export KORRID_SUNSHINE_CERTIFICATE_CONTROL_GID KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_UID
export KORRID_SUNSHINE_CERTIFICATE_CONTROL_PEER_GID
setsid "$CARGO_TARGET_DIR/debug/examples/federation_fixture" "$RUN_DIR/host" >"$RUN_DIR/host.log" 2>&1 &
HOST_PID=$!
deadline=$((SECONDS + 30))
while [[ ! -f "$RUN_DIR/host/ready" ]]; do
  if ! kill -0 "$HOST_PID" 2>/dev/null || (( SECONDS >= deadline )); then
    tail -100 "$RUN_DIR/host.log" >&2
    fail_with_emulator_diagnostics 'Federation host fixture did not become ready within 30 seconds'
  fi
  sleep 0.1
done
read -r HOST_PORT HOST_KEY <"$RUN_DIR/host/ready"
[[ "$HOST_PORT" =~ ^[0-9]+$ && "$HOST_KEY" =~ ^[a-f0-9]{64}$ ]]
bundle_portal_assets
build_x86_64_korrid
create_avd
boot_emulator
timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" reverse "tcp:$HOST_PORT" "tcp:$HOST_PORT"
REVERSE_INSTALLED=true
use_build_sdk_env
export ANDROID_SERIAL="$SERIAL"
cd "$ANDROID_CLIENT"
timeout --kill-after=30s 600 ./gradlew :app:assembleDebug :app:assembleDebugAndroidTest :signer-test:assembleDebug
timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" install -r signer-test/build/outputs/apk/debug/signer-test-debug.apk
timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" install -r app/build/outputs/apk/debug/app-x86_64-debug.apk
timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" shell appops set com.simonwjackson.korri.debug MANAGE_EXTERNAL_STORAGE allow
status=0
timeout --kill-after=30s "$CONTRACT_TEST_TIMEOUT_SECONDS" ./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=com.limelight.KorriFederationAcceptanceTest \
  -Pandroid.testInstrumentationRunnerArguments.federationPort="$HOST_PORT" \
  -Pandroid.testInstrumentationRunnerArguments.federationDeviceKey="$HOST_KEY" || status=$?
if [[ "$status" != 0 ]]; then
  tail -100 "$RUN_DIR/host.log" >&2
  tail -100 "$RUN_DIR/units/calls" >&2 || true
  timeout "$ADB_TIMEOUT_SECONDS" adb -s "$SERIAL" logcat -d -s KorriBrain KorridServer AndroidRuntime >&2 || true
  print_emulator_diagnostics
  exit "$status"
fi
# The helper trace proves that no systemd operation targeted a real unit.
[[ "$(grep -c -- '--unit=' "$RUN_DIR/units/calls")" == 1 ]]
[[ "$(grep -c -- ' stop ' "$RUN_DIR/units/calls")" == 1 ]]
[[ "$(grep -c -- ' freeze ' "$RUN_DIR/units/calls")" == 1 ]]
[[ "$(grep -c -- ' thaw ' "$RUN_DIR/units/calls")" == 1 ]]
echo "federation acceptance passed on $SERIAL (real encrypted peer, one completed play)"
