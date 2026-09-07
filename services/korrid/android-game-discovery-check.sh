#!/usr/bin/env nix-shell
#! nix-shell -i bash -p diffutils bash android-tools coreutils curl gnugrep gnused jq websocat
# shellcheck shell=bash
# Explicit-device Android proof for user-selected game discovery.
#
# The automated gate does not drive Android's system folder picker. It stages
# controlled folders, then an androidTest-only seam asks the same JNI receipt
# issuer used by the picker for one-use receipts and registers them through the
# production RPC endpoint. Manual observation note: the interactive chooser
# remains covered by KorriGameFolderBridgeTest plus a human check that Add folder
# opens Android's directory picker and reports cancellation without mutation.
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
CRATE="$ROOT/services/korrid"
# shellcheck source=services/korrid/android-instrumentation-result.sh
source "$CRATE/android-instrumentation-result.sh"
SERIAL=""
ADB_BIN="${KORRI_ADB_BIN:-$(command -v adb)}"
PKG="com.simonwjackson.korri.debug"
TEST_PKG="$PKG.test"
RETROARCH_PKG="${KORRI_RETROARCH_PACKAGE:-com.korri.retroarch}"
HOST_PORT="${KORRI_ANDROID_GAME_DISCOVERY_HOST_PORT:-43124}"
DEVTOOLS_HOST_PORT="${KORRI_ANDROID_GAME_DISCOVERY_DEVTOOLS_HOST_PORT:-43120}"
DEBUG_CAPABILITY_SH="${KORRI_ANDROID_DEBUG_CAPABILITY_SH:-$CRATE/android-debug-capability.sh}"
DEBUG_LAUNCH_LOCAL_SH="${KORRI_ANDROID_DEBUG_LAUNCH_LOCAL_SH:-$CRATE/android-debug-launch-local.sh}"
ANDROID_STORAGE_ROOT="/sdcard/korri"
LOCK_REMOTE="$ANDROID_STORAGE_ROOT/.android-game-discovery-check.lock"
LOCK_OWNER_REMOTE="$LOCK_REMOTE/owner"
BACKUP_REMOTE="$ANDROID_STORAGE_ROOT/.android-game-discovery-check-backup-$$"
FIXTURE_A="/sdcard/korri-u9-discovery-a-$$"
FIXTURE_B="/sdcard/korri-u9-discovery-b-$$"
FIXTURE_A_UNAVAILABLE="${FIXTURE_A}.unavailable"
FIXTURE_B_UNAVAILABLE="${FIXTURE_B}.unavailable"
DEVICE_REMOTE="$ANDROID_STORAGE_ROOT/device.yaml"
GAMES_REMOTE="$ANDROID_STORAGE_ROOT/catalog/games.yaml"
RELEASES_REMOTE="$ANDROID_STORAGE_ROOT/catalog/releases.yaml"
APK="$ROOT/clients/android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk"
TEST_APK="$ROOT/clients/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"
CURL=(curl --connect-timeout 2 --max-time 5)
FORWARD_ACTIVE=false
LOCK_ACQUIRED=false
DEVICE_WAS_PRESENT=false
GAMES_WAS_PRESENT=false
RELEASES_WAS_PRESENT=false
CATALOG_DIR_WAS_PRESENT=false
BACKUP_CREATED=false
CHECKPOINT_RESTORE_NEEDED=false
PRIVATE_STATE_MOVED=false
PRIOR_UID_APPOP_MODE=""
PRIOR_TOP_ACTIVITY=""
RUN_DIR=""

usage() {
  echo "usage: android-game-discovery-check.sh --serial <adb-serial>" >&2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --serial)
      SERIAL="${2:-}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage
      exit 1
      ;;
  esac
done
if [[ -z "$SERIAL" ]]; then
  usage
  exit 1
fi

validate_host_forward_port() {
  local name="$1"
  local value="$2"
  if [[ ! "$value" =~ ^[0-9]+$ || "$value" -lt 1024 || "$value" -gt 65535 ]]; then
    echo "$name must be a TCP port in 1024..65535, got '$value'" >&2
    exit 1
  fi
}

validate_host_forward_port KORRI_ANDROID_GAME_DISCOVERY_HOST_PORT "$HOST_PORT"
validate_host_forward_port KORRI_ANDROID_GAME_DISCOVERY_DEVTOOLS_HOST_PORT "$DEVTOOLS_HOST_PORT"
if [[ "$HOST_PORT" == "$DEVTOOLS_HOST_PORT" ]]; then
  echo "KORRI_ANDROID_GAME_DISCOVERY_HOST_PORT and KORRI_ANDROID_GAME_DISCOVERY_DEVTOOLS_HOST_PORT must be distinct" >&2
  exit 1
fi

