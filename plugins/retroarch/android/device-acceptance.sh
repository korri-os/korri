#!/usr/bin/env nix-shell
#! nix-shell -i bash -p python3 python3Packages.pyyaml bash coreutils curl diffutils gnugrep gnused imagemagick jq tesseract android-tools websocat
# shellcheck shell=bash
set -Eeuo pipefail

# Failure diagnostics intentionally expose only a fixed phase label and source
# line. Never include BASH_COMMAND, RPC payloads, capabilities, or authority.
STAGE="preflight"
report_stage_failure() {
  local status="$1"
  local line="$2"
  printf 'RetroArch acceptance failed: stage=%s line=%s\n' "$STAGE" "$line" >&2
  return "$status"
}
trap 'status=$?; report_stage_failure "$status" "$LINENO"' ERR

usage() {
  echo 'usage: device-acceptance.sh <adb-serial> <exact-device-model> <exact-hardware-serial>' >&2
  exit 2
}

[[ $# -eq 3 ]] || usage
SERIAL="$1"
EXPECTED_MODEL="$2"
EXPECTED_HARDWARE_SERIAL="$3"
[[ -n "$SERIAL" && -n "$EXPECTED_MODEL" && -n "$EXPECTED_HARDWARE_SERIAL" ]] || usage
KORRI_PACKAGE="${KORRI_PACKAGE:-com.simonwjackson.korri.debug}"
KORRI_ACTIVITY="$KORRI_PACKAGE/com.limelight.KorriShellActivity"
KORRI_GAME_COMPONENT="$KORRI_PACKAGE/com.limelight.Game"
FORK_PACKAGE="com.korri.retroarch"
STOCK_PACKAGE="com.retroarch.aarch64"
SYSTEM_DIR="/storage/emulated/0/korri/system"
STATES_ROOT_DIR="/storage/emulated/0/korri/states"
STATE_DIR="$STATES_ROOT_DIR/mGBA"
STATE_FILE="$STATE_DIR/wl4.state.auto"
SAVE_DIR="/storage/emulated/0/korri/saves"
SAVE_FILE="$SAVE_DIR/wl4.srm"
SCREENSHOTS_DIR="/storage/emulated/0/korri/screenshots"
HOST_PORT="${KORRI_ACCEPTANCE_HOST_PORT:-43119}"
ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
DEBUG_CAPABILITY_SH="${KORRI_ANDROID_DEBUG_CAPABILITY_SH:-$ROOT/services/korrid/android-debug-capability.sh}"
DEBUG_PORTAL_RELOAD_SH="${KORRI_ANDROID_DEBUG_PORTAL_RELOAD_SH:-$ROOT/services/korrid/android-debug-reload-portal.sh}"
DEBUG_PORTAL_FOCUS_GAME_SH="${KORRI_ANDROID_DEBUG_PORTAL_FOCUS_GAME_SH:-$ROOT/services/korrid/android-debug-focus-portal-game.sh}"
ANDROID_STORAGE_ROOT="/sdcard/korri"
DEVICE_REMOTE="$ANDROID_STORAGE_ROOT/device.yaml"
GAMES_REMOTE="$ANDROID_STORAGE_ROOT/catalog/games.yaml"
RELEASES_REMOTE="$ANDROID_STORAGE_ROOT/catalog/releases.yaml"
CATALOG_DIR_WAS_PRESENT=false
RETROARCH_CONFIG_REMOTE="$ANDROID_STORAGE_ROOT/retroarch.cfg"
CHECKPOINT_DEVICE="$ROOT/docs/research/retroarch-plugin-route/device.yaml"
CHECKPOINT_GAMES="$ROOT/docs/research/retroarch-plugin-route/catalog/games.yaml"
CHECKPOINT_RELEASES="$ROOT/docs/research/retroarch-plugin-route/catalog/releases.yaml"
RUN_NONCE="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
[[ "$RUN_NONCE" =~ ^[0-9a-f]{32}$ ]] || {
  echo 'could not create a random RetroArch acceptance run nonce' >&2
  exit 1
}
CHECKPOINT_BACKUP_DIR="$ANDROID_STORAGE_ROOT/.retroarch-route-check-backup-$RUN_NONCE"
BACKUP_OWNER_REMOTE="$CHECKPOINT_BACKUP_DIR/.korri-acceptance-owner"
UDP_COMPLETION_MARKER="korri-udp-probe-complete-$RUN_NONCE"
LOCK_REMOTE="$ANDROID_STORAGE_ROOT/.android-app-route-check.lock"
LOCK_OWNER_REMOTE="$LOCK_REMOTE/owner"
DEVICE_WAS_PRESENT=false
GAMES_WAS_PRESENT=false
RELEASES_WAS_PRESENT=false
RETROARCH_CONFIG_WAS_PRESENT=false
STATE_WAS_PRESENT=false
SAVE_WAS_PRESENT=false
SYSTEM_DIR_WAS_PRESENT=false
STATES_ROOT_DIR_WAS_PRESENT=false
STATE_DIR_WAS_PRESENT=false
SAVE_DIR_WAS_PRESENT=false
SCREENSHOTS_DIR_WAS_PRESENT=false
CHECKPOINT_RESTORE_NEEDED=false
BACKUP_CREATED=false
FORWARD_ACTIVE=false
LOCK_ACQUIRED=false
TARGET_STARTED_BY_GATE=false
SHELL_BROUGHT_FORWARD=false
PORTAL_EVIDENCE_DIR=""
capability=""
GATE_CURRENT_LAUNCH=""
GATE_CURRENT_LAUNCH_QUIESCED=false
declare -A GATE_LAUNCH_IDS=()
declare -A GATE_RETROARCH_PIDS=()
KORRI_SERVICE_COMPONENT="$KORRI_PACKAGE/com.limelight.korri.overlay.KorriOverlayService"
ADB_BIN="$(command -v adb)"
adb() {
  if ! timeout 15 "$ADB_BIN" "$@"; then
    echo "adb command failed or timed out: $*" >&2
    return 1
  fi
}

if [[ "$SERIAL" == *:* ]]; then
  timeout 15 adb connect "$SERIAL" >/dev/null || true
fi
ADB=(adb -s "$SERIAL")
if ! timeout 15 "${ADB[@]}" wait-for-device; then
  echo "Android target is not reachable: $SERIAL" >&2
  exit 1
fi
[[ "$("${ADB[@]}" get-state)" == device ]] || {
  echo "Android target is not ready: $SERIAL" >&2
  exit 1
}
ACTUAL_MODEL="$("${ADB[@]}" shell getprop ro.product.model | tr -d '\r\n')"
ACTUAL_HARDWARE_SERIAL="$("${ADB[@]}" shell getprop ro.serialno | tr -d '\r\n')"
[[ "$ACTUAL_MODEL" == "$EXPECTED_MODEL" ]] || {
  echo "device model mismatch: expected '$EXPECTED_MODEL', got '$ACTUAL_MODEL'" >&2
  exit 1
}
[[ "$ACTUAL_HARDWARE_SERIAL" == "$EXPECTED_HARDWARE_SERIAL" ]] || {
  echo "hardware serial mismatch: expected '$EXPECTED_HARDWARE_SERIAL', got '$ACTUAL_HARDWARE_SERIAL'" >&2
  exit 1
}

remote_state() {
  local path="$1"
  local state
  if ! state="$("${ADB[@]}" shell "if test -e '$path'; then echo present; else echo absent; fi" 2>/dev/null | tr -d '\r\n')"; then
    echo "failed to inspect device path $path" >&2
    return 1
  fi
  case "$state" in
    present | absent) printf '%s' "$state" ;;
    *)
      echo "unexpected device path state for $path: $state" >&2
      return 1
      ;;
  esac
}

