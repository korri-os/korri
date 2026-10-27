#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash bun coreutils git shellcheck
# shellcheck shell=bash
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
bash -n clients/android/bridge-contract-check.sh clients/android/federation-acceptance-check.sh
shellcheck -x clients/android/bridge-contract-check.sh clients/android/federation-acceptance-check.sh "$0"
bun test clients/android/test/emulator-bootstrap-proxy.test.ts
root=$(mktemp -d -t korri-emulator-ownership-test.XXXXXXXX)
trap 'rm -rf "$root"' EXIT
(
  export KORRI_ROOT="$ROOT" ANDROID_HOME=/not-a-sdk ANDROID_SDK_ROOT=/not-a-sdk ANDROID_NDK_ROOT=/not-an-ndk
  export KORRI_BRIDGE_AVD_PACKAGE='system-images;android-34;google_apis;x86_64' KORRI_EMULATOR_NIX_SDK=/not-a-sdk KORRI_NDK_VERSION=unused KORRI_PORTAL_BUNDLE=/must-not-execute
  # shellcheck source=clients/android/bridge-contract-check.sh
  source clients/android/bridge-contract-check.sh
  trap - EXIT INT TERM
  [[ -z "$RUN_DIR" && -z "$EMULATOR_PID" && -z "$BOOTSTRAP_PROXY_PID" && "$LOCK_ACQUIRED" == false ]]
  # No ADB operation is valid in any of these states. Use actual guards, including
  # Bash's conditional-call context where implicit errexit cannot protect them.
  if verify_bootstrap_isolation; then exit 1; fi
  if start_bootstrap_proxy; then exit 1; fi
  RUN_DIR="$root"
  mkdir -p "$RUN_DIR/avd/$AVD_NAME.avd"
  EMULATOR_PID=$$
  if assert_owned_emulator; then exit 1; fi # no lock
  LOCK_ACQUIRED=true
  SERIAL=not-an-owned-emulator
  if assert_owned_emulator; then exit 1; fi # no device call
  SERIAL="emulator-$EMULATOR_PORT"
  bash -c 'exit 0' &
  EMULATOR_PID=$!
  wait "$EMULATOR_PID"
  if assert_owned_emulator 2>/dev/null; then exit 1; fi # emulator has exited
)
echo 'Owned-AVD guards reject missing authority, wrong serial, and an exited process before ADB.'