adb_failure_is_transient() {
  grep -Eiq '(error: closed|device offline|offline|connection reset|connection refused|protocol fault|no devices/emulators found|device .* not found|transport.*closed)' "$1"
}

adb_reconnect_and_wait() {
  if [[ "$SERIAL" == *:* ]]; then
    timeout 15 "$ADB_BIN" connect "$SERIAL" >/dev/null 2>&1 || true
  fi
  timeout 20 "$ADB_BIN" -s "$SERIAL" wait-for-device >/dev/null 2>&1
}

adb_command() {
  local retry_safe="$1"
  shift
  local attempt=1
  local max_attempts=3
  local stdout_file=""
  local stderr_file=""
  local combined_file=""
  local status=0
  while true; do
    stdout_file="$(mktemp)"
    stderr_file="$(mktemp)"
    combined_file="$(mktemp)"
    if timeout 20 "$ADB_BIN" "$@" >"$stdout_file" 2>"$stderr_file"; then
      cat "$stdout_file"
      rm -f "$stdout_file" "$stderr_file" "$combined_file"
      return 0
    else
      status=$?
    fi
    cat "$stdout_file" "$stderr_file" >"$combined_file"
    if [[ "$retry_safe" == true && "$attempt" -lt "$max_attempts" ]] \
      && { [[ "$status" -eq 124 ]] || adb_failure_is_transient "$combined_file"; }; then
      echo "adb transient failure for safe command (attempt $attempt/$max_attempts): $*" >&2
      cat "$stderr_file" >&2
      adb_reconnect_and_wait || true
      attempt=$((attempt + 1))
      rm -f "$stdout_file" "$stderr_file" "$combined_file"
      continue
    fi
    cat "$stdout_file"
    cat "$stderr_file" >&2
    echo "adb command failed or timed out: $*" >&2
    rm -f "$stdout_file" "$stderr_file" "$combined_file"
    return "$status"
  done
}

adb_target() {
  adb_command true "$@"
}

adb_target_once() {
  adb_command false "$@"
}

adb_capture() {
  adb_command true -s "$SERIAL" "$@"
}

adb_shell_capture() {
  adb_capture shell "$@"
}

clear_rpc_forward() {
  if [[ "$FORWARD_ACTIVE" == true ]]; then
    adb_target -s "$SERIAL" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
    FORWARD_ACTIVE=false
  fi
}

rpc() {
  local stage="$1"
  local body="$2"
  local output_file=""
  local status=0
  output_file="$(mktemp)"
  set +e
  "${CURL[@]}" --fail --silent \
    -H 'content-type: application/json' \
    -H "authorization: Bearer $RPC_CAPABILITY" \
    -d "$body" \
    "http://127.0.0.1:$HOST_PORT/rpc" >"$output_file"
  status=$?
  set -e
  if [[ "$status" -ne 0 ]]; then
    echo "RPC transport failed during $stage (curl exit $status, host tcp:$HOST_PORT -> device tcp:${RPC_PORT:-unknown}); restart/recover may be required" >&2
    if [[ -s "$output_file" ]]; then
      printf 'RPC partial response during %s: ' "$stage" >&2
      head -c 400 "$output_file" >&2 || true
      printf '\n' >&2
    fi
    rm -f "$output_file"
    return "$status"
  fi
  cat "$output_file"
  rm -f "$output_file"
}

cleanup() {
  local status=$?
  local cleanup_failed=false
  trap - EXIT INT TERM
  set +e

  clear_rpc_forward
  adb_target -s "$SERIAL" shell "am force-stop '$PKG'; am force-stop '$TEST_PKG'; am force-stop '$RETROARCH_PKG'" >/dev/null 2>&1 || true
  adb_target -s "$SERIAL" shell "rm -rf '$FIXTURE_A' '$FIXTURE_B' '$FIXTURE_A_UNAVAILABLE' '$FIXTURE_B_UNAVAILABLE'" >/dev/null 2>&1 || cleanup_failed=true
  restore_checkpoint_files || cleanup_failed=true
  restore_private_state || cleanup_failed=true
  restore_appop || cleanup_failed=true
  if [[ "$cleanup_failed" == false ]]; then
    release_device_lock || cleanup_failed=true
  fi
  [[ -n "$RUN_DIR" ]] && rm -rf "$RUN_DIR"

  if [[ "$cleanup_failed" == true && "$status" -eq 0 ]]; then
    echo "Android game discovery check cleanup failed after successful run" >&2
    exit 1
  fi
  if [[ "$cleanup_failed" == true ]]; then
    echo "Primary Android game discovery check status was $status" >&2
    echo "Android game discovery check cleanup also failed; preserving primary status $status" >&2
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

remote_state() {
  local state
  state="$(adb_target -s "$SERIAL" shell "if test -e '$1'; then echo present; else echo absent; fi" | tr -d '\r\n')" || return 1
  case "$state" in
    present|absent) printf '%s' "$state" ;;
    *) echo "Cannot classify remote path: $1" >&2; return 1 ;;
  esac
}