package_pid() {
  local package="$1"
  local output
  if ! output="$("${ADB[@]}" shell "pidof '$package' || { status=\$?; [ \"\$status\" -eq 1 ] && exit 0; exit \"\$status\"; }" 2>/dev/null)"; then
    echo "failed to probe Android process state for $package" >&2
    return 1
  fi
  printf '%s' "$output" | tr -d '\r\n'
}

assert_accessibility_service_enabled() {
  local enabled_services
  enabled_services="$("${ADB[@]}" shell settings get secure enabled_accessibility_services | tr -d '\r')" || return 1
  grep -Fq "$KORRI_SERVICE_COMPONENT" <<<"$enabled_services" || {
    echo 'Korri gameplay overlay accessibility service is no longer enabled.' >&2
    echo 'Re-enable it manually in Android Settings before relying on Guide.' >&2
    return 1
  }
}

activity_dump_has_resumed_component() {
  local activities="$1"
  local component_needle="$2"
  local line
  while IFS= read -r line; do
    [[ "$line" == *"$component_needle"* ]] || continue
    if [[ "$line" =~ (^|[[:space:]])(topResumedActivity|mResumedActivity|ResumedActivity)[:=] ]]; then
      return 0
    fi
  done <<<"$activities"
  return 1
}

activity_dump_shell_instance_count() {
  local activities="$1"
  local component_needle="$2"
  local line
  local count=0
  # Count only task-history records. dumpsys repeats an ActivityRecord in
  # resumed/focus summaries, while each live Activity instance has one Hist
  # record with a non-negative task id.
  while IFS= read -r line; do
    [[ "$line" == *"* Hist"* && "$line" == *"$component_needle"* ]] || continue
    [[ "$line" =~ ActivityRecord\{.*[[:space:]]t[0-9]+([[:space:]}]|$) ]] || continue
    count=$((count + 1))
  done <<<"$activities"
  printf '%s\n' "$count"
}

assert_single_shell_task_activity() {
  local activities
  local count
  activities="$("${ADB[@]}" shell dumpsys activity activities | tr -d '\r')" || return 1
  count="$(activity_dump_shell_instance_count "$activities" "$KORRI_ACTIVITY")"
  [[ "$count" == 1 ]] || {
    echo "RetroArch acceptance requires exactly one live Korri Shell Activity; observed $count" >&2
    return 1
  }
}

assert_shell_foreground() {
  local activities
  activities="$("${ADB[@]}" shell dumpsys activity activities | tr -d '\r')" || return 1
  activity_dump_has_resumed_component "$activities" "$KORRI_ACTIVITY"
}

bring_existing_shell_task_forward() {
  local activities
  local before_count
  local after_count
  activities="$("${ADB[@]}" shell dumpsys activity activities | tr -d '\r')" || return 1
  before_count="$(activity_dump_shell_instance_count "$activities" "$KORRI_ACTIVITY")"
  [[ "$before_count" == 0 || "$before_count" == 1 ]] || {
    echo "refusing to bring Korri forward with $before_count live Shell Activities" >&2
    return 1
  }

  # Match launcher-icon task semantics. NEW_TASK|RESET_TASK_IF_NEEDED brings an
  # existing launcher-root task forward instead of stacking a bare component
  # instance above it. If Android reclaimed the Activity, one root is created.
  "${ADB[@]}" shell am start --display 0 \
    -a android.intent.action.MAIN \
    -c android.intent.category.LAUNCHER \
    -f 0x10200000 \
    -n "$KORRI_ACTIVITY" >/dev/null || return 1
  for _ in $(seq 1 20); do
    if assert_shell_foreground; then
      activities="$("${ADB[@]}" shell dumpsys activity activities | tr -d '\r')" || return 1
      after_count="$(activity_dump_shell_instance_count "$activities" "$KORRI_ACTIVITY")"
      [[ "$after_count" == 1 ]] || {
        echo "launcher-equivalent Korri return produced $after_count live Shell Activities" >&2
        return 1
      }
      assert_korri_process_unchanged
      return
    fi
    sleep 0.25
  done
  echo 'launcher-equivalent Korri return did not resume the Shell Activity' >&2
  return 1
}

assert_device_awake_and_shell_focused() {
  local power
  local display_zero
  local current_focus
  local focused_component
  power="$("${ADB[@]}" shell dumpsys power | tr -d '\r')" || return 1
  grep -Eq '(^|[[:space:]])mWakefulness=Awake([[:space:]]|$)' <<<"$power" || {
    echo 'RetroArch acceptance requires the device to already be awake; it will not wake or unlock it automatically' >&2
    return 1
  }
  display_zero="$("${ADB[@]}" shell dumpsys window displays | tr -d '\r' \
    | sed -n '/Display: mDisplayId=0/,/Display: mDisplayId=[1-9]/p')" || return 1
  current_focus="$(grep 'mCurrentFocus=' <<<"$display_zero" | head -1 || true)"
  focused_component="$(sed -n 's/.* u[0-9][0-9]* \([^ }]*\)}.*/\1/p' <<<"$current_focus")"
  [[ "$focused_component" == "$KORRI_ACTIVITY" ]] || {
    echo "RetroArch acceptance requires exact Korri Shell window focus before mutation: $current_focus" >&2
    return 1
  }
}

activity_dump_has_live_component() {
  local activities="$1"
  local component_needle="$2"
  local line
  # A resumed/top record is live regardless of whether this Android build
  # includes a task id on the same summary line.
  if activity_dump_has_resumed_component "$activities" "$component_needle"; then
    return 0
  fi
  while IFS= read -r line; do
    [[ "$line" == *"$component_needle"* ]] || continue
    # ActivityRecord task ids are `t0` and above. Framework bookkeeping may
    # retain destroyed/finishing tombstones as `t-1 f`; those are not tasks.
    if [[ "$line" =~ ActivityRecord\{.*[[:space:]]t[0-9]+([[:space:]}]|$) ]]; then
      return 0
    fi
  done <<<"$activities"
  return 1
}

assert_no_artemis_game_activity() {
  local activities
  activities="$("${ADB[@]}" shell dumpsys activity activities | tr -d '\r')" || return 1
  if activity_dump_has_live_component "$activities" "$KORRI_GAME_COMPONENT"; then
    echo 'RetroArch acceptance requires no Artemis Game Activity in any active task, so it cannot exercise Artemis Game SharedPreferences' >&2
    return 1
  fi
}

assert_no_retroarch_activities() {
  local activities
  activities="$("${ADB[@]}" shell dumpsys activity activities | tr -d '\r')" || return 1
  if activity_dump_has_live_component "$activities" "$FORK_PACKAGE/" \
    || activity_dump_has_live_component "$activities" "$STOCK_PACKAGE/"; then
    echo 'RetroArch acceptance requires no Korri or stock RetroArch Activity in any active task before mutation' >&2
    return 1
  fi
}

assert_korri_process_unchanged() {
  local current
  current="$(package_pid "$KORRI_PACKAGE")" || return 1
  [[ -n "$current" && "$current" == "$existing_korri_pid" ]] || {
    echo 'Korri PID changed during RetroArch acceptance' >&2
    return 1
  }
}

assert_pristine_gate_state() {
  local fork_pid
  local stock_pid
  assert_device_awake_and_shell_focused
  assert_shell_foreground
  assert_single_shell_task_activity
  assert_no_artemis_game_activity
  assert_no_retroarch_activities
  fork_pid="$(package_pid "$FORK_PACKAGE")" || return 1
  stock_pid="$(package_pid "$STOCK_PACKAGE")" || return 1
  [[ -z "$fork_pid" && -z "$stock_pid" ]] || {
    echo 'RetroArch acceptance requires both Korri and stock RetroArch processes to be stopped before mutation' >&2
    return 1
  }
  assert_korri_process_unchanged
}

assert_session_idle() {
  local session
  session="$(rpc '{"_tag":"app.session.status","payload":{}}')" || return 1
  jq -e '.outcome._tag == "Ok" and (.outcome.payload.active | not)' \
    <<<"$session" >/dev/null || {
    echo 'an app.session launch is active; end it before RetroArch acceptance mutates device data' >&2
    return 1
  }
}

revalidate_gate_state_after_mutation() {
  assert_pristine_gate_state
  assert_session_idle
}

record_gate_launch() {
  local launch_id="$1"
  [[ -n "$launch_id" ]] || return 1
  GATE_LAUNCH_IDS["$launch_id"]=1
}

is_gate_launch() {
  [[ -n "${GATE_LAUNCH_IDS[$1]:-}" ]]
}

record_gate_pid() {
  local pid="$1"
  [[ -n "$pid" ]] || return 1
  GATE_RETROARCH_PIDS["$pid"]=1
}

new_logcat_marker() {
  local label="$1"
  local marker
  marker="korri-retroarch-acceptance-$label-$$-$(date -u +%s)"
  "${ADB[@]}" shell log -t KorriAcceptance "$marker" >/dev/null
  printf '%s' "$marker"
}

logcat_since() {
  local marker="$1"
  shift
  "${ADB[@]}" logcat -d "$@" \
    | awk -v marker="$marker" 'index($0, marker) { found = 1; next } found'
}

backup_path_if_present() {
  local path="$1"
  local backup_name="$2"
  local flag_name="$3"
  local state
  state="$(remote_state "$path")" || return 1
  if [[ "$state" == present ]]; then
    "${ADB[@]}" shell "cp '$path' '$CHECKPOINT_BACKUP_DIR/$backup_name' && cmp -s '$path' '$CHECKPOINT_BACKUP_DIR/$backup_name'" || return 1
    printf -v "$flag_name" true
  fi
}

record_directory_state() {
  local path="$1"
  local flag_name="$2"
  local state
  state="$(remote_state "$path")" || return 1
  if [[ "$state" == present ]]; then
    printf -v "$flag_name" true
  fi
}

acquire_device_lock() {
  if ! "${ADB[@]}" shell "mkdir -p '$ANDROID_STORAGE_ROOT' && if mkdir '$LOCK_REMOTE' 2>/dev/null; then printf '%s\n' 'pid=$$ started=$(date -u +%Y-%m-%dT%H:%M:%SZ)' > '$LOCK_OWNER_REMOTE'; else echo 'Korri config route lock is held at $LOCK_REMOTE. Remove it manually only after verifying no route check is running.' >&2; exit 75; fi"; then
    echo "RetroArch acceptance could not acquire the device config lock at $LOCK_REMOTE" >&2
    exit 1
  fi
  LOCK_ACQUIRED=true
}

release_device_lock() {
  if [[ "$LOCK_ACQUIRED" != true ]]; then
    return 0
  fi
  if ! "${ADB[@]}" shell "rm -rf '$LOCK_REMOTE'" >/dev/null 2>&1; then
    echo "RetroArch acceptance failed to release the device config lock at $LOCK_REMOTE" >&2
    return 1
  fi
  LOCK_ACQUIRED=false
}

create_owned_backup() {
  if [[ "$(remote_state "$CHECKPOINT_BACKUP_DIR")" == present ]]; then
    echo "refusing pre-existing backup directory: $CHECKPOINT_BACKUP_DIR" >&2
    return 1
  fi
  "${ADB[@]}" shell "if mkdir '$CHECKPOINT_BACKUP_DIR' 2>/dev/null; then printf '%s\\n' '$RUN_NONCE' > '$BACKUP_OWNER_REMOTE'; else exit 73; fi"
  local owner
  owner="$("${ADB[@]}" shell "cat '$BACKUP_OWNER_REMOTE'" | tr -d '\r\n')" || return 1
  [[ "$owner" == "$RUN_NONCE" ]] || {
    echo "backup ownership marker mismatch after creation: $CHECKPOINT_BACKUP_DIR" >&2
    return 1
  }
  BACKUP_CREATED=true
}

remove_owned_backup() {
  [[ "$BACKUP_CREATED" == true ]] || return 0
  local owner
  owner="$("${ADB[@]}" shell "cat '$BACKUP_OWNER_REMOTE'" 2>/dev/null | tr -d '\r\n')" || {
    echo "refusing to remove backup without its ownership marker: $CHECKPOINT_BACKUP_DIR" >&2
    return 1
  }
  [[ "$owner" == "$RUN_NONCE" ]] || {
    echo "refusing to remove backup owned by another run: $CHECKPOINT_BACKUP_DIR" >&2
    return 1
  }
  "${ADB[@]}" shell "test \"\$(cat '$BACKUP_OWNER_REMOTE')\" = '$RUN_NONCE' && rm -rf '$CHECKPOINT_BACKUP_DIR'" || return 1
  BACKUP_CREATED=false
}

restore_checkpoint_files() {
  local restore_failed=false
  local directory=""
  local directory_state
  if [[ "$CHECKPOINT_RESTORE_NEEDED" != true ]]; then
    return 0
  fi

  if [[ "$DEVICE_WAS_PRESENT" == true ]]; then
    "${ADB[@]}" shell "cp '$CHECKPOINT_BACKUP_DIR/device.yaml' '$DEVICE_REMOTE' && cmp -s '$CHECKPOINT_BACKUP_DIR/device.yaml' '$DEVICE_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  else
    "${ADB[@]}" shell "rm -f '$DEVICE_REMOTE' && test ! -e '$DEVICE_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$GAMES_WAS_PRESENT" == true ]]; then
    "${ADB[@]}" shell "cp '$CHECKPOINT_BACKUP_DIR/games.yaml' '$GAMES_REMOTE' && cmp -s '$CHECKPOINT_BACKUP_DIR/games.yaml' '$GAMES_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  else
    "${ADB[@]}" shell "rm -f '$GAMES_REMOTE' && test ! -e '$GAMES_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$RELEASES_WAS_PRESENT" == true ]]; then
    "${ADB[@]}" shell "cp '$CHECKPOINT_BACKUP_DIR/releases.yaml' '$RELEASES_REMOTE' && cmp -s '$CHECKPOINT_BACKUP_DIR/releases.yaml' '$RELEASES_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  else
    "${ADB[@]}" shell "rm -f '$RELEASES_REMOTE' && test ! -e '$RELEASES_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$CATALOG_DIR_WAS_PRESENT" != true ]]; then
    "${ADB[@]}" shell "rmdir '$ANDROID_STORAGE_ROOT/catalog' 2>/dev/null || test ! -e '$ANDROID_STORAGE_ROOT/catalog'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$RETROARCH_CONFIG_WAS_PRESENT" == true ]]; then
    "${ADB[@]}" shell "cp '$CHECKPOINT_BACKUP_DIR/retroarch.cfg' '$RETROARCH_CONFIG_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  else
    "${ADB[@]}" shell "rm -f '$RETROARCH_CONFIG_REMOTE'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$STATE_WAS_PRESENT" == true ]]; then
    "${ADB[@]}" shell "mkdir -p '$STATE_DIR'; cp '$CHECKPOINT_BACKUP_DIR/wl4.state.auto' '$STATE_FILE'" >/dev/null 2>&1 || restore_failed=true
  else
    "${ADB[@]}" shell "rm -f '$STATE_FILE'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$SAVE_WAS_PRESENT" == true ]]; then
    "${ADB[@]}" shell "mkdir -p '$SAVE_DIR'; cp '$CHECKPOINT_BACKUP_DIR/wl4.srm' '$SAVE_FILE'" >/dev/null 2>&1 || restore_failed=true
  else
    "${ADB[@]}" shell "rm -f '$SAVE_FILE'" >/dev/null 2>&1 || restore_failed=true
  fi
  for directory_state in \
    "$STATE_DIR_WAS_PRESENT:$STATE_DIR" \
    "$STATES_ROOT_DIR_WAS_PRESENT:$STATES_ROOT_DIR" \
    "$SAVE_DIR_WAS_PRESENT:$SAVE_DIR" \
    "$SCREENSHOTS_DIR_WAS_PRESENT:$SCREENSHOTS_DIR" \
    "$SYSTEM_DIR_WAS_PRESENT:$SYSTEM_DIR"; do
    if [[ "${directory_state%%:*}" == false ]]; then
      directory="${directory_state#*:}"
      "${ADB[@]}" shell "rmdir '$directory' 2>/dev/null || test ! -e '$directory'" >/dev/null 2>&1 || restore_failed=true
    fi
  done
  if [[ "$restore_failed" == false ]]; then
    assert_korri_process_unchanged || restore_failed=true
    "$DEBUG_PORTAL_RELOAD_SH" "$SERIAL" "$KORRI_PACKAGE" --expect-portal >/dev/null \
      || restore_failed=true
  fi
  if [[ "$restore_failed" == true ]]; then
    echo "RetroArch acceptance failed to restore device data and reload the trusted portal; backup retained at $CHECKPOINT_BACKUP_DIR" >&2
    return 1
  fi
  if ! remove_owned_backup; then
    echo "RetroArch acceptance restored device data but could not remove its owned backup $CHECKPOINT_BACKUP_DIR" >&2
    return 1
  fi
  CHECKPOINT_RESTORE_NEEDED=false
}

cleanup() {
  local status=$?
  STAGE="restoration"
  local cleanup_failed=false
  local safe_to_restore=true
  local target_pid=""
  local confirmed_pid=""
  local replacement_observed=false
  local active_classification="unavailable"
  local pid_classification="absent"
  local tracked_launch_stale=false
  local quiesce_attempt
  if [[ "$TARGET_STARTED_BY_GATE" == true ]]; then
    local active_launch=""
    local controls=""
    local invocation=""
    local session=""
    if [[ "$FORWARD_ACTIVE" == true && -n "$capability" ]] \
      && session="$(rpc '{"_tag":"app.session.status","payload":{}}' 2>/dev/null)" \
      && jq -e '.outcome._tag == "Ok"' <<<"$session" >/dev/null 2>&1; then
      # app.session.status is only the federated-host replacement precondition.
      # Local ownership comes from the launch ID recorded through the exact
      # post-start resume result and a fresh policy-rechecked controls response.
      active_classification="absent"
      active_launch="$(jq -r '.outcome.payload.active.launchId // empty' <<<"$session")"
      if [[ -n "$active_launch" ]] && ! is_gate_launch "$active_launch"; then
        active_classification="replacement"
        safe_to_restore=false
        replacement_observed=true
      elif [[ -n "$GATE_CURRENT_LAUNCH" ]]; then
        if controls="$(controls_for_launch "$GATE_CURRENT_LAUNCH" 2>/dev/null)" \
          && jq -e --arg launchId "$GATE_CURRENT_LAUNCH" '
            .outcome._tag == "Ok" and .outcome.payload.launchId == $launchId
          ' <<<"$controls" >/dev/null 2>&1; then
          active_classification="recorded"
          if ! invocation="$(invoke_control "$GATE_CURRENT_LAUNCH" '@korri:retroarch/quit' 2>/dev/null)" \
            || ! jq -e '
              .outcome._tag == "Ok" and .outcome.payload._tag == "Completed"
            ' <<<"$invocation" >/dev/null 2>&1; then
            safe_to_restore=false
          fi
        elif jq -e '
          .outcome._tag == "Err" and .outcome.payload.reason == "StaleSession"
        ' <<<"$controls" >/dev/null 2>&1; then
          if [[ "$GATE_CURRENT_LAUNCH_QUIESCED" == true ]]; then
            # The main path already paired process teardown with stale-control
            # rejection, so this exact launch is known to be absent.
            tracked_launch_stale=true
          else
            # Stale without paired teardown could mean an unobservable local
            # replacement. Never restore configuration based on absence alone.
            active_classification="replacement"
            safe_to_restore=false
            replacement_observed=true
          fi
        else
          active_classification="unavailable"
          safe_to_restore=false
        fi
      elif [[ -n "$active_launch" ]]; then
        # Defensive: a host session carrying a gate ID is still policy-checked
        # through controls before cleanup invokes any effect.
        if controls="$(controls_for_launch "$active_launch" 2>/dev/null)" \
          && jq -e --arg launchId "$active_launch" '
            .outcome._tag == "Ok" and .outcome.payload.launchId == $launchId
          ' <<<"$controls" >/dev/null 2>&1; then
          active_classification="recorded"
          invoke_control "$active_launch" '@korri:retroarch/quit' >/dev/null 2>&1 \
            || safe_to_restore=false
        else
          active_classification="unavailable"
          safe_to_restore=false
        fi
      fi
    else
      # Without current host replacement state, cleanup cannot prove that
      # restoration would not race another session.
      safe_to_restore=false
    fi
    printf 'RetroArch acceptance cleanup: active-launch=%s\n' "$active_classification" >&2
    if [[ "$replacement_observed" == false ]]; then
      if ! target_pid="$(package_pid "$FORK_PACKAGE")"; then
        pid_classification="unavailable"
        safe_to_restore=false
      elif [[ -n "$target_pid" ]]; then
        if [[ "$tracked_launch_stale" == true ]] \
          || [[ -z "${GATE_RETROARCH_PIDS[$target_pid]:-}" ]]; then
          pid_classification="replacement"
          safe_to_restore=false
          replacement_observed=true
        else
          pid_classification="recorded"
          # Revalidate the package PID immediately before the package-scoped
          # force-stop. A replacement process is never treated as gate-owned.
          if ! confirmed_pid="$(package_pid "$FORK_PACKAGE")"; then
            safe_to_restore=false
          elif [[ "$confirmed_pid" != "$target_pid" ]]; then
            pid_classification="replacement"
            safe_to_restore=false
            replacement_observed=true
          elif ! "${ADB[@]}" shell am force-stop "$FORK_PACKAGE" >/dev/null 2>&1; then
            safe_to_restore=false
          else
            # Android process teardown can lag behind a successful force-stop.
            # Wait boundedly, rejecting any unrecorded replacement observed in
            # the interval instead of restoring files underneath it.
            for ((quiesce_attempt = 1; quiesce_attempt <= 20; quiesce_attempt++)); do
              if ! target_pid="$(package_pid "$FORK_PACKAGE")"; then
                safe_to_restore=false
                break
              fi
              [[ -z "$target_pid" ]] && break
              if [[ -z "${GATE_RETROARCH_PIDS[$target_pid]:-}" ]]; then
                pid_classification="replacement"
                safe_to_restore=false
                replacement_observed=true
                break
              fi
              sleep 0.25
            done
            [[ -z "$target_pid" ]] || safe_to_restore=false
          fi
        fi
      fi
      printf 'RetroArch acceptance cleanup: fork-pid=%s\n' "$pid_classification" >&2
    else
      # A replacement active launch forbids mutation, but read-only PID
      # classification still explains whether the package process is one this
      # gate recorded. Never force-stop from this branch.
      if ! target_pid="$(package_pid "$FORK_PACKAGE")"; then
        pid_classification="unavailable"
      elif [[ -z "$target_pid" ]]; then
        pid_classification="absent"
      elif [[ -n "${GATE_RETROARCH_PIDS[$target_pid]:-}" ]]; then
        pid_classification="recorded"
      else
        pid_classification="replacement"
      fi
      printf 'RetroArch acceptance cleanup: fork-pid=%s\n' "$pid_classification" >&2
    fi
    if [[ "$safe_to_restore" != true ]]; then
      echo "RetroArch acceptance could not quiesce the exact recorded launch; backup and lock retained at $CHECKPOINT_BACKUP_DIR and $LOCK_REMOTE" >&2
      cleanup_failed=true
    fi
  fi
  if [[ "$SHELL_BROUGHT_FORWARD" == true && "$replacement_observed" == false ]]; then
    bring_existing_shell_task_forward >/dev/null 2>&1 || cleanup_failed=true
    assert_shell_foreground || cleanup_failed=true
    assert_single_shell_task_activity || cleanup_failed=true
  fi
  if [[ -n "$PORTAL_EVIDENCE_DIR" && "$status" -eq 0 ]]; then
    rm -rf "$PORTAL_EVIDENCE_DIR"
  fi
  if [[ "$safe_to_restore" == true ]]; then
    if [[ "$CHECKPOINT_RESTORE_NEEDED" != true ]]; then
      remove_owned_backup || cleanup_failed=true
      release_device_lock || cleanup_failed=true
    elif restore_checkpoint_files; then
      release_device_lock || cleanup_failed=true
    else
      cleanup_failed=true
    fi
  fi
  if [[ "$FORWARD_ACTIVE" == true ]]; then
    "${ADB[@]}" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
    FORWARD_ACTIVE=false
  fi
  assert_accessibility_service_enabled || cleanup_failed=true
  if [[ "$cleanup_failed" == true && "$status" -eq 0 ]]; then
    status=1
  fi
  trap - EXIT
  exit "$status"
}
trap cleanup EXIT

provision_checkpoint_files() {
  acquire_device_lock
  create_owned_backup
  backup_path_if_present "$DEVICE_REMOTE" device.yaml DEVICE_WAS_PRESENT
  backup_path_if_present "$GAMES_REMOTE" games.yaml GAMES_WAS_PRESENT
  backup_path_if_present "$RELEASES_REMOTE" releases.yaml RELEASES_WAS_PRESENT
  record_directory_state "$ANDROID_STORAGE_ROOT/catalog" CATALOG_DIR_WAS_PRESENT
  backup_path_if_present "$RETROARCH_CONFIG_REMOTE" retroarch.cfg RETROARCH_CONFIG_WAS_PRESENT
  backup_path_if_present "$STATE_FILE" wl4.state.auto STATE_WAS_PRESENT
  backup_path_if_present "$SAVE_FILE" wl4.srm SAVE_WAS_PRESENT
  record_directory_state "$SYSTEM_DIR" SYSTEM_DIR_WAS_PRESENT
  record_directory_state "$STATES_ROOT_DIR" STATES_ROOT_DIR_WAS_PRESENT
  record_directory_state "$STATE_DIR" STATE_DIR_WAS_PRESENT
  record_directory_state "$SAVE_DIR" SAVE_DIR_WAS_PRESENT
  record_directory_state "$SCREENSHOTS_DIR" SCREENSHOTS_DIR_WAS_PRESENT
  # Every original path is now classified and every present file is verified in
  # the backup. Only now may cleanup interpret false flags as original absence.
  CHECKPOINT_RESTORE_NEEDED=true
  "${ADB[@]}" shell "mkdir -p '$ANDROID_STORAGE_ROOT/catalog'"
  "${ADB[@]}" push "$CHECKPOINT_DEVICE" "$DEVICE_REMOTE" >/dev/null
  "${ADB[@]}" push "$CHECKPOINT_GAMES" "$GAMES_REMOTE" >/dev/null
  "${ADB[@]}" push "$CHECKPOINT_RELEASES" "$RELEASES_REMOTE" >/dev/null
  "${ADB[@]}" exec-out cat "$DEVICE_REMOTE" | cmp -s "$CHECKPOINT_DEVICE" - || {
    echo 'RetroArch acceptance device.yaml checkpoint mismatch' >&2
    exit 1
  }
  "${ADB[@]}" exec-out cat "$GAMES_REMOTE" | cmp -s "$CHECKPOINT_GAMES" - || {
    echo 'RetroArch acceptance catalog/games.yaml checkpoint mismatch' >&2
    exit 1
  }
  "${ADB[@]}" exec-out cat "$RELEASES_REMOTE" | cmp -s "$CHECKPOINT_RELEASES" - || {
    echo 'RetroArch acceptance catalog/releases.yaml checkpoint mismatch' >&2
    exit 1
  }
}