acquire_device_lock() {
  adb_target -s "$SERIAL" shell "mkdir -p '$ANDROID_STORAGE_ROOT' && if mkdir '$LOCK_REMOTE' 2>/dev/null; then printf '%s\n' 'pid=$$ started=$(date -u +%Y-%m-%dT%H:%M:%SZ)' > '$LOCK_OWNER_REMOTE'; else echo 'Android game discovery check lock is held at $LOCK_REMOTE. If this is stale, remove it manually only after verifying no discovery check is running.' >&2; exit 75; fi"
  LOCK_ACQUIRED=true
}

release_device_lock() {
  if [[ "$LOCK_ACQUIRED" != true ]]; then
    return 0
  fi
  adb_target -s "$SERIAL" shell "rm -rf '$LOCK_REMOTE'" >/dev/null 2>&1 || {
    echo "Android game discovery check failed to release $LOCK_REMOTE" >&2
    return 1
  }
  LOCK_ACQUIRED=false
}

backup_checkpoint_files() {
  acquire_device_lock
  adb_target -s "$SERIAL" shell "mkdir '$BACKUP_REMOTE'"
  BACKUP_CREATED=true
  local state
  state="$(remote_state "$ANDROID_STORAGE_ROOT/catalog")" || return 1
  [[ "$state" != present ]] || CATALOG_DIR_WAS_PRESENT=true
  state="$(remote_state "$DEVICE_REMOTE")" || return 1
  if [[ "$state" == present ]]; then
    adb_target -s "$SERIAL" shell "cp '$DEVICE_REMOTE' '$BACKUP_REMOTE/device.yaml' && cmp -s '$DEVICE_REMOTE' '$BACKUP_REMOTE/device.yaml'" || return 1
    DEVICE_WAS_PRESENT=true
  fi
  state="$(remote_state "$GAMES_REMOTE")" || return 1
  if [[ "$state" == present ]]; then
    adb_target -s "$SERIAL" shell "cp '$GAMES_REMOTE' '$BACKUP_REMOTE/games.yaml' && cmp -s '$GAMES_REMOTE' '$BACKUP_REMOTE/games.yaml'" || return 1
    GAMES_WAS_PRESENT=true
  fi
  state="$(remote_state "$RELEASES_REMOTE")" || return 1
  if [[ "$state" == present ]]; then
    adb_target -s "$SERIAL" shell "cp '$RELEASES_REMOTE' '$BACKUP_REMOTE/releases.yaml' && cmp -s '$RELEASES_REMOTE' '$BACKUP_REMOTE/releases.yaml'" || return 1
    RELEASES_WAS_PRESENT=true
  fi
  # Only a complete, verified backup permits mutation or absence-based cleanup.
  CHECKPOINT_RESTORE_NEEDED=true
  adb_target -s "$SERIAL" shell "mkdir -p '$ANDROID_STORAGE_ROOT/catalog'"

}

restore_checkpoint_files() {
  local restore_failed=false
  if [[ "$CHECKPOINT_RESTORE_NEEDED" != true ]]; then
    if [[ "$BACKUP_CREATED" == true ]]; then
      echo "Incomplete backup retained with device lock; no checkpoint files changed" >&2
      return 1
    fi
    return 0
  fi
  if [[ "$DEVICE_WAS_PRESENT" == true ]]; then
    adb_target -s "$SERIAL" shell "cp '$BACKUP_REMOTE/device.yaml' '$DEVICE_REMOTE' && cmp -s '$BACKUP_REMOTE/device.yaml' '$DEVICE_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  else
    adb_target -s "$SERIAL" shell "rm -f '$DEVICE_REMOTE' && test ! -e '$DEVICE_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$GAMES_WAS_PRESENT" == true ]]; then
    adb_target -s "$SERIAL" shell "cp '$BACKUP_REMOTE/games.yaml' '$GAMES_REMOTE' && cmp -s '$BACKUP_REMOTE/games.yaml' '$GAMES_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  else
    adb_target -s "$SERIAL" shell "rm -f '$GAMES_REMOTE' && test ! -e '$GAMES_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$RELEASES_WAS_PRESENT" == true ]]; then
    adb_target -s "$SERIAL" shell "cp '$BACKUP_REMOTE/releases.yaml' '$RELEASES_REMOTE' && cmp -s '$BACKUP_REMOTE/releases.yaml' '$RELEASES_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  else
    adb_target -s "$SERIAL" shell "rm -f '$RELEASES_REMOTE' && test ! -e '$RELEASES_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$CATALOG_DIR_WAS_PRESENT" != true ]]; then
    adb_target -s "$SERIAL" shell "rmdir '$ANDROID_STORAGE_ROOT/catalog' 2>/dev/null || test ! -e '$ANDROID_STORAGE_ROOT/catalog'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$restore_failed" == true ]]; then
    echo "Checkpoint restore failed; backup and lock retained" >&2
    return 1
  fi
  adb_target -s "$SERIAL" shell "rm -rf '$BACKUP_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  [[ "$restore_failed" == false ]]
}