stock_before="$("${ADB[@]}" shell pm path "$STOCK_PACKAGE" 2>/dev/null || true)"
[[ "$stock_before" == package:* ]] || {
  echo 'stock RetroArch must be installed for the coexistence gate' >&2
  exit 1
}
if ! stock_pid="$(package_pid "$STOCK_PACKAGE")"; then
  exit 1
fi
[[ -z "$stock_pid" ]] || {
  echo 'stock RetroArch must be stopped before the coexistence gate' >&2
  exit 1
}
[[ "$("${ADB[@]}" shell pm path "$FORK_PACKAGE" 2>/dev/null || true)" == package:* ]] || {
  echo 'Korri RetroArch is not installed' >&2
  exit 1
}
if ! fork_pid="$(package_pid "$FORK_PACKAGE")"; then
  exit 1
fi
[[ -z "$fork_pid" ]] || {
  echo 'Korri RetroArch must be stopped before acceptance can back up save state' >&2
  exit 1
}
assert_accessibility_service_enabled
existing_korri_pid="$(package_pid "$KORRI_PACKAGE")"
[[ -n "$existing_korri_pid" ]] || {
  echo 'Korri is not already running, so its live brain cannot be safely discovered.' >&2
  echo 'Open Korri normally, leave its Shell visible, and rerun this gate; do not force-stop or reinstall Korri.' >&2
  exit 1
}
assert_pristine_gate_state
PORTAL_EVIDENCE_DIR="$(mktemp -d)"
permission_info="$("${ADB[@]}" shell dumpsys package permissions | \
  grep -A6 'Permission \[com.korri.retroarch.permission.LAUNCH\]' || true)"
grep -q 'sourcePackage=com.korri.retroarch' <<<"$permission_info"
grep -q 'prot=signature' <<<"$permission_info"

# The installed fork is the acceptance target. Do not install, clear, grant, or
# otherwise mutate its package state; build-time APK checks prove bundling and
# the launch below proves the already-installed core is fulfillable.
sdk_level="$("${ADB[@]}" shell getprop ro.build.version.sdk | tr -d '\r')"
if [[ "$sdk_level" -ge 30 ]]; then
  korri_storage_op="$("${ADB[@]}" shell appops get "$KORRI_PACKAGE" MANAGE_EXTERNAL_STORAGE 2>/dev/null | tr -d '\r' || true)"
  grep -q ': allow' <<<"$korri_storage_op" || {
    echo 'Korri all-files access must be granted before RetroArch acceptance' >&2
    exit 1
  }
fi
# The user must present the already-running Shell. Foregrounding it here could
# hide a pre-existing Game/RetroArch activity and would invalidate preflight.
assert_pristine_gate_state

# Atomically read the live port and capability from exactly one trusted main
# portal DevTools target. Neither value is recovered from historical logcat.
authority_json="${KORRI_ANDROID_DEBUG_AUTHORITY_JSON:-}"
if [[ -z "$authority_json" ]]; then
  authority_json="$("$DEBUG_CAPABILITY_SH" "$SERIAL" "$KORRI_PACKAGE" --json)"
fi
jq -e '
  type == "object" and (keys == ["capability", "port"])
  and (.port | type == "number" and floor == . and . >= 1 and . <= 65535)
  and (.capability | type == "string" and test("^[0-9a-f]{64}$"))
' <<<"$authority_json" >/dev/null || {
  echo 'Debug authority helper returned an invalid current authority.' >&2
  exit 1
}
port="$(jq -er '.port' <<<"$authority_json")"
capability="$(jq -er '.capability' <<<"$authority_json")"
discover_live_korri_authority() {
  local health=''
  "${ADB[@]}" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
  "${ADB[@]}" forward "tcp:$HOST_PORT" "tcp:$port" >/dev/null || return 1
  FORWARD_ACTIVE=true
  if ! health="$(curl --fail --silent --show-error \
    --connect-timeout 2 --max-time 5 \
    -H 'content-type: application/json' \
    -H "authorization: Bearer $capability" \
    -d '{"_tag":"system.health","payload":{}}' \
    "http://127.0.0.1:$HOST_PORT/rpc" 2>/dev/null)"; then
    "${ADB[@]}" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
    FORWARD_ACTIVE=false
    return 1
  fi
  jq -e '
    ._tag == "system.health"
    and .outcome._tag == "Ok"
    and (.outcome.payload.version | type == "string" and length > 0)
  ' <<<"$health" >/dev/null 2>&1 || {
    "${ADB[@]}" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
    FORWARD_ACTIVE=false
    return 1
  }
}
discover_live_korri_authority || {
  echo 'No live Korri brain/portal authority could be safely discovered and validated.' >&2
  echo 'Open Korri normally, leave its Shell visible, and rerun; do not force-stop, reinstall, or clear Korri.' >&2
  exit 1
}
ACCEPTANCE_LOG_MARKER="$(new_logcat_marker gate-start)"
rpc() {
  curl --fail --silent --show-error \
    --connect-timeout 2 --max-time 5 --retry 2 --retry-connrefused \
    -H 'content-type: application/json' \
    -H "authorization: Bearer $capability" \
    -d "$1" "http://127.0.0.1:$HOST_PORT/rpc"
}
controls_for_launch() {
  local launch_id="$1"
  rpc "{\"_tag\":\"app.session.controls\",\"payload\":{\"launchId\":\"$launch_id\"}}"
}
invoke_control() {
  local launch_id="$1"
  local control_id="$2"
  rpc "{\"_tag\":\"app.session.control.invoke\",\"payload\":{\"launchId\":\"$launch_id\",\"controlId\":\"$control_id\"}}"
}
wait_old_launch_stale() {
  local launch_id="$1"
  local controls=''
  local invocation=''
  for _ in $(seq 1 20); do
    controls="$(controls_for_launch "$launch_id" 2>/dev/null)" || controls=''
    invocation="$(invoke_control "$launch_id" '@korri:retroarch/quit' 2>/dev/null)" || invocation=''
    if jq -e '.outcome._tag == "Err" and .outcome.payload.reason == "StaleSession"' \
      <<<"$controls" >/dev/null 2>&1 \
      && jq -e '.outcome._tag == "Err" and .outcome.payload.reason == "StaleSession"' \
        <<<"$invocation" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  echo "old launch did not become exactly stale after bounded process teardown: controls=$controls invoke=$invocation" >&2
  return 1
}

# Discovery and every pristine-state assertion precede the first config,
# library, state, save, or preferences mutation.
STAGE="fixture"
discover_live_korri_authority || {
  echo 'The already-running Korri RPC became unavailable before mutation.' >&2
  exit 1
}
assert_pristine_gate_state
assert_session_idle
python3 "$ROOT/services/korrid/prepare-wario-checkpoint.py" \
  "$ROOT/docs/research/retroarch-plugin-route" "$PORTAL_EVIDENCE_DIR/checkpoint"
CHECKPOINT_DEVICE="$PORTAL_EVIDENCE_DIR/checkpoint/device.yaml"
CHECKPOINT_GAMES="$PORTAL_EVIDENCE_DIR/checkpoint/catalog/games.yaml"
CHECKPOINT_RELEASES="$PORTAL_EVIDENCE_DIR/checkpoint/catalog/releases.yaml"
provision_checkpoint_files
"$DEBUG_PORTAL_RELOAD_SH" "$SERIAL" "$KORRI_PACKAGE" \
  --expect-game 01K4J6K8Y00000000000000002 'Wario Land 4' >/dev/null
revalidate_gate_state_after_mutation

local_games="$(rpc '{"_tag":"app.local-games.list","payload":{}}')"
if ! jq -e '
  .outcome._tag == "Ok"
  and (.outcome.payload.failures | not)
  and (.outcome.payload.games | length == 1)
  and .outcome.payload.games[0].id == "01K4J6K8Y00000000000000002"
  and .outcome.payload.games[0].title == "Wario Land 4"
' <<<"$local_games" >/dev/null; then
  echo "RetroArch acceptance did not load the one-item Wario fixture: $local_games" >&2
  exit 1
fi

assert_adb_probe_ready() {
  local state
  state="$(timeout 15 "$ADB_BIN" -s "$SERIAL" get-state | tr -d '\r\n')" || {
    echo 'adb transport is not ready for the UDP negative probe' >&2
    return 1
  }
  [[ "$state" == device ]] || {
    echo "adb transport changed state before the UDP negative probe: $state" >&2
    return 1
  }
}

assert_udp_probe_ready() {
  local tools
  tools="$("${ADB[@]}" shell toybox | tr ', ' '\n')" || return 1
  for tool in dd timeout nc; do
    grep -qx "$tool" <<<"$tools" || {
      echo "required Android toybox tool is unavailable: $tool" >&2
      return 1
    }
  done
}

udp_unauthenticated() {
  local port="$1"
  timeout 15 "$ADB_BIN" -s "$SERIAL" shell \
    "output=\$(toybox dd if=/dev/zero bs=66 count=1 2>/dev/null | toybox timeout 2 nc -4 -u -q 1 127.0.0.1 '$port' 2>&1); rc=\$?; printf '%s remote_nc_rc=%s remote_nc_output=%s\\n' '$UDP_COMPLETION_MARKER' \"\$rc\" \"\$output\"" \
    | tr -d '\r'
}

assert_udp_no_response() {
  local marker="$1"
  local remote_result="$2"
  if [[ "$remote_result" != "$marker "* ]]; then
    echo 'UDP probe transport failed before its remote completion marker' >&2
    return 1
  fi
  case "$remote_result" in
    "$marker remote_nc_rc=0 remote_nc_output=" | "$marker remote_nc_rc=124 remote_nc_output=")
      return 0
      ;;
    *)
      echo "unauthenticated UDP probe must report empty output and remote rc 0 or 124; observed=$remote_result" >&2
      return 1
      ;;
  esac
}

assert_udp_rejection_log() {
  local marker="$1"
  local logs=''
  local malformed_count=0
  local authenticated_count=0
  for _ in $(seq 1 20); do
    logs="$(logcat_since "$marker" -v raw -s \
      KorriAcceptance:I RetroArch:V '*:S' 2>/dev/null)" || return 1
    malformed_count="$(grep -Fxc \
      '[NetCMD] Rejected malformed Korri command.' <<<"$logs" || true)"
    authenticated_count="$(grep -Ec \
      '\[NetCMD\] Korri authenticated (request accepted|reply)' \
      <<<"$logs" || true)"
    if [[ "$malformed_count" -gt 1 || "$authenticated_count" -ne 0 ]]; then
      break
    fi
    if [[ "$malformed_count" -eq 1 ]]; then
      # Let a same-action duplicate or reply log settle before final evidence.
      sleep 0.1
      logs="$(logcat_since "$marker" -v raw -s \
        KorriAcceptance:I RetroArch:V '*:S' 2>/dev/null)" || return 1
      malformed_count="$(grep -Fxc \
        '[NetCMD] Rejected malformed Korri command.' <<<"$logs" || true)"
      authenticated_count="$(grep -Ec \
        '\[NetCMD\] Korri authenticated (request accepted|reply)' \
        <<<"$logs" || true)"
      [[ "$malformed_count" -eq 1 && "$authenticated_count" -eq 0 ]] && return 0
      break
    fi
    sleep 0.1
  done
  echo "unauthenticated UDP probe lacked exact action-bound rejection evidence: malformed=$malformed_count authenticated=$authenticated_count" >&2
  return 1
}
wait_playing() {
  local pid=''
  for _ in $(seq 1 30); do
    if ! pid="$(package_pid "$FORK_PACKAGE")"; then
      return 1
    fi
    [[ -n "$pid" ]] && {
      printf 'RetroArch process %s\n' "$pid"
      return 0
    }
    sleep 1
  done
  echo 'fork process did not become live' >&2
  return 1
}
human_checkpoint() {
  local checkpoint_id="$1"
  local instructions="$2"
  local expected_confirmation="${RUN_NONCE}/${checkpoint_id}"
  local confirmation

  printf 'Human checkpoint %s:\n%s\n' "$checkpoint_id" "$instructions" >&2
  printf 'Only after completing those physical actions, confirm with exactly: %s\n' \
    "$expected_confirmation" >&2
  if ! IFS= read -r confirmation; then
    printf 'Human checkpoint %s failed: stdin reached EOF before confirmation\n' \
      "$checkpoint_id" >&2
    return 1
  fi
  if [[ "$confirmation" != "$expected_confirmation" ]]; then
    printf 'Human checkpoint %s failed: confirmation did not match this run nonce/id\n' \
      "$checkpoint_id" >&2
    return 1
  fi
}
authenticated_retroarch_status() {
  local launch_id="$1"
  local controls
  controls="$(controls_for_launch "$launch_id")"
  jq -ce --arg launch_id "$launch_id" '
    .outcome._tag == "Ok"
    and .outcome.payload.launchId == $launch_id
    and .outcome.payload.retroarchTelemetry.contentBasename == "wl4.gba"
    and (.outcome.payload.retroarchTelemetry.crc32 | type == "string" and length > 0)
    | if . then input_filename else error("invalid authenticated RetroArch telemetry") end
  ' <<<"$controls" >/dev/null || {
    echo "RetroArch authenticated status telemetry was invalid: $controls" >&2
    return 1
  }
  jq -c '.outcome.payload.retroarchTelemetry' <<<"$controls"
}
assert_menu_status() {
  local expected_input="${1-}"
  local expected_json
  case "$expected_input" in
    0) expected_json=false ;;
    1) expected_json=true ;;
    *)
      echo "RetroArch menu status expectation must be exactly 0 or 1: $expected_input" >&2
      return 2
      ;;
  esac
  local status
  status="$(authenticated_retroarch_status "$GATE_CURRENT_LAUNCH")"
  jq -e --argjson expected "$expected_json" '.menuAlive == $expected' <<<"$status" >/dev/null || {
    echo "RetroArch menu telemetry did not report menu=$expected_input: $status" >&2
    return 1
  }
}
assert_selection_advanced() {
  local before="$1"
  local target
  local status
  local menu_alive
  local after="unavailable"
  local attempt

  if [[ ! "$before" =~ ^[0-9]+$ ]]; then
    echo 'RetroArch menu selection baseline is invalid: before=invalid after=unavailable' >&2
    return 1
  fi
  target=$((before + 1))

  for attempt in $(seq 1 20); do
    if ! status="$(authenticated_retroarch_status "$GATE_CURRENT_LAUNCH")"; then
      echo "RetroArch menu selection status was unavailable: before=$before after=unavailable" >&2
      return 1
    fi
    menu_alive="$(jq -r '
      if (.menuAlive | type) != "boolean" then "invalid"
      elif .menuAlive then "true"
      else "false"
      end
    ' <<<"$status" 2>/dev/null || printf invalid)"
    after="$(jq -r '
      if ((.menuSelection | type) == "number"
          and (.menuSelection >= 0)
          and (.menuSelection == (.menuSelection | floor)))
      then (.menuSelection | tostring)
      else "invalid"
      end
    ' <<<"$status" 2>/dev/null || printf invalid)"

    if [[ "$menu_alive" != true ]]; then
      [[ "$menu_alive" == false ]] || menu_alive=invalid
      echo "RetroArch menu closed or invalid while selection advanced: before=$before after=$after menu=$menu_alive" >&2
      return 1
    fi
    if [[ ! "$after" =~ ^[0-9]+$ ]]; then
      echo "RetroArch menu selection telemetry is invalid: before=$before after=invalid" >&2
      return 1
    fi
    if [[ "$after" -eq "$target" ]]; then
      return 0
    fi
    if [[ "$after" -ne "$before" ]]; then
      echo "RetroArch menu selection did not advance exactly once: before=$before after=$after" >&2
      return 1
    fi
    if [[ "$attempt" -lt 20 ]]; then
      sleep 0.1
    fi
  done

  echo "RetroArch menu selection did not advance exactly once before timeout: before=$before after=$after" >&2
  return 1
}
capture_rgui_evidence() {
  local label="$1"
  local image="$PORTAL_EVIDENCE_DIR/$label.png"
  "${ADB[@]}" exec-out screencap -p >"$image"
}
assert_overlay_window_absent() {
  if "${ADB[@]}" shell dumpsys window windows | grep -Fq 'Korri gameplay overlay'; then
    echo 'Korri accessibility overlay remained present after RetroArch acknowledgement' >&2
    return 1
  fi
}
wait_stopped() {
  local pid=""
  for _ in $(seq 1 20); do
    if ! pid="$(package_pid "$FORK_PACKAGE")"; then
      return 1
    fi
    [[ -z "$pid" ]] && return 0
    sleep 0.5
  done
  echo 'fork did not stop after graceful QUIT' >&2
  return 1
}
exact_library_tile_observation() {
  local observation="$1"
  jq -e '
    def bounded_number:
      type == "number" and . > -100000 and . < 100000;
    .view == "home" and .part == "shift.cine-library-tile"
    and .title == "Library" and .focused == true
    and .rectFinitePositive == true and .fullyOnScreen == true
    and (.bounds.left | bounded_number)
    and (.bounds.top | bounded_number)
    and (.bounds.width | bounded_number) and (.bounds.width > 0)
    and (.bounds.height | bounded_number) and (.bounds.height > 0)
    and (.viewport.width | bounded_number) and (.viewport.width > 0)
    and (.viewport.height | bounded_number) and (.viewport.height > 0)
    and .bounds.left >= 0 and .bounds.top >= 0
    and (.bounds.left + .bounds.width) <= .viewport.width
    and (.bounds.top + .bounds.height) <= .viewport.height
  ' <<<"$observation" >/dev/null 2>&1
}
library_tile_bounds_stable() {
  local previous="$1"
  local current="$2"
  jq -en --argjson previous "$previous" --argjson current "$current" '
    def abs: if . < 0 then -. else . end;
    (($previous.bounds.left - $current.bounds.left) | abs) <= 1
    and (($previous.bounds.top - $current.bounds.top) | abs) <= 1
    and (($previous.bounds.width - $current.bounds.width) | abs) <= 1
    and (($previous.bounds.height - $current.bounds.height) | abs) <= 1
  ' >/dev/null 2>&1
}
wait_for_stable_library_tile() {
  local previous="$1"
  local current=''
  local max_attempts=10
  local attempt

  exact_library_tile_observation "$previous" || previous=''
  for ((attempt = 1; attempt <= max_attempts; attempt++)); do
    sleep 0.15
    current=''
    if ! current="$(timeout 1 "$DEBUG_PORTAL_FOCUS_GAME_SH" \
      "$SERIAL" "$KORRI_PACKAGE" --library)" \
      || ! exact_library_tile_observation "$current"; then
      previous=''
      continue
    fi
    if [[ -n "$previous" ]] \
      && library_tile_bounds_stable "$previous" "$current"; then
      printf '%s\n' "$current"
      return 0
    fi
    previous="$current"
  done
  echo 'Library tile bounds did not stabilize within 10 trusted re-observations' >&2
  return 1
}
traverse_library_to_final_viewport() {
  local step
  local max_steps=64
  # The complete federated Library is virtualized and may contain many rows.
  # A bounded real-controller traversal makes the trailing Wario tile render;
  # Down simply sticks after the final row.
  for ((step = 1; step <= max_steps; step++)); do
    "${ADB[@]}" shell input -d 0 keyevent KEYCODE_DPAD_DOWN
    sleep 0.15
  done
}
focus_wario_in_installed_library() {
  local label="$1"
  local navigation_observation
  local library_observation=''
  local library_verified=false
  local focus_observation
  local navigation_json="$PORTAL_EVIDENCE_DIR/$label.library-navigation.json"
  local library_json="$PORTAL_EVIDENCE_DIR/$label.library-view.json"
  local library_diagnostic="$PORTAL_EVIDENCE_DIR/$label.library-view.last-diagnostic.txt"
  local library_failure_image="$PORTAL_EVIDENCE_DIR/$label.library-view.post-tap.png"
  local tap_x tap_y
  # A reload may restore focus anywhere on curated Home. DevTools may focus and
  # measure only the exact visible Library tile; activation remains one bounded
  # pointer event through the installed UI, never a click/RPC/native shortcut.
  navigation_observation="$("$DEBUG_PORTAL_FOCUS_GAME_SH" \
    "$SERIAL" "$KORRI_PACKAGE" --library)"
  exact_library_tile_observation "$navigation_observation"
  # Focus can start a CSS rail transition after the first measurement. Reuse
  # only the same trusted focus/observation helper until two consecutive exact
  # observations differ by at most one CSS pixel on every bounds dimension.
  navigation_observation="$(
    wait_for_stable_library_tile "$navigation_observation"
  )"
  printf '%s\n' "$navigation_observation" >"$navigation_json"
  if ! read -r tap_x tap_y < <(
    verified_library_system_edge_avoiding_point "$navigation_json"
  ); then
    echo 'focused Library bounds did not produce a safe system-edge-avoiding tap coordinate' >&2
    return 1
  fi
  [[ "$tap_x" =~ ^[0-9]+$ && "$tap_y" =~ ^[0-9]+$ ]] || {
    echo 'focused Library system-edge-avoiding point was not an integer coordinate' >&2
    return 1
  }
  "${ADB[@]}" shell input tap "$tap_x" "$tap_y"
  # Physical A activation is retained by the human unified-overlay gate; this
  # automated gate verifies the resulting installed Shift Library explicitly.
  # React may commit the route after the pointer command returns. Poll only the
  # existing read-only exact-view helper: never click again or invoke an RPC.
  for _ in $(seq 1 20); do
    library_observation=''
    if library_observation="$(timeout 1 "$DEBUG_PORTAL_FOCUS_GAME_SH" \
      "$SERIAL" "$KORRI_PACKAGE" --verify-library 2>"$library_diagnostic")" \
      && jq -e '.view == "library" and .verified == true' \
        <<<"$library_observation" >/dev/null 2>&1; then
      library_verified=true
      break
    fi
    sleep 0.25
  done
  printf '%s\n' "$library_observation" >"$library_json"
  if [[ "$library_verified" != true ]]; then
    if ! "${ADB[@]}" exec-out screencap -p >"$library_failure_image"; then
      echo "could not capture post-tap Library failure screenshot at $library_failure_image" >&2
    fi
    echo "installed Library did not become exactly verified; last output is in $library_json, last diagnostic is in $library_diagnostic, and post-tap screenshot is in $library_failure_image" >&2
    [[ ! -s "$library_diagnostic" ]] || cat "$library_diagnostic" >&2
    return 1
  fi
  traverse_library_to_final_viewport
  # Korrid's local RPC id is `01K4J6K8Y00000000000000002`; the folded surface identity is namespaced
  # so it cannot collide with peer or provider entries.
  focus_observation="$("$DEBUG_PORTAL_FOCUS_GAME_SH" \
    "$SERIAL" "$KORRI_PACKAGE" --game 'local-game:01K4J6K8Y00000000000000002' 'Wario Land 4')"
  jq -e '
    .view == "library" and .gameId == "local-game:01K4J6K8Y00000000000000002"
    and .title == "Wario Land 4" and .focused == true
    and .rectFinitePositive == true and .fullyOnScreen == true
    and (.bounds.width > 0) and (.bounds.height > 0)
    and (.viewport.width > 0) and (.viewport.height > 0)
  ' <<<"$focus_observation" >/dev/null
  printf '%s\n' "$focus_observation" >"$PORTAL_EVIDENCE_DIR/$label.focus.json"
}
render_focused_wario_crop_evidence() {
  local image="$1"
  local focus_json="$2"
  local crop="$3"
  local scaled="$4"
  local text="$5"
  local observation="$6"
  local image_width image_height
  local crop_x crop_y crop_width crop_height
  local element_x element_y element_width element_height
  local padding=10
  local element_image="${crop}.focus-element.png"
  local outline_ratio

  jq -e '
    .gameId == "local-game:01K4J6K8Y00000000000000002" and .title == "Wario Land 4"
    and .focused == true and .rectFinitePositive == true and .fullyOnScreen == true
    and (.bounds.left | type == "number") and (.bounds.top | type == "number")
    and (.bounds.width | type == "number") and .bounds.width > 0
    and (.bounds.height | type == "number") and .bounds.height > 0
    and (.viewport.width | type == "number") and .viewport.width > 0
    and (.viewport.height | type == "number") and .viewport.height > 0
  ' "$focus_json" >/dev/null
  read -r image_width image_height < <(
    magick identify -format '%w %h' "$image"
    printf '\n'
  )
  [[ "$image_width" =~ ^[0-9]+$ && "$image_height" =~ ^[0-9]+$ \
    && "$image_width" -gt 0 && "$image_height" -gt 0 ]] || return 1

  read -r crop_x crop_y crop_width crop_height \
    element_x element_y element_width element_height < <(jq -r \
    --argjson imageWidth "$image_width" --argjson imageHeight "$image_height" \
    --argjson padding "$padding" '
      (.bounds.left * $imageWidth / .viewport.width | floor) as $left
      | (.bounds.top * $imageHeight / .viewport.height | floor) as $top
      | ((.bounds.left + .bounds.width) * $imageWidth / .viewport.width | ceil) as $right
      | ((.bounds.top + .bounds.height) * $imageHeight / .viewport.height | ceil) as $bottom
      | ([0, $left - $padding] | max) as $x
      | ([0, $top - $padding] | max) as $y
      | ([$imageWidth, $right + $padding] | min) as $x2
      | ([$imageHeight, $bottom + $padding] | min) as $y2
      | [$x, $y, $x2 - $x, $y2 - $y,
         $left, $top, $right - $left, $bottom - $top] | @tsv
    ' "$focus_json")
  [[ "$crop_x" =~ ^[0-9]+$ && "$crop_y" =~ ^[0-9]+$ \
    && "$crop_width" =~ ^[0-9]+$ && "$crop_height" =~ ^[0-9]+$ \
    && "$element_x" =~ ^[0-9]+$ && "$element_y" =~ ^[0-9]+$ \
    && "$element_width" =~ ^[0-9]+$ && "$element_height" =~ ^[0-9]+$ \
    && "$crop_width" -gt 0 && "$crop_height" -gt 0 \
    && "$element_width" -gt 0 && "$element_height" -gt 0 ]] || return 1

  magick "$image" -crop "${crop_width}x${crop_height}+${crop_x}+${crop_y}" +repage "$crop"
  # Preserve the exact element-box artifact alongside the padded crop. Shift's
  # focus treatment is a CSS outline outside getBoundingClientRect(), so the
  # element box is intentionally not used to find the outline.
  magick "$image" -crop "${element_width}x${element_height}+${element_x}+${element_y}" \
    +repage "$element_image"
  # The measured RG405M padded crop contains about 2.9% cyan outline pixels.
  # A 1% floor tolerates antialiasing and scaling while rejecting the teal tile
  # fill; evaluate in sRGB so the channel thresholds match screenshot bytes.
  outline_ratio="$(magick "$crop" -alpha off -colorspace sRGB \
    -fx 'g > 0.65 && b > 0.75 && r < 0.4 ? 1 : 0' \
    -format '%[fx:mean]' info:)"
  awk -v ratio="$outline_ratio" 'BEGIN { exit !(ratio + 0 >= 0.01) }' || return 1
  magick "$crop" -filter point -resize 400% "$scaled"
  tesseract "$scaled" stdout --psm 6 >"$text" 2>/dev/null
  tr '\n' ' ' <"$text" | grep -Eqi 'wario[[:space:]]+land[[:space:]]+4' || return 1
  jq -cn \
    --argjson x "$crop_x" --argjson y "$crop_y" \
    --argjson width "$crop_width" --argjson height "$crop_height" \
    --argjson elementX "$element_x" --argjson elementY "$element_y" \
    --argjson elementWidth "$element_width" --argjson elementHeight "$element_height" \
    --argjson outlineRatio "$outline_ratio" \
    '{crop:{x:$x,y:$y,width:$width,height:$height},
      focusedElement:{x:$elementX,y:$elementY,width:$elementWidth,height:$elementHeight},
      focusOutlinePaddedCropRatio:$outlineRatio,activeElementVerified:true,
      ocrTitle:"Wario Land 4"}' >"$observation"
}