move_private_state_aside() {
  adb_target -s "$SERIAL" shell "run-as '$PKG' sh -c 'set -e; root=\"no_backup/korrid-state\"; backup=\"no_backup/korrid-state.android-game-discovery-check.$$\"; rm -rf \"\$backup\"; mkdir -p \"\$backup\"; for name in steamgriddb.credential game-discovery steamgriddb-enrichment; do if [ -e \"\$root/\$name\" ]; then mv \"\$root/\$name\" \"\$backup/\$name\"; fi; done'"
  PRIVATE_STATE_MOVED=true
}

restore_private_state() {
  if [[ "$PRIVATE_STATE_MOVED" != true ]]; then
    return 0
  fi
  adb_target -s "$SERIAL" shell "run-as '$PKG' sh -c 'set -e; root=\"no_backup/korrid-state\"; backup=\"no_backup/korrid-state.android-game-discovery-check.$$\"; mkdir -p \"\$root\"; rm -rf \"\$root/steamgriddb.credential\" \"\$root/game-discovery\" \"\$root/steamgriddb-enrichment\"; if [ -d \"\$backup\" ]; then for name in steamgriddb.credential game-discovery steamgriddb-enrichment; do if [ -e \"\$backup/\$name\" ]; then mv \"\$backup/\$name\" \"\$root/\$name\"; fi; done; rm -rf \"\$backup\"; fi'" >/dev/null 2>&1 || {
    echo "Android game discovery check failed to restore private discovery state" >&2
    return 1
  }
}

current_uid_appop_mode() {
  local line
  local mode
  line="$(adb_shell_capture "cmd appops get '$PKG' MANAGE_EXTERNAL_STORAGE 2>/dev/null || true" | tr -d '\r' || true)"
  mode="$(printf '%s\n' "$line" | sed -nE 's/^[[:space:]]*Uid mode: MANAGE_EXTERNAL_STORAGE: ([a-z_]+).*/\1/p' | tail -1)"
  printf '%s\n' "${mode:-default}"
}

capture_appop() {
  PRIOR_UID_APPOP_MODE="$(current_uid_appop_mode)"
  printf 'Prior MANAGE_EXTERNAL_STORAGE UID app-op: %s\n' "$PRIOR_UID_APPOP_MODE"
}

restore_appop() {
  if [[ -z "$PRIOR_UID_APPOP_MODE" ]]; then
    return 0
  fi
  adb_target -s "$SERIAL" shell "cmd appops set --uid '$PKG' MANAGE_EXTERNAL_STORAGE '$PRIOR_UID_APPOP_MODE'" >/dev/null 2>&1 || {
    echo "Android game discovery check failed to restore MANAGE_EXTERNAL_STORAGE UID app-op to $PRIOR_UID_APPOP_MODE" >&2
    return 1
  }
}

set_appop_and_require_effective_mode() {
  local requested="$1"
  shift
  local expected_modes=("$@")
  local effective=""
  adb_target -s "$SERIAL" shell "cmd appops set --uid '$PKG' MANAGE_EXTERNAL_STORAGE '$requested'" >/dev/null
  effective="$(current_uid_appop_mode)"
  for mode in "${expected_modes[@]}"; do
    if [[ "$effective" == "$mode" ]]; then
      printf 'MANAGE_EXTERNAL_STORAGE UID app-op after %s: %s\n' "$requested" "$effective"
      return 0
    fi
  done
  echo "Expected MANAGE_EXTERNAL_STORAGE UID app-op after $requested to be one of: ${expected_modes[*]}; got $effective" >&2
  exit 1
}

write_controlled_config() {
  mkdir -p "$RUN_DIR/catalog"
  printf '{}\n' >"$RUN_DIR/device.yaml"
  printf '{}\n' >"$RUN_DIR/catalog/games.yaml"
  printf '{}\n' >"$RUN_DIR/catalog/releases.yaml"
  adb_target -s "$SERIAL" push "$RUN_DIR/device.yaml" "$DEVICE_REMOTE" >/dev/null
  adb_target -s "$SERIAL" push "$RUN_DIR/catalog/games.yaml" "$GAMES_REMOTE" >/dev/null
  adb_target -s "$SERIAL" push "$RUN_DIR/catalog/releases.yaml" "$RELEASES_REMOTE" >/dev/null
  adb_target -s "$SERIAL" exec-out cat "$DEVICE_REMOTE" | cmp -s "$RUN_DIR/device.yaml" -
  adb_target -s "$SERIAL" exec-out cat "$GAMES_REMOTE" | cmp -s "$RUN_DIR/catalog/games.yaml" -
  adb_target -s "$SERIAL" exec-out cat "$RELEASES_REMOTE" | cmp -s "$RUN_DIR/catalog/releases.yaml" -
}