portal_shot_focuses_wario() {
  local label="$1"
  local image="$PORTAL_EVIDENCE_DIR/$label.png"
  local focus_json="$PORTAL_EVIDENCE_DIR/$label.focus.json"
  local crop="$PORTAL_EVIDENCE_DIR/$label.focus-crop.png"
  local scaled="$PORTAL_EVIDENCE_DIR/$label.focus-crop-4x.png"
  local text="$PORTAL_EVIDENCE_DIR/$label.focus-crop.ocr.txt"
  local observation="$PORTAL_EVIDENCE_DIR/$label.focus-crop.json"
  "${ADB[@]}" exec-out screencap -p >"$image"
  render_focused_wario_crop_evidence \
    "$image" "$focus_json" "$crop" "$scaled" "$text" "$observation"
}
verified_element_center() {
  local focus_json="$1"
  # Identity and role are checked by each strict helper/caller before this
  # common coordinate reducer accepts a center from validated on-screen bounds.
  jq -er '
    def bounded_number:
      type == "number" and . > -100000 and . < 100000;
    select(.focused == true and .rectFinitePositive == true
        and .fullyOnScreen == true)
    | .bounds as $bounds | .viewport as $viewport
    | select($bounds.left | bounded_number)
    | select($bounds.top | bounded_number)
    | select($bounds.width | bounded_number)
    | select($bounds.height | bounded_number)
    | select($viewport.width | bounded_number)
    | select($viewport.height | bounded_number)
    | select($bounds.left >= 0 and $bounds.top >= 0
        and $bounds.width > 0 and $bounds.height > 0
        and $viewport.width > 0 and $viewport.height > 0
        and ($bounds.left + $bounds.width) <= $viewport.width
        and ($bounds.top + $bounds.height) <= $viewport.height)
    | (($bounds.left + ($bounds.width / 2)) | floor) as $x
    | (($bounds.top + ($bounds.height / 2)) | floor) as $y
    | select($x >= 0 and $x < $viewport.width
        and $y >= 0 and $y < $viewport.height)
    | "\($x) \($y)"
  ' "$focus_json"
}
verified_library_system_edge_avoiding_point() {
  local focus_json="$1"
  # The measured RG405M Library tile reaches into the bottom system-edge area.
  # Keep x centered, but place y in the upper quarter of this exact tile so the
  # one installed pointer activation cannot be intercepted by that edge.
  jq -er '
    def bounded_number:
      type == "number" and . > -100000 and . < 100000;
    select(.view == "home" and .part == "shift.cine-library-tile"
        and .title == "Library" and .focused == true
        and .rectFinitePositive == true and .fullyOnScreen == true)
    | .bounds as $bounds | .viewport as $viewport
    | select($bounds.left | bounded_number)
    | select($bounds.top | bounded_number)
    | select($bounds.width | bounded_number)
    | select($bounds.height | bounded_number)
    | select($viewport.width | bounded_number)
    | select($viewport.height | bounded_number)
    | select($bounds.left >= 0 and $bounds.top >= 0
        and $bounds.width > 0 and $bounds.height > 0
        and $viewport.width > 0 and $viewport.height > 0
        and ($bounds.left + $bounds.width) <= $viewport.width
        and ($bounds.top + $bounds.height) <= $viewport.height)
    | (($bounds.left + ($bounds.width / 2)) | floor) as $x
    | (($bounds.top + ($bounds.height / 4)) | floor) as $y
    | (($bounds.top + ($bounds.height / 2)) | floor) as $center_y
    | select($x >= $bounds.left
        and $x < ($bounds.left + $bounds.width)
        and $y >= $bounds.top
        and $y < ($bounds.top + $bounds.height)
        and $x >= 0 and $x < $viewport.width
        and $y >= 0 and $y < $viewport.height
        and $y < $center_y)
    | "\($x) \($y)"
  ' "$focus_json"
}
parse_local_publication() {
  local publication_lines="$1"
  local publication_count
  local launch_id
  publication_count="$(grep -c . <<<"$publication_lines" || true)"
  [[ "$publication_count" -eq 1 ]] || return 1
  launch_id="$(sed -nE \
    's/^launchId=([0-9a-f]{32}) event=published gameId=01K4J6K8Y00000000000000002 package=com\.korri\.retroarch launcher=retroarch$/\1/p' \
    <<<"$publication_lines")"
  [[ "$launch_id" =~ ^[0-9a-f]{32}$ ]] || return 1
  printf '%s' "$launch_id"
}

assert_exact_wario_resume() {
  local response="$1"
  local expected_launch_id="$2"
  jq -e --arg launchId "$expected_launch_id" '
    .outcome._tag == "Ok"
    and .outcome.payload.disposition == "resume"
    and .outcome.payload.launchId == $launchId
    and (.outcome.payload.launchId | test("^[0-9a-f]{32}$"))
    and .outcome.payload.launcherId == "retroarch"
    and .outcome.payload.context.gameId == "01K4J6K8Y00000000000000002"
    and .outcome.payload.context.title == "Wario Land 4"
    and .outcome.payload.context.contentCrc32 == "d6141609"
    and .outcome.payload.context.contributors == [
      {"kind":"launcher","id":"@korri:retroarch/retroarch"},
      {"kind":"runtime","id":"@korri:mgba/mgba"}
    ]
    and .outcome.payload.context.executor == {"id":"retroarch-control","available":true}
    and .outcome.payload.context.foreground == {
      "kind":"component",
      "packageName":"com.korri.retroarch",
      "className":"com.retroarch.browser.retroactivity.RetroActivityFuture"
    }
    and .outcome.payload.component == {
      "packageName":"com.korri.retroarch",
      "className":"com.retroarch.browser.retroactivity.RetroActivityFuture"
    }
    and .outcome.payload.extras.ROM == "/storage/emulated/0/korri/roms/wl4.gba"
    and .outcome.payload.extras.LIBRETRO == "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so"
    and .outcome.payload.extras.CONFIGFILE == "/storage/emulated/0/korri/retroarch.cfg"
    and (.outcome.payload.integrity | type == "string" and length == 64)
  ' <<<"$response" >/dev/null
}

launch_wario_entry() {
  local label="$1"
  local pid=""
  local observed_resume
  local observed_controls=''
  local publication_marker=''
  local publication_log=''
  local publication_lines=''
  local publication_launch=''
  local publication_count=0
  local controls_ready=false
  local tap_x tap_y
  local focus_json="$PORTAL_EVIDENCE_DIR/$label.focus.json"
  local detail_focus_json="$PORTAL_EVIDENCE_DIR/$label.detail-play.focus.json"
  local detail_image="$PORTAL_EVIDENCE_DIR/$label.detail-play.png"
  local location_focus_json="$PORTAL_EVIDENCE_DIR/$label.local-location.focus.json"
  local location_image="$PORTAL_EVIDENCE_DIR/$label.local-location.png"
  local location_failure_image="$PORTAL_EVIDENCE_DIR/$label.local-location.launch-failed.png"
  local location_failure_diagnostic="$PORTAL_EVIDENCE_DIR/$label.local-location.launch-failed.txt"
  local publication_diagnostic="$PORTAL_EVIDENCE_DIR/$label.local-publication.log"
  local controls_diagnostic="$PORTAL_EVIDENCE_DIR/$label.local-controls.json"
  local local_location_id='["local",null,"01K4J6K8Y00000000000000002"]'
  local detail_observation
  local location_observation
  local location_failure_observation
  STAGE="portal-card"
  focus_wario_in_installed_library "$label"
  sleep 1
  portal_shot_focuses_wario "$label" || {
    echo "installed Library did not render focused Wario evidence; evidence is in $PORTAL_EVIDENCE_DIR" >&2
    return 1
  }

  # DevTools only focused and measured the exact tile. Activate that already
  # verified installed-UI element through the normal pointer path: never click
  # through DevTools and never call a launch RPC/native shortcut.
  if ! read -r tap_x tap_y < <(verified_element_center "$focus_json"); then
    echo 'focused Wario bounds did not produce a safe on-screen tap coordinate' >&2
    return 1
  fi
  [[ "$tap_x" =~ ^[0-9]+$ && "$tap_y" =~ ^[0-9]+$ ]] || {
    echo 'focused Wario center was not an integer coordinate' >&2
    return 1
  }
  "${ADB[@]}" shell input tap "$tap_x" "$tap_y"

  STAGE="portal-detail"
  # Library selection opens detail. Prove exact Wario identity and exactly one
  # visible primary Play/Continue action before activation. DevTools focuses
  # and measures only; it never clicks, dispatches input, or calls RPC.
  detail_observation="$("$DEBUG_PORTAL_FOCUS_GAME_SH" \
    "$SERIAL" "$KORRI_PACKAGE" --detail-play)" || {
      echo 'installed portal did not open exact Wario detail with one Play action' >&2
      return 1
    }
  printf '%s\n' "$detail_observation" >"$detail_focus_json"
  "${ADB[@]}" exec-out screencap -p >"$detail_image"
  pid="$(package_pid "$FORK_PACKAGE")" || return 1
  [[ -z "$pid" ]] || {
    echo 'Wario Library card bypassed detail and launched before Play confirmation' >&2
    return 1
  }
  if ! read -r tap_x tap_y < <(verified_element_center "$detail_focus_json"); then
    echo 'focused Wario Play bounds did not produce a safe on-screen tap coordinate' >&2
    return 1
  fi
  [[ "$tap_x" =~ ^[0-9]+$ && "$tap_y" =~ ^[0-9]+$ ]] || {
    echo 'focused Wario Play center was not an integer coordinate' >&2
    return 1
  }

  "${ADB[@]}" shell input tap "$tap_x" "$tap_y"

  STAGE="portal-location-launch"
  # A folded game never silently chooses a host. Prove the exact chooser and
  # select the opaque local copy identity through its verified installed row;
  # labels are presentation only and are never used for routing.
  location_observation="$("$DEBUG_PORTAL_FOCUS_GAME_SH" \
    "$SERIAL" "$KORRI_PACKAGE" --launch-location \
    "$local_location_id" 'Wario Land 4')" || {
      echo 'installed portal did not expose exact Wario local launch location' >&2
      return 1
    }
  printf '%s\n' "$location_observation" >"$location_focus_json"
  "${ADB[@]}" exec-out screencap -p >"$location_image"
  pid="$(package_pid "$FORK_PACKAGE")" || return 1
  [[ -z "$pid" ]] || {
    echo 'Wario detail Play bypassed explicit location selection' >&2
    return 1
  }
  if ! read -r tap_x tap_y < <(verified_element_center "$location_focus_json"); then
    echo 'focused local launch-location bounds did not produce a safe tap coordinate' >&2
    return 1
  fi
  [[ "$tap_x" =~ ^[0-9]+$ && "$tap_y" =~ ^[0-9]+$ ]] || {
    echo 'focused local launch-location center was not an integer coordinate' >&2
    return 1
  }

  TARGET_STARTED_BY_GATE=true
  # Bound publication evidence to this one verified installed-UI activation.
  publication_marker="$(new_logcat_marker "local-publication-$label")"
  "${ADB[@]}" shell input tap "$tap_x" "$tap_y"
  # Physical controller confirm remains mandatory in the unified-overlay human
  # gate; this automated RetroArch gate proves the normal installed pointer UI.
  pid=""
  for _ in $(seq 1 40); do
    pid="$(package_pid "$FORK_PACKAGE")" || return 1
    [[ -n "$pid" ]] && break
    sleep 0.25
  done
  [[ -n "$pid" ]] || {
    "${ADB[@]}" exec-out screencap -p >"$location_failure_image"
    # Diagnose only by re-observing the exact chooser row. This helper may
    # focus and measure the trusted element, but it cannot click, dispatch
    # input, invoke an RPC, or retry the launch.
    if location_failure_observation="$(timeout 5 "$DEBUG_PORTAL_FOCUS_GAME_SH" \
      "$SERIAL" "$KORRI_PACKAGE" --launch-location \
      "$local_location_id" 'Wario Land 4' 2>&1)"; then
      if jq -e --arg locationId "$local_location_id" '
        .view == "launch-location"
        and .dialogLabel == "Choose where to play Wario Land 4"
        and .title == "Wario Land 4"
        and .locationId == $locationId
        and .label == "This device"
        and .focused == true
        and .rectFinitePositive == true
        and .fullyOnScreen == true
      ' <<<"$location_failure_observation" >/dev/null; then
        printf 'exact local launch row remained visible after the one pointer activation\n%s\n' \
          "$location_failure_observation" >"$location_failure_diagnostic"
      else
        printf 'post-activation launch-location observation was not exact\n%s\n' \
          "$location_failure_observation" >"$location_failure_diagnostic"
      fi
    else
      printf 'exact local launch row was unavailable after the one pointer activation\n%s\n' \
        "$location_failure_observation" >"$location_failure_diagnostic"
    fi
    echo "verified Wario local launch location did not start Korri RetroArch; evidence is in $PORTAL_EVIDENCE_DIR" >&2
    return 1
  }
  # This is the first process observed after the verified local-location tap.
  # Record it before any RPC can fail so cleanup can distinguish the gate-owned
  # process from a later replacement even if launch discovery is interrupted.
  record_gate_pid "$pid"

  GATE_CURRENT_LAUNCH=""
  GATE_CURRENT_LAUNCH_QUIESCED=false
  # Discover only the publication caused by the action-bound pointer activation.
  # The dedicated event contains identity, never signed specs or authorities.
  for _ in $(seq 1 40); do
    publication_log="$(logcat_since "$publication_marker" -v raw -s \
      KorriAcceptance:I KorriLocalLifecycle:I '*:S' 2>/dev/null || true)"
    publication_lines="$(sed '/^[[:space:]]*$/d' <<<"$publication_log")"
    publication_count="$(grep -c . <<<"$publication_lines" || true)"
    [[ "$publication_count" -gt 0 ]] && break
    sleep 0.25
  done
  printf '%s\n' "$publication_log" >"$publication_diagnostic"
  [[ "$publication_count" -eq 1 ]] || {
    echo "local launch publication evidence was missing or ambiguous; evidence is in $PORTAL_EVIDENCE_DIR" >&2
    return 1
  }
  if ! GATE_CURRENT_LAUNCH="$(parse_local_publication "$publication_lines")"; then
    echo "local launch publication evidence was malformed; evidence is in $PORTAL_EVIDENCE_DIR" >&2
    return 1
  fi
  record_gate_launch "$GATE_CURRENT_LAUNCH"

  # Publication precedes runtime readiness. Poll the exact launch until korrid
  # authenticates GET_STATUS and materializes both required RetroArch controls.
  for _ in $(seq 1 40); do
    observed_controls="$(controls_for_launch "$GATE_CURRENT_LAUNCH" 2>/dev/null || true)"
    if jq -e --arg launchId "$GATE_CURRENT_LAUNCH" '
      .outcome._tag == "Ok"
      and .outcome.payload.launchId == $launchId
      and .outcome.payload.retroarchTelemetry.contentBasename == "wl4.gba"
      and .outcome.payload.retroarchTelemetry.crc32 == "d6141609"
      and ([.outcome.payload.groups[] | select(.id == "@korri:retroarch")] | length) == 1
      and ([.outcome.payload.groups[]
        | select(.id == "@korri:retroarch")
        | .controls[]
        | {id, destructive}] | sort_by(.id)) == [
          {"id":"@korri:retroarch/open-menu","destructive":false},
          {"id":"@korri:retroarch/quit","destructive":true}
        ]
    ' <<<"$observed_controls" >/dev/null 2>&1; then
      controls_ready=true
      break
    fi
    sleep 0.25
  done
  printf '%s\n' "$observed_controls" >"$controls_diagnostic"
  [[ "$controls_ready" == true ]] || {
    echo "published local launch did not expose authenticated RetroArch controls; evidence is in $PORTAL_EVIDENCE_DIR" >&2
    return 1
  }

  # Re-read the complete action-bounded window after readiness. A delayed
  # duplicate/replacement publication must not pass merely because the first
  # poll observed one valid line before the second line arrived.
  publication_log="$(logcat_since "$publication_marker" -v raw -s \
    KorriAcceptance:I KorriLocalLifecycle:I '*:S' 2>/dev/null || true)"
  publication_lines="$(sed '/^[[:space:]]*$/d' <<<"$publication_log")"
  printf '%s\n' "$publication_log" >"$publication_diagnostic"
  if ! publication_launch="$(parse_local_publication "$publication_lines")" \
      || [[ "$publication_launch" != "$GATE_CURRENT_LAUNCH" ]]; then
    echo "local launch publication evidence changed or became ambiguous; evidence is in $PORTAL_EVIDENCE_DIR" >&2
    return 1
  fi

  # Only after authenticated controls are ready may the conservative repeated
  # launch prove exact same-session resume without rotating authority.
  observed_resume="$(rpc '{"_tag":"app.local-games.launch","payload":{"gameId":"01K4J6K8Y00000000000000002"}}')"
  if ! assert_exact_wario_resume "$observed_resume" "$GATE_CURRENT_LAUNCH"; then
    echo 'running Wario route did not return the exact conservative local resume specification' >&2
    return 1
  fi
  if grep -qE 'KORRI_CONTROL_TOKEN|capability|authorization: Bearer' \
    <<<"$publication_log$observed_resume$observed_controls"; then
    echo 'private launch authority leaked into actual launch evidence' >&2
    return 1
  fi
}