stage_fixtures() {
  printf 'KORRI-U9-FIRST-ROM\n' >"$RUN_DIR/U9 First.gba"
  printf 'KORRI-U9-SECOND-ROM\n' >"$RUN_DIR/U9 Second.gba"
  adb_target -s "$SERIAL" shell "rm -rf '$FIXTURE_A' '$FIXTURE_B' '$FIXTURE_A_UNAVAILABLE' '$FIXTURE_B_UNAVAILABLE'; mkdir -p '$FIXTURE_A' '$FIXTURE_B'"
  adb_target -s "$SERIAL" push "$RUN_DIR/U9 First.gba" "$FIXTURE_A/U9 First.gba" >/dev/null
  adb_target -s "$SERIAL" push "$RUN_DIR/U9 Second.gba" "$FIXTURE_B/U9 Second.gba" >/dev/null
}

move_fixture_roots_aside() {
  adb_target -s "$SERIAL" shell "set -e; mv '$FIXTURE_A' '$FIXTURE_A_UNAVAILABLE'; mv '$FIXTURE_B' '$FIXTURE_B_UNAVAILABLE'"
}

restore_fixture_roots() {
  adb_target -s "$SERIAL" shell "set -e; mv '$FIXTURE_A_UNAVAILABLE' '$FIXTURE_A'; mv '$FIXTURE_B_UNAVAILABLE' '$FIXTURE_B'"
}

build_and_install_instrumentation() {
  cd "$ROOT/clients/android"
  ./gradlew --quiet :app:assembleDebugAndroidTest
  test -f "$TEST_APK"
  adb_target -s "$SERIAL" install -r "$TEST_APK" >/dev/null
}

run_discovery_instrumentation() {
  local action="$1"
  shift
  local log_file="$RUN_DIR/instrument-$action.log"
  local output=""
  local status=0
  set +e
  output="$(adb_target_once -s "$SERIAL" shell am instrument -w \
    -e class com.limelight.KorriGameDiscoveryDebugTest \
    -e korriDebugDiscoveryAction "$action" \
    "$@" \
    "$TEST_PKG/androidx.test.runner.AndroidJUnitRunner" 2>&1)"
  status=$?
  set -e
  printf '%s\n' "$output" | tee "$log_file"
  if ! korri_android_instrumentation_passed "$status" "$log_file"; then
    echo "Android game discovery instrumentation action failed: $action" >&2
    exit 1
  fi
}

register_folder() {
  local folder="$1"
  run_discovery_instrumentation register -e gameFolderPath "$folder"
}

resumed_activity() {
  adb_shell_capture "dumpsys activity activities 2>/dev/null | grep -m1 -E '(^|[[:space:]])((topResumedActivity|mResumedActivity)[:=]|Resumed:)'" | tr -d '\r'
}

retroarch_is_resumed() {
  local activity="$1"
  grep -F "$RETROARCH_PKG/" <<<"$activity" >/dev/null
}

assert_retroarch_not_resumed() {
  local activity=""
  if ! activity="$(resumed_activity)" || [[ -z "$activity" ]]; then
    echo 'Could not confirm the foreground activity before launch scheduling' >&2
    exit 1
  fi
  if retroarch_is_resumed "$activity"; then
    echo "RetroArch is already foreground before launch scheduling: $activity" >&2
    exit 1
  fi
  printf 'Top activity before discovered launch: %s\n' "${activity:-unknown}"
}

wait_retroarch_resumed_after_launch() {
  local activity=""
  for _ in $(seq 1 40); do
    if activity="$(resumed_activity)" && retroarch_is_resumed "$activity"; then
      printf 'Final top activity after discovered launch: %s\n' "$activity"
      return 0
    fi
    sleep 0.5
  done
  printf 'Final top activity after discovered launch: %s\n' "${activity:-unknown}"
  echo "Discovered launch did not foreground RetroArch package $RETROARCH_PKG" >&2
  exit 1
}

launch_local_spec() {
  local launch_response="$1"
  local compact_spec_json=""
  local envelope_json=""
  local helper_stderr=""
  local launch_result=""
  compact_spec_json="$(jq -c '.outcome.payload' <<<"$launch_response")"
  envelope_json="$(
    {
      printf '%s\n' "$compact_spec_json"
      printf '%s\n' "$RPC_PORT"
      printf '%s' "$RPC_CAPABILITY"
    } | jq -Rsc '
      split("\n") as $parts
      | {
          expectedSigner: {
            port: ($parts[1] | tonumber),
            capability: $parts[2]
          },
          spec: ($parts[0] | fromjson)
        }
    '
  )"
  helper_stderr="$(mktemp)"
  if ! launch_result="$(printf '%s' "$envelope_json" \
    | "$DEBUG_LAUNCH_LOCAL_SH" "$SERIAL" "$PKG" "$DEVTOOLS_HOST_PORT" 2>"$helper_stderr")"; then
    echo 'Trusted same-process portal launchLocal helper failed' >&2
    if [[ -s "$helper_stderr" ]]; then
      sed 's/^/launchLocal helper: /' "$helper_stderr" >&2
    fi
    rm -f "$helper_stderr"
    exit 1
  fi
  rm -f "$helper_stderr"
  if ! jq -e '. == {"_tag":"LaunchScheduled"}' <<<"$launch_result" >/dev/null; then
    echo 'Trusted same-process portal launchLocal helper returned an unexpected schedule ack' >&2
    exit 1
  fi
  printf 'Trusted same-process portal launchLocal schedule ack: LaunchScheduled\n'
}

recover_rpc_details() {
  local label="${1:-embedded korrid RPC}"
  local authority_json=""
  local helper_stderr=""
  local port=""
  local capability=""

  helper_stderr="$(mktemp)"
  for authority_attempt in 1 2 3; do
    : >"$helper_stderr"
    if authority_json="$({ "$DEBUG_CAPABILITY_SH" "$SERIAL" "$PKG" --json "$DEVTOOLS_HOST_PORT"; } 2>"$helper_stderr")"; then
      break
    fi
    authority_json=""
    if [[ "$authority_attempt" -eq 3 ]]; then
      echo "Could not recover embedded korrid RPC details during $label from trusted portal DevTools" >&2
      if [[ -s "$helper_stderr" ]]; then
        sed 's/^/debug authority helper: /' "$helper_stderr" >&2
      fi
      rm -f "$helper_stderr"
      exit 1
    fi
  done
  rm -f "$helper_stderr"

  if ! jq -e '
    type == "object" and (keys == ["capability", "port"])
    and (.port | type == "number" and floor == . and . >= 1 and . <= 65535)
    and (.capability | type == "string" and test("^[0-9a-f]{64}$"))
  ' <<<"$authority_json" >/dev/null; then
    echo "Debug authority helper returned invalid RPC authority JSON during $label" >&2
    exit 1
  fi
  port="$(jq -er '.port' <<<"$authority_json")"
  capability="$(jq -er '.capability' <<<"$authority_json")"
  if [[ ! "$port" =~ ^[0-9]+$ || "$port" -lt 1 || "$port" -gt 65535 || ! "$capability" =~ ^[0-9a-f]{64}$ ]]; then
    echo "Debug authority helper returned invalid RPC authority fields during $label" >&2
    exit 1
  fi

  RPC_PORT="$port"
  RPC_CAPABILITY="$capability"
  clear_rpc_forward
  adb_target -s "$SERIAL" forward "tcp:$HOST_PORT" "tcp:$RPC_PORT"
  FORWARD_ACTIVE=true
  printf 'Recovered %s: host tcp:%s -> device tcp:%s via trusted portal DevTools\n' "$label" "$HOST_PORT" "$RPC_PORT"
}

restart_portal_and_recover() {
  local label="$1"
  clear_rpc_forward
  # Preserve the scan metric emitted by instrumentation, but do not reuse its
  # process while Android is tearing the test runner down. A fresh production
  # process gives recovery a stable RPC server to discover.
  adb_target -s "$SERIAL" shell "am force-stop '$PKG'"
  adb_target -s "$SERIAL" shell "am start -n '$PKG/com.limelight.KorriShellActivity'" >/dev/null
  recover_rpc_details "$label"
}

wait_discovery_idle() {
  local label="$1"
  local response=""
  local state=""
  for _ in $(seq 1 60); do
    response="$(rpc "$label snapshot" '{"_tag":"app.discovery.snapshot","payload":{}}')"
    state="$(jq -r '.outcome.payload.state._tag // empty' <<<"$response")"
    if [[ "$state" == "Enriching" ]]; then
      assert_two_u9_games_listable "while Enriching"
    fi
    if [[ "$state" == "Idle" || "$state" == "Problem" ]]; then
      printf 'Discovery %s snapshot: %s\n' "$label" "$response"
      return 0
    fi
    sleep 1
  done
  echo "Discovery did not settle after $label: state=$state response=$response" >&2
  exit 1
}