enabled_accessibility_services="$("${ADB[@]}" shell settings get secure enabled_accessibility_services | tr -d '\r')"
if ! grep -q 'com.limelight.korri.overlay.KorriOverlayService' \
    <<<"$enabled_accessibility_services"; then
  echo 'Korri gameplay overlay accessibility service must be enabled by the device owner' >&2
  echo 'acceptance will not modify Android accessibility settings' >&2
  exit 1
fi

launch_wario_entry first
STAGE="wait-playing"
status_first="$(wait_playing)"
if ! pid_first="$(package_pid "$FORK_PACKAGE")"; then
  exit 1
fi
[[ -n "$pid_first" ]] || { echo 'fork process is missing after launch' >&2; exit 1; }
record_gate_pid "$pid_first"
assert_no_artemis_game_activity

STAGE="udp-negative"
# Loopback is transport, not authority. The launch-derived endpoint is visible
# in Korri's generated config, but an exact-size unauthenticated frame receives
# no response and cannot forge a command.
control_port="$("${ADB[@]}" shell "grep '^network_cmd_port = ' '$RETROARCH_CONFIG_REMOTE'" | tr -cd '0-9')"
[[ "$control_port" =~ ^[0-9]+$ && "$control_port" -ge 49152 && "$control_port" -le 65535 ]]
assert_adb_probe_ready
assert_udp_probe_ready
UDP_REJECTION_LOG_MARKER="$(new_logcat_marker udp-negative)"
if ! udp_remote_result="$(udp_unauthenticated "$control_port")"; then
  echo 'UDP probe transport failed before its remote completion marker' >&2
  exit 1
fi
assert_udp_no_response "$UDP_COMPLETION_MARKER" "$udp_remote_result"
assert_udp_rejection_log "$UDP_REJECTION_LOG_MARKER"
if ! current_pid="$(package_pid "$FORK_PACKAGE")"; then
  exit 1
fi
[[ "$current_pid" == "$pid_first" ]]

STAGE="overlay-menu"
# Non-destructive native-menu journey. Authenticated, MAC-covered GET_STATUS
# telemetry is the pass criterion; screenshots are retained only as supporting
# evidence. SHOW_MENU must acknowledge before the portal removes its window.
assert_menu_status 0
human_checkpoint open-retroarch-menu $'On the physical device only:\n  1. Press the physical Guide button.\n  2. Visually verify the actual Shift gameplay sheet is visible before selecting anything.\n  3. Press physical Down exactly once to focus Open RetroArch menu.\n  4. Press physical A exactly once to invoke Open RetroArch menu.'
assert_overlay_window_absent
assert_menu_status 1
menu_selection_before="$(authenticated_retroarch_status "$GATE_CURRENT_LAUNCH" | jq -r '.menuSelection')"
capture_rgui_evidence retroarch-rgui-before-move
human_checkpoint move-retroarch-menu $'On the physical device only:\n  1. Press physical Down exactly once in the native RetroArch menu.\n  2. Visually verify the native RetroArch menu selection moved down exactly one row.'
sleep 0.5
assert_selection_advanced "$menu_selection_before"
capture_rgui_evidence retroarch-rgui-after-move
human_checkpoint resume-retroarch-menu $'On the physical device only:\n  1. Press physical Up exactly once to return selection to Resume.\n  2. Visually verify Resume is selected.\n  3. Press physical A exactly once to activate Resume and return to gameplay.'
sleep 1
assert_menu_status 0
# This synthetic SELECT input is intentionally a non-parity negative probe:
# after the Korri overlay has dismissed, the configured native shortcut must
# not independently reopen RetroArch's menu.
"${ADB[@]}" shell input -d 0 keyevent KEYCODE_BUTTON_SELECT
sleep 0.5
assert_menu_status 0
capture_rgui_evidence retroarch-safe-key
if ! current_pid="$(package_pid "$FORK_PACKAGE")"; then
  exit 1
fi
[[ "$current_pid" == "$pid_first" ]]
human_checkpoint resume-from-overlay $'On the physical device only:\n  1. Press the physical Guide button.\n  2. Visually verify the actual Shift gameplay sheet is visible and Resume is focused before selecting anything.\n  3. Press physical A exactly once on Resume.'
assert_overlay_window_absent
assert_menu_status 0

STAGE="save-pause"
# Android pause must synchronously replace the auto-state before suspension.
before_mtime="$("${ADB[@]}" shell stat -c %Y "$STATE_FILE" 2>/dev/null | tr -d '\r' || printf 0)"
sleep 1
"${ADB[@]}" shell input -d 0 keyevent KEYCODE_HOME
for _ in $(seq 1 20); do
  after_mtime="$("${ADB[@]}" shell stat -c %Y "$STATE_FILE" 2>/dev/null | tr -d '\r' || printf 0)"
  state_size="$("${ADB[@]}" shell stat -c %s "$STATE_FILE" 2>/dev/null | tr -d '\r' || printf 0)"
  [[ "$after_mtime" -gt "$before_mtime" && "$state_size" -gt 0 ]] && break
  sleep 0.25
done
[[ "${after_mtime:-0}" -gt "$before_mtime" && "${state_size:-0}" -gt 0 ]] || {
  echo 'Android pause did not synchronously refresh the auto-state' >&2
  exit 1
}
paused_launch_id="$GATE_CURRENT_LAUNCH"
# Pause persistence is proved above independently of terminal retirement. End
# this exact authority through the authenticated effect before relaunching.
# Abrupt external-emulator death remains deferred to 01KZBYHCA4R9C8QK131HK0VWSA;
# a control timeout alone must never become automatic retirement evidence.
pause_quit="$(invoke_control "$paused_launch_id" '@korri:retroarch/quit')"
if ! jq -e '
  .outcome._tag == "Ok" and .outcome.payload._tag == "Completed"
' <<<"$pause_quit" >/dev/null; then
  echo 'authenticated pause cleanup Quit was not acknowledged as Completed' >&2
  exit 1
fi
# Relaunch is forbidden until acknowledged process teardown and policy-level
# retirement are paired for this exact launch. A publication resets the flag.
wait_stopped
wait_old_launch_stale "$paused_launch_id"
GATE_CURRENT_LAUNCH_QUIESCED=true

STAGE="relaunch"
# Relaunch through Korri again; verbose runtime logging proves the non-empty
# auto-state was loaded successfully rather than merely left on disk.
AUTO_LOAD_LOG_MARKER="$(new_logcat_marker auto-load)"
bring_existing_shell_task_forward
SHELL_BROUGHT_FORWARD=true
"$DEBUG_PORTAL_RELOAD_SH" "$SERIAL" "$KORRI_PACKAGE" \
  --expect-game 01K4J6K8Y00000000000000002 'Wario Land 4' >/dev/null
launch_wario_entry second
STAGE="relaunch"
status_second="$(wait_playing)"
assert_no_artemis_game_activity
"${ADB[@]}" shell "test -s '$STATE_FILE'"
auto_load_log="$(logcat_since "$AUTO_LOAD_LOG_MARKER" 2>/dev/null | \
  grep -F '[State] Auto-loading save state from' | \
  grep -F "$STATE_FILE" | grep 'succeeded' | tail -1 || true)"
[[ -n "$auto_load_log" ]] || {
  echo 'relaunch did not report a successful auto-state load' >&2
  exit 1
}

STAGE="quit-stale"
before_quit_mtime="$("${ADB[@]}" shell stat -c %Y "$STATE_FILE" | tr -d '\r')"
sleep 1
# Resume is row zero, Open RetroArch menu is row one, and destructive Quit is row two.
# Completion requires the explicit native QUIT acknowledgement before the sheet dismisses.
quit_launch_id="$GATE_CURRENT_LAUNCH"
human_checkpoint quit-retroarch $'On the physical device only:\n  1. Press the physical Guide button.\n  2. Visually verify the actual Shift gameplay sheet is visible before selecting anything.\n  3. Press physical Down exactly twice to focus Quit game.\n  4. Press physical A exactly once to invoke Quit game.'
# Local completion is proved by process teardown plus rejection of the exact old
# controls. Remote app.session.status is not local Android launch evidence.
wait_stopped
wait_old_launch_stale "$quit_launch_id"
# Retain host idleness as a replacement-safety precondition only.
assert_session_idle
GATE_CURRENT_LAUNCH_QUIESCED=true
after_quit_mtime="$("${ADB[@]}" shell stat -c %Y "$STATE_FILE" | tr -d '\r')"
[[ "$after_quit_mtime" -gt "$before_quit_mtime" ]] || {
  echo 'graceful QUIT did not refresh the auto-state' >&2
  exit 1
}
display_zero="$("${ADB[@]}" shell dumpsys window displays | \
  sed -n '/Display: mDisplayId=0/,/Display: mDisplayId=[1-9]/p')"
resumed="$(grep 'mCurrentFocus=' <<<"$display_zero" | head -1 || true)"
grep -q "$KORRI_PACKAGE/com.limelight.KorriShellActivity" <<<"$resumed" || {
  echo "Korri did not resume on display 0 after graceful quit: $resumed" >&2
  exit 1
}
if ! stock_pid="$(package_pid "$STOCK_PACKAGE")"; then
  exit 1
fi
[[ -z "$stock_pid" ]] || {
  echo 'acceptance unexpectedly left stock RetroArch running' >&2
  exit 1
}
stock_after="$("${ADB[@]}" shell pm path "$STOCK_PACKAGE" 2>/dev/null || true)"
[[ "$stock_after" == "$stock_before" ]] || {
  echo 'stock RetroArch package path changed during acceptance' >&2
  exit 1
}
acceptance_error_logs="$(logcat_since "$ACCEPTANCE_LOG_MARKER" -s KorriAcceptance:I DEBUG:E AndroidRuntime:E)"
if grep -qE 'Fatal signal|FATAL EXCEPTION' <<<"$acceptance_error_logs"; then
  echo 'runtime emitted a fatal process error during acceptance' >&2
  exit 1
fi
assert_accessibility_service_enabled
assert_korri_process_unchanged
assert_no_artemis_game_activity

STAGE="restoration"
printf 'First launch: %s\n' "$status_first"
printf 'Pause state: %s bytes at %s\n' "$state_size" "$after_mtime"
printf 'Relaunch: %s\n' "$status_second"
printf 'Auto-load: %s\n' "$auto_load_log"
printf 'Quit state refreshed at: %s\n' "$after_quit_mtime"
printf 'Graceful return: %s\n' "$resumed"
printf 'Stock RetroArch preserved: %s\n' "$stock_after"