assert_two_u9_games_listable() {
  local label="$1"
  local response
  response="$(rpc "$label local-games list" '{"_tag":"app.local-games.list","payload":{}}')"
  if ! jq -e '
    .outcome._tag == "Ok"
    and ([.outcome.payload.games[] | select(.title == "U9 First" or .title == "U9 Second")] | length) == 2
  ' <<<"$response" >/dev/null; then
    echo "Expected exactly two U9 games listable $label: $response" >&2
    exit 1
  fi
  printf 'Local games %s: %s\n' "$label" "$response"
}

assert_two_u9_games_unavailable() {
  local label="$1"
  local response
  response="$(rpc "$label local-games list" '{"_tag":"app.local-games.list","payload":{}}')"
  if ! jq -e --argjson selectedLocationIds "$selected_location_ids" '
    .outcome._tag == "Ok"
    and .outcome.payload.games == []
    and (.outcome.payload.failures | type == "array" and length == 2)
    and all(.outcome.payload.failures[];
      .code == "LocalRouteUnavailable"
      and (.message as $message
        | any($selectedLocationIds[]; . as $locationId | $message | contains($locationId))))
  ' <<<"$response" >/dev/null; then
    echo "Expected two unavailable selected-location routes $label: $response" >&2
    exit 1
  fi
  printf 'Unavailable local games %s: %s\n' "$label" "$response"
}

u9_game_id() {
  rpc "lookup U9 game id" '{"_tag":"app.local-games.list","payload":{}}' \
    | jq -r '.outcome.payload.games[] | select(.title == "U9 First") | .id' \
    | head -1
}

latest_scan_metric() {
  adb_capture logcat -d -s KorriDiscovery:I 2>/dev/null \
    | tr -d '\r' \
    | sed -n 's/.*KorriDiscovery: //p' \
    | jq -c 'select(.schema == "korri.discovery.scanMetrics.v1")' \
    | tail -1
}

assert_latest_hashed_bytes() {
  local expected="$1"
  local label="$2"
  local metric
  metric="$(latest_scan_metric)"
  if [[ -z "$metric" ]]; then
    echo "No KorriDiscovery scan metric was logged for $label" >&2
    exit 1
  fi
  if ! jq -e --argjson expected "$expected" '.hashedBytes == $expected and (.durationMs | type == "number")' <<<"$metric" >/dev/null; then
    echo "Unexpected scan metric for $label: $metric" >&2
    exit 1
  fi
  printf 'Scan metric %s: %s\n' "$label" "$metric"
}

assert_latest_hashed_bytes_nonzero() {
  local label="$1"
  local metric
  metric="$(latest_scan_metric)"
  if [[ -z "$metric" ]]; then
    echo "No KorriDiscovery scan metric was logged for $label" >&2
    exit 1
  fi
  if ! jq -e '.hashedBytes > 0 and (.durationMs | type == "number")' <<<"$metric" >/dev/null; then
    echo "Expected nonzero hashed bytes for $label: $metric" >&2
    exit 1
  fi
  printf 'Scan metric %s: %s\n' "$label" "$metric"
}

rescan() {
  rpc "$1 rescan" '{"_tag":"app.discovery.rescan","payload":{}}' >/dev/null
  wait_discovery_idle "$1"
}

if [[ "$SERIAL" == *:* ]]; then
  timeout 15 "$ADB_BIN" connect "$SERIAL" >/dev/null || true
fi
if ! timeout 15 "$ADB_BIN" -s "$SERIAL" wait-for-device; then
  echo "Android target is not reachable: $SERIAL" >&2
  exit 1
fi
RUN_DIR="$(mktemp -d)"
PRIOR_TOP_ACTIVITY="$(adb_shell_capture "dumpsys activity activities 2>/dev/null | grep -m1 -E '(^|[[:space:]])((topResumedActivity|mResumedActivity)[:=]|Resumed:)'" | tr -d '\r' || true)"
printf 'Prior top activity: %s\n' "${PRIOR_TOP_ACTIVITY:-unknown}"
capture_appop

test -f "$APK" || { echo "Korri debug APK is missing; run nix run .#korrid-check first" >&2; exit 1; }
adb_target -s "$SERIAL" install -r "$APK" >/dev/null
adb_target -s "$SERIAL" shell "am force-stop '$PKG'"
backup_checkpoint_files
move_private_state_aside
write_controlled_config
stage_fixtures
build_and_install_instrumentation

# Start from an allowed state, then let the existing smoke gate install, open,
# and prove the protected RPC surface for this explicit device.
set_appop_and_require_effective_mode allow allow
"$CRATE/android-smoke.sh" "$SERIAL"
recover_rpc_details "initial smoke launch"
settings_response="$(rpc "settings snapshot" '{"_tag":"system.settings.snapshot","payload":{}}')"
if ! jq -e '.outcome.payload.steamGridDbCredential == "NotConfigured"' <<<"$settings_response" >/dev/null; then
  echo "SteamGridDB credential was not isolated for the offline device gate: $settings_response" >&2
  exit 1
fi
printf 'SteamGridDB credential status during gate: NotConfigured\n'

adb_target -s "$SERIAL" logcat -c
register_folder "$FIXTURE_A"
restart_portal_and_recover "after first folder registration"
wait_discovery_idle "after first folder registration"
assert_latest_hashed_bytes_nonzero "after first folder registration"
register_folder "$FIXTURE_B"
restart_portal_and_recover "after second folder registration"
wait_discovery_idle "after second folder registration"
assert_two_u9_games_listable "after two folder registrations"
assert_latest_hashed_bytes_nonzero "after second folder registration"

rescan "repeat rescan"
assert_two_u9_games_listable "after repeat rescan"
assert_latest_hashed_bytes 0 "unchanged repeat rescan"

adb_target -s "$SERIAL" shell "cp '$FIXTURE_A/U9 First.gba' '$FIXTURE_B/U9 First Duplicate.gba'"
rescan "duplicate-content rescan"
assert_two_u9_games_listable "after duplicate-content rescan"
assert_latest_hashed_bytes_nonzero "duplicate-content rescan"
rescan "unchanged duplicate-content rescan"
assert_latest_hashed_bytes 0 "unchanged duplicate-content rescan"

selected_location_ids="$(rpc "selected locations snapshot" '{"_tag":"app.discovery.snapshot","payload":{}}' \
  | jq -c '.outcome.payload.locations | map(.id)')"
if ! jq -e 'length > 0' <<<"$selected_location_ids" >/dev/null; then
  echo "No selected discovery locations were available before all-files denial" >&2
  exit 1
fi
set_appop_and_require_effective_mode ignore ignore
# TrebleDroid's userdebug FUSE layer still exposes adb-created files to this
# debug package under UID mode ignore. Move only this run's PID-unique fixture
# roots aside to exercise the same unavailable-location and recovery behavior.
move_fixture_roots_aside
restart_portal_and_recover "after selected locations become unavailable"
rescan "with selected locations unavailable"
denied_snapshot="$(rpc "denied all-files snapshot" '{"_tag":"app.discovery.snapshot","payload":{}}')"
if ! jq -e --argjson selectedLocationIds "$selected_location_ids" '
  .outcome._tag == "Ok"
  and any(.outcome.payload.diagnostics[];
    (.code == "StorageUnavailable"
      or .code == "EntryUnavailable"
      or .code == "DiscoveryStorageUnavailable")
    and (.locationId as $locationId
      | $locationId != null and ($selectedLocationIds | index($locationId))))
' <<<"$denied_snapshot" >/dev/null; then
  echo "Unavailable selected locations did not yield a storage diagnostic under ignored all-files access: $denied_snapshot selectedLocationIds=$selected_location_ids" >&2
  exit 1
fi
assert_two_u9_games_unavailable "while selected locations are unavailable"
restore_fixture_roots
set_appop_and_require_effective_mode allow allow
restart_portal_and_recover "after all-files recovery"
rescan "after all-files recovery"
assert_two_u9_games_listable "after all-files recovery"

# Launch is the terminal proof. Android may retire the background Korri process
# after RetroArch takes focus, so no later assertion may depend on its RPC.
if ! adb_shell_capture "pm path '$RETROARCH_PKG'" | grep -q '^package:'; then
  echo "Required RetroArch package is not installed: $RETROARCH_PKG" >&2
  exit 1
fi
game_id="$(u9_game_id)"
if [[ -z "$game_id" ]]; then
  echo "Could not find discovered U9 First game id" >&2
  exit 1
fi
launch_response="$(rpc "launch discovered U9 First" "{\"_tag\":\"app.local-games.launch\",\"payload\":{\"gameId\":$(jq -n --arg id "$game_id" '$id')}}")"
if ! jq -e '
  .outcome._tag == "Ok"
  and .outcome.payload.launcherId == "retroarch"
  and .outcome.payload.component.packageName == "com.korri.retroarch"
  and .outcome.payload.extras.LIBRETRO == "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so"
  and (.outcome.payload.extras.ROM | endswith("/U9 First.gba"))
  and (.outcome.payload.integrity | type == "string" and length > 0)
' <<<"$launch_response" >/dev/null; then
  echo "Discovered game did not produce the signed RetroArch+mGBA launch route: $launch_response" >&2
  exit 1
fi
assert_retroarch_not_resumed
launch_local_spec "$launch_response"
wait_retroarch_resumed_after_launch

printf 'Android game discovery check passed on %s\n' "$SERIAL"
