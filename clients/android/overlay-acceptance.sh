#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash android-tools coreutils curl diffutils gnugrep gnused gnutar jq python3 websocat
# shellcheck shell=bash
# Human-led installed-device gate for the unified TYPE_ACCESSIBILITY_OVERLAY host.
set -euo pipefail

usage() {
  echo 'usage: overlay-acceptance.sh <adb-serial> <exact-device-model> <exact-hardware-serial> <direct-launch-package> <unrelated-package> [evidence-dir]' >&2
  exit 2
}

[[ $# -ge 5 && $# -le 6 ]] || usage
SERIAL="$1"
EXPECTED_MODEL="$2"
EXPECTED_HARDWARE_SERIAL="$3"
DIRECT_PACKAGE="$4"
UNRELATED_PACKAGE="$5"
EVIDENCE_DIR="${6:-$PWD/overlay-acceptance-evidence-$(date -u +%Y%m%dT%H%M%SZ)}"
[[ -n "$SERIAL" && -n "$EXPECTED_MODEL" && -n "$EXPECTED_HARDWARE_SERIAL" \
  && -n "$DIRECT_PACKAGE" && -n "$UNRELATED_PACKAGE" ]] || usage

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
LOCAL_LAUNCH_PUBLICATION_SH="$ROOT/clients/android/local-launch-publication.sh"
OVERLAY_EVIDENCE_PREDICATES_SH="$ROOT/clients/android/overlay-evidence-predicates.sh"
# shellcheck source=local-launch-publication.sh disable=SC1091
source "$LOCAL_LAUNCH_PUBLICATION_SH"
# shellcheck source=overlay-evidence-predicates.sh disable=SC1091
source "$OVERLAY_EVIDENCE_PREDICATES_SH"
ADB_BIN="${KORRI_ADB_BIN:-$(command -v adb)}"
KORRI_PACKAGE="${KORRI_PACKAGE:-com.simonwjackson.korri.debug}"
KORRI_ACTIVITY="$KORRI_PACKAGE/com.limelight.KorriShellActivity"
KORRI_SERVICE_COMPONENT="$KORRI_PACKAGE/com.limelight.korri.overlay.KorriOverlayService"
RETROARCH_PACKAGE="${KORRI_RETROARCH_PACKAGE:-com.korri.retroarch}"
STOCK_RETROARCH_PACKAGE="${KORRI_STOCK_RETROARCH_PACKAGE:-com.retroarch.aarch64}"
KORRI_GAME_COMPONENT="$KORRI_PACKAGE/com.limelight.Game"
ANDROID_PACKAGE_PATTERN='^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$'
[[ "$SERIAL" =~ ^[A-Za-z0-9._:-]+$ ]] || {
  echo "invalid adb serial: $SERIAL" >&2
  exit 2
}
for package in "$KORRI_PACKAGE" "$RETROARCH_PACKAGE" "$STOCK_RETROARCH_PACKAGE" "$DIRECT_PACKAGE" "$UNRELATED_PACKAGE"; do
  [[ "$package" =~ $ANDROID_PACKAGE_PATTERN ]] || {
    echo "invalid Android package: $package" >&2
    exit 2
  }
done
# The Korri RetroArch build deliberately publishes no launcher activity and
# gates starting on a signature permission, so it cannot be started outside
# Korri at all. The direct-launch negative therefore uses a user-launchable
# emulator, and the fork's non-launchability is asserted separately below.
[[ "$DIRECT_PACKAGE" != "$RETROARCH_PACKAGE" ]] || {
  echo "DIRECT_PACKAGE must not be $RETROARCH_PACKAGE: it cannot be launched outside Korri" >&2
  exit 2
}
[[ "$DIRECT_PACKAGE" != "$KORRI_PACKAGE" ]] || {
  echo 'DIRECT_PACKAGE must not be Korri itself' >&2
  exit 2
}
[[ "$DIRECT_PACKAGE" != "$UNRELATED_PACKAGE" ]] || {
  echo 'direct-launch and unrelated negative packages must be distinct' >&2
  exit 2
}
# Scope is explicit and fail-closed: an unknown value can never silently widen
# or narrow what a recorded run claims. Validation happens before any device
# contact so a typo cannot reach hardware.
ACCEPT_SCOPE="${KORRI_OVERLAY_ACCEPT_SCOPE:-full}"
case "$ACCEPT_SCOPE" in
  full|stream) ;;
  *)
    echo "unknown KORRI_OVERLAY_ACCEPT_SCOPE: $ACCEPT_SCOPE (expected exactly 'full' or 'stream')" >&2
    exit 2
    ;;
esac
# Stages that require a Korri-launched local RetroArch session. In stream scope
# they are skipped because their parity is proven by the RetroArch device gate,
# never because this run observed them.
STREAM_SCOPE_SKIPPED_STAGES=(
  local-overlay-open
  local-mid-overlay-end
  unrelated-active-session-negative
  old-game-still-disarmed
  fresh-publication-rearmed
)
STORAGE_ROOT="/storage/emulated/0/korri"
CONFIG_REMOTE="$STORAGE_ROOT/config.yaml"
LIBRARY_REMOTE="$STORAGE_ROOT/library.yaml"
RETROARCH_CONFIG_REMOTE="$STORAGE_ROOT/retroarch.cfg"
STATE_ROOT="$STORAGE_ROOT/states"
STATE_DIR="$STATE_ROOT/mGBA"
STATE_FILE="$STATE_DIR/wl4.state.auto"
SAVE_DIR="$STORAGE_ROOT/saves"
SAVE_FILE="$SAVE_DIR/wl4.srm"
SYSTEM_DIR="$STORAGE_ROOT/system"
SCREENSHOTS_DIR="$STORAGE_ROOT/screenshots"
CHECKPOINT_CONFIG="$ROOT/docs/research/retroarch-plugin-route/config.yaml"
CHECKPOINT_LIBRARY="$ROOT/docs/research/retroarch-plugin-route/library-wl4.yaml"
LOCK_REMOTE="$STORAGE_ROOT/.android-app-route-check.lock"
LOCK_OWNER_REMOTE="$LOCK_REMOTE/owner"
RUN_NONCE="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
[[ "$RUN_NONCE" =~ ^[0-9a-f]{32}$ ]] || {
  echo 'could not create a random overlay acceptance run nonce' >&2
  exit 1
}
OWNER_MARKER="korri-overlay-acceptance:$RUN_NONCE"
BACKUP_REMOTE="$STORAGE_ROOT/.overlay-acceptance-backup-$RUN_NONCE"
BACKUP_OWNER_REMOTE="$BACKUP_REMOTE/.korri-acceptance-owner"
PREFS_BACKUP="files/.overlay-acceptance-prefs-$RUN_NONCE"
PREFS_BACKUP_OWNER="$PREFS_BACKUP/.korri-acceptance-owner"
HOST_PORT="${KORRI_OVERLAY_ACCEPTANCE_HOST_PORT:-43122}"
DEBUG_CAPABILITY_SH="${KORRI_ANDROID_DEBUG_CAPABILITY_SH:-$ROOT/services/korrid/android-debug-capability.sh}"
DEBUG_PORTAL_RELOAD_SH="${KORRI_ANDROID_DEBUG_PORTAL_RELOAD_SH:-$ROOT/services/korrid/android-debug-reload-portal.sh}"
PREFS_SNAPSHOT_TOOL="$ROOT/clients/android/shared-preferences-snapshot.py"
PREFS_WORK_DIR="${TMPDIR:-/tmp}/korri-overlay-preferences-$RUN_NONCE"
PREFS_WORK_OWNER="$PREFS_WORK_DIR/.korri-acceptance-owner"
PREFS_SEMANTIC_BEFORE="$PREFS_WORK_DIR/before.json"
PREFS_SEMANTIC_AFTER="$PREFS_WORK_DIR/after.json"
PREFS_LOCAL_BEFORE="$PREFS_WORK_DIR/before"
PREFS_LOCAL_AFTER="$PREFS_WORK_DIR/after"
CONNECTION_LOSS_PROBE="${KORRI_STREAM_CONNECTION_LOSS_PROBE:-}"
[[ -n "$CONNECTION_LOSS_PROBE" && -x "$CONNECTION_LOSS_PROBE" ]] || {
  echo 'PENDING: set KORRI_STREAM_CONNECTION_LOSS_PROBE to an executable deterministic connection-loss command/probe' >&2
  exit 2
}

LOCK_ACQUIRED=false
BACKUP_CLASSIFIED=false
BACKUP_CREATED=false
PREFS_BACKUP_CREATED=false
PREFS_WORK_CREATED=false
FORWARD_ACTIVE=false
RPC_READY=false
SHELL_BROUGHT_FORWARD=false
SEMANTIC_COMPARISON_REQUIRED=false
SEMANTIC_VALUES_EQUAL=false
CONFIG_WAS_PRESENT=false
LIBRARY_WAS_PRESENT=false
RETROARCH_CONFIG_WAS_PRESENT=false
STATE_WAS_PRESENT=false
SAVE_WAS_PRESENT=false
STATE_ROOT_WAS_PRESENT=false
STATE_DIR_WAS_PRESENT=false
SAVE_DIR_WAS_PRESENT=false
SYSTEM_DIR_WAS_PRESENT=false
SCREENSHOTS_DIR_WAS_PRESENT=false
PREFS_BACKUP_READY=false
KORRI_PID=''
CAPABILITY=''
ACTIVE_EVIDENCE_CHECKPOINT=''
LOCAL_PUBLICATION_COLLECTOR_PID=''
declare -A GATE_LAUNCH_IDS=()
declare -A GATE_RETROARCH_PIDS=()

adb_target() {
  if ! timeout 15 "$ADB_BIN" "$@"; then
    echo "adb command failed or timed out: $*" >&2
    return 1
  fi
}

adb_capture() {
  timeout 15 "$ADB_BIN" -s "$SERIAL" "$@"
}

adb_shell() {
  adb_capture shell "$@"
}

package_pid() {
  local package="$1"
  local output
  if ! output="$(adb_shell "pidof '$package' || { status=\$?; [ \"\$status\" -eq 1 ] && exit 0; exit \"\$status\"; }")"; then
    echo "could not inspect process state for $package" >&2
    return 1
  fi
  printf '%s' "$output" | tr -d '\r\n'
}

require_preinstalled() {
  local package="$1"
  adb_shell "pm path '$package'" | grep -q '^package:' || {
    echo "required package is not preinstalled: $package" >&2
    exit 1
  }
}

remote_state() {
  local path="$1"
  local state
  state="$(adb_shell "if test -e '$path'; then echo present; else echo absent; fi" | tr -d '\r\n')"
  case "$state" in
    present|absent) printf '%s' "$state" ;;
    *) echo "unexpected state for $path: $state" >&2; return 1 ;;
  esac
}

initialize_host_work_directory() {
  [[ ! -e "$PREFS_WORK_DIR" ]] || {
    echo "refusing pre-existing host work directory: $PREFS_WORK_DIR" >&2
    return 1
  }
  mkdir "$PREFS_WORK_DIR"
  PREFS_WORK_CREATED=true
  printf '%s\n' "$OWNER_MARKER" >"$PREFS_WORK_OWNER"
  [[ "$(<"$PREFS_WORK_OWNER")" == "$OWNER_MARKER" ]]
}

remove_owned_host_work_directory() {
  [[ "$PREFS_WORK_CREATED" == true ]] || return 0
  [[ "$PREFS_WORK_DIR" == "${TMPDIR:-/tmp}/korri-overlay-preferences-$RUN_NONCE" \
    && -f "$PREFS_WORK_OWNER" \
    && "$(<"$PREFS_WORK_OWNER")" == "$OWNER_MARKER" ]] || {
    echo "refusing to remove unverified host work directory: $PREFS_WORK_DIR" >&2
    return 1
  }
  rm -rf -- "$PREFS_WORK_DIR"
  PREFS_WORK_CREATED=false
}

external_backup_is_owned() {
  [[ "$BACKUP_REMOTE" == "$STORAGE_ROOT/.overlay-acceptance-backup-$RUN_NONCE" ]] || return 1
  adb_shell "test \"\$(cat '$BACKUP_OWNER_REMOTE')\" = '$OWNER_MARKER'"
}

preferences_backup_is_owned() {
  local observed_owner
  [[ "$PREFS_BACKUP" == "files/.overlay-acceptance-prefs-$RUN_NONCE" ]] || return 1
  observed_owner="$(adb_capture exec-out run-as "$KORRI_PACKAGE" cat "$PREFS_BACKUP_OWNER" | tr -d '\r\n')" || return 1
  [[ "$observed_owner" == "$OWNER_MARKER" ]]
}

remove_owned_external_backup() {
  [[ "$BACKUP_CREATED" == true ]] || return 0
  external_backup_is_owned || {
    echo "refusing to remove unverified external backup: $BACKUP_REMOTE" >&2
    return 1
  }
  adb_shell "test \"\$(cat '$BACKUP_OWNER_REMOTE')\" = '$OWNER_MARKER' && rm -rf '$BACKUP_REMOTE' && test ! -e '$BACKUP_REMOTE'" >/dev/null
  BACKUP_CREATED=false
}

remove_owned_preferences_backup() {
  [[ "$PREFS_BACKUP_CREATED" == true ]] || return 0
  preferences_backup_is_owned || {
    echo "refusing to remove unverified app-private backup: $PREFS_BACKUP" >&2
    return 1
  }
  adb_shell "run-as '$KORRI_PACKAGE' sh -c \"chmod -R u+w '$PREFS_BACKUP'\"" >/dev/null
  preferences_backup_is_owned || {
    echo "app-private backup ownership changed during cleanup: $PREFS_BACKUP" >&2
    return 1
  }
  adb_shell "run-as '$KORRI_PACKAGE' sh -c \"rm -rf '$PREFS_BACKUP' && test ! -e '$PREFS_BACKUP'\"" >/dev/null
  PREFS_BACKUP_CREATED=false
  PREFS_BACKUP_READY=false
}

backup_file() {
  local path="$1"
  local name="$2"
  local flag="$3"
  if [[ "$(remote_state "$path")" == present ]]; then
    adb_shell "cp '$path' '$BACKUP_REMOTE/$name' && cmp -s '$path' '$BACKUP_REMOTE/$name'"
    printf -v "$flag" true
  fi
}

record_directory() {
  local path="$1"
  local flag="$2"
  if [[ "$(remote_state "$path")" == present ]]; then
    printf -v "$flag" true
  fi
}

snapshot_shared_preferences() {
  local destination_directory="$1"
  local destination_json="$2"
  rm -rf "$destination_directory"
  mkdir -p "$destination_directory"
  adb_capture exec-out run-as "$KORRI_PACKAGE" sh -c \
    'test -d shared_prefs && tar -cf - shared_prefs' \
    | tar -xf - -C "$destination_directory"
  python3 "$PREFS_SNAPSHOT_TOOL" \
    "$destination_directory/shared_prefs" >"$destination_json"
}

require_reversible_materialized_preferences() {
  local snapshot="$1"
  local file="${KORRI_PACKAGE}_preferences.xml"
  local key
  for key in \
    seekbar_sgsr_sharpness \
    seekbar_sgsr_edge_threshold \
    checkbox_flip_face_buttons \
    checkbox_enable_rumble \
    checkbox_enable_pip; do
    jq -e --arg identity "$file:$key" 'has($identity)' "$snapshot" >/dev/null || {
      echo "required materialized SharedPreferences keys are absent: $file:$key" >&2
      echo 'Set and save every reversible Artemis gameplay preference through the product UI before rerunning; acceptance will not materialize defaults.' >&2
      return 1
    }
  done
  if jq -e --arg identity "$file:checkbox_remember_mouse_mode" \
      '.[$identity] == {type:"boolean",value:true}' "$snapshot" >/dev/null; then
    jq -e --arg identity "$file:mouse_mode_list" 'has($identity)' "$snapshot" >/dev/null || {
      echo "required materialized SharedPreferences keys are absent: $file:mouse_mode_list" >&2
      return 1
    }
  fi
  if jq -e --arg identity "$file:checkbox_remember_zoom_pan" \
      '.[$identity] == {type:"boolean",value:true}' "$snapshot" >/dev/null; then
    for key in number_zoom_scale number_pan_offset_x number_pan_offset_y; do
      jq -e --arg identity "$file:$key" 'has($identity)' "$snapshot" >/dev/null || {
        echo "required materialized SharedPreferences keys are absent: $file:$key" >&2
        return 1
      }
    done
  fi
  for key in CrashCount LastNotifiedCrashCount; do
    jq -e --arg identity "DecoderTombstone.xml:$key" \
      '(.[$identity] // {type:"int",value:0}) == {type:"int",value:0}' \
      "$snapshot" >/dev/null || {
        echo "nonzero $key cannot be reversibly restored while Korri remains running" >&2
        return 1
      }
  done
}

record_gate_launch() {
  local launch_id="$1"
  [[ -n "$launch_id" ]] || return 1
  GATE_LAUNCH_IDS["$launch_id"]=1
}

is_gate_launch() {
  [[ -n "${GATE_LAUNCH_IDS[$1]:-}" ]]
}

record_gate_retroarch_pid() {
  local pid="$1"
  [[ -n "$pid" ]] || return 1
  GATE_RETROARCH_PIDS["$pid"]=1
}

restore_file() {
  local path="$1"
  local name="$2"
  local present="$3"
  if [[ "$present" == true ]]; then
    adb_shell "mkdir -p '$(dirname "$path")'; cp '$BACKUP_REMOTE/$name' '$path'; cmp -s '$BACKUP_REMOTE/$name' '$path'" >/dev/null
  else
    adb_shell "rm -f '$path'; test ! -e '$path'" >/dev/null
  fi
}

acquire_lock() {
  adb_shell "mkdir -p '$STORAGE_ROOT' && if mkdir '$LOCK_REMOTE' 2>/dev/null; then printf '%s\n' '$OWNER_MARKER' > '$LOCK_OWNER_REMOTE' && test \"\$(cat '$LOCK_OWNER_REMOTE')\" = '$OWNER_MARKER'; else echo 'Overlay acceptance lock is held. Remove it only after proving no device gate is active.' >&2; exit 75; fi"
  LOCK_ACQUIRED=true
}

release_lock() {
  [[ "$LOCK_ACQUIRED" == true ]] || return 0
  adb_shell "test \"\$(cat '$LOCK_OWNER_REMOTE')\" = '$OWNER_MARKER' && rm -rf '$LOCK_REMOTE' && test ! -e '$LOCK_REMOTE'" >/dev/null || {
    echo "refusing to remove unverified acceptance lock: $LOCK_REMOTE" >&2
    return 1
  }
  LOCK_ACQUIRED=false
}

backup_before_mutation() {
  local app_private_backup_state
  [[ "$(remote_state "$STORAGE_ROOT")" == present ]] || {
    echo "Korri storage root must already exist before acceptance: $STORAGE_ROOT" >&2
    exit 1
  }
  acquire_lock
  [[ "$(remote_state "$BACKUP_REMOTE")" == absent ]] || {
    echo "refusing pre-existing external backup directory: $BACKUP_REMOTE" >&2
    return 1
  }
  adb_shell "mkdir '$BACKUP_REMOTE'"
  BACKUP_CREATED=true
  adb_shell "printf '%s\n' '$OWNER_MARKER' > '$BACKUP_OWNER_REMOTE' && test \"\$(cat '$BACKUP_OWNER_REMOTE')\" = '$OWNER_MARKER'"
  external_backup_is_owned
  backup_file "$CONFIG_REMOTE" config.yaml CONFIG_WAS_PRESENT
  backup_file "$LIBRARY_REMOTE" library.yaml LIBRARY_WAS_PRESENT
  backup_file "$RETROARCH_CONFIG_REMOTE" retroarch.cfg RETROARCH_CONFIG_WAS_PRESENT
  backup_file "$STATE_FILE" wl4.state.auto STATE_WAS_PRESENT
  backup_file "$SAVE_FILE" wl4.srm SAVE_WAS_PRESENT
  record_directory "$STATE_ROOT" STATE_ROOT_WAS_PRESENT
  record_directory "$STATE_DIR" STATE_DIR_WAS_PRESENT
  record_directory "$SAVE_DIR" SAVE_DIR_WAS_PRESENT
  record_directory "$SYSTEM_DIR" SYSTEM_DIR_WAS_PRESENT
  record_directory "$SCREENSHOTS_DIR" SCREENSHOTS_DIR_WAS_PRESENT

  snapshot_shared_preferences "$PREFS_LOCAL_BEFORE" "$PREFS_SEMANTIC_BEFORE"
  require_reversible_materialized_preferences "$PREFS_SEMANTIC_BEFORE"

  # SharedPreferences are diagnostic evidence only. The live Korri process owns
  # these files; acceptance never overwrites or removes shared_prefs. Stateful
  # controls are restored through their public overlay actions and compared
  # semantically before the gate can pass.
  app_private_backup_state="$(adb_shell "run-as '$KORRI_PACKAGE' sh -c \"if test -e '$PREFS_BACKUP'; then echo present; else echo absent; fi\"" | tr -d '\r\n')"
  [[ "$app_private_backup_state" == absent ]] || {
    echo "refusing pre-existing app-private backup directory: $PREFS_BACKUP" >&2
    return 1
  }
  adb_shell "run-as '$KORRI_PACKAGE' sh -c \"mkdir '$PREFS_BACKUP'\""
  PREFS_BACKUP_CREATED=true
  adb_shell "run-as '$KORRI_PACKAGE' sh -c \"printf '%s\\n' '$OWNER_MARKER' > '$PREFS_BACKUP_OWNER'\""
  preferences_backup_is_owned
  adb_shell "run-as '$KORRI_PACKAGE' sh -c \"if test -d shared_prefs; then cp -R shared_prefs '$PREFS_BACKUP/'; diff -r shared_prefs '$PREFS_BACKUP/shared_prefs'; fi; chmod -R a-w '$PREFS_BACKUP'\""
  PREFS_BACKUP_READY=true

  # Every mutable path is classified and every present value has a verified
  # backup. Only now may cleanup interpret a false flag as original absence.
  BACKUP_CLASSIFIED=true
}

restore_exact_state() {
  local failed=false
  restore_file "$CONFIG_REMOTE" config.yaml "$CONFIG_WAS_PRESENT" || failed=true
  restore_file "$LIBRARY_REMOTE" library.yaml "$LIBRARY_WAS_PRESENT" || failed=true
  restore_file "$RETROARCH_CONFIG_REMOTE" retroarch.cfg "$RETROARCH_CONFIG_WAS_PRESENT" || failed=true
  restore_file "$STATE_FILE" wl4.state.auto "$STATE_WAS_PRESENT" || failed=true
  restore_file "$SAVE_FILE" wl4.srm "$SAVE_WAS_PRESENT" || failed=true

  local state
  for state in \
    "$STATE_DIR_WAS_PRESENT:$STATE_DIR" \
    "$STATE_ROOT_WAS_PRESENT:$STATE_ROOT" \
    "$SAVE_DIR_WAS_PRESENT:$SAVE_DIR" \
    "$SCREENSHOTS_DIR_WAS_PRESENT:$SCREENSHOTS_DIR" \
    "$SYSTEM_DIR_WAS_PRESENT:$SYSTEM_DIR"; do
    if [[ "${state%%:*}" == false ]]; then
      adb_shell "rmdir '${state#*:}' 2>/dev/null || test ! -e '${state#*:}'" >/dev/null || failed=true
    fi
  done

  [[ "$failed" == false ]] || {
    echo "exact restoration failed; backup and lock retained at $BACKUP_REMOTE and $LOCK_REMOTE" >&2
    return 1
  }
  if [[ "$SEMANTIC_COMPARISON_REQUIRED" == true && "$SEMANTIC_VALUES_EQUAL" != true ]]; then
    echo "semantic control restoration is incomplete; read-only preference backup retained at $PREFS_BACKUP" >&2
    echo "recovery: restore every changed control through the Korri gameplay overlay, using $EVIDENCE_DIR/stream-controls-original.json as the source of truth" >&2
    echo "external backup and lock retained at $BACKUP_REMOTE and $LOCK_REMOTE until semantic values are verified equal" >&2
    return 1
  fi
  snapshot_shared_preferences "$PREFS_LOCAL_AFTER" "$PREFS_SEMANTIC_AFTER" || return 1
  if ! cmp -s "$PREFS_SEMANTIC_BEFORE" "$PREFS_SEMANTIC_AFTER"; then
    echo "complete SharedPreferences semantic map changed; read-only backup retained at $PREFS_BACKUP" >&2
    echo "external backup and lock retained at $BACKUP_REMOTE and $LOCK_REMOTE; Korri preferences were not overwritten" >&2
    return 1
  fi
  [[ "$(package_pid "$KORRI_PACKAGE")" == "$KORRI_PID" ]] || {
    echo 'Korri process changed before restored portal verification; backups retained' >&2
    return 1
  }
  "$DEBUG_PORTAL_RELOAD_SH" "$SERIAL" "$KORRI_PACKAGE" --expect-portal >/dev/null || {
    echo 'restored files did not produce a usable trusted main portal; backups retained' >&2
    return 1
  }
  if [[ "$PREFS_BACKUP_READY" == true ]]; then
    remove_owned_preferences_backup || return 1
  fi
  remove_owned_external_backup || return 1
  BACKUP_CLASSIFIED=false
}

rpc() {
  curl --fail --silent --show-error --connect-timeout 2 --max-time 5 \
    -H 'content-type: application/json' \
    -H "authorization: Bearer $CAPABILITY" \
    -d "$1" "http://127.0.0.1:$HOST_PORT/rpc"
}

discover_live_korri_authority() {
  local authority_json
  local port
  local health

  # Bind the port and capability from one evaluation in exactly one trusted
  # main portal. Historical readiness records are neither required nor consulted.
  authority_json="${KORRI_ANDROID_DEBUG_AUTHORITY_JSON:-}"
  [[ -n "$authority_json" ]] || authority_json="$("$DEBUG_CAPABILITY_SH" "$SERIAL" "$KORRI_PACKAGE" --json)"
  jq -e '
    type == "object" and (keys == ["capability", "port"])
    and (.port | type == "number" and floor == . and . >= 1 and . <= 65535)
    and (.capability | type == "string" and test("^[0-9a-f]{64}$"))
  ' <<<"$authority_json" >/dev/null || return 1
  port="$(jq -er '.port' <<<"$authority_json")"
  CAPABILITY="$(jq -er '.capability' <<<"$authority_json")"
  adb_target -s "$SERIAL" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
  adb_target -s "$SERIAL" forward "tcp:$HOST_PORT" "tcp:$port" >/dev/null || return 1
  FORWARD_ACTIVE=true
  if ! health="$(rpc '{"_tag":"system.health","payload":{}}')"; then
    adb_target -s "$SERIAL" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
    FORWARD_ACTIVE=false
    return 1
  fi
  jq -e '
    ._tag == "system.health"
    and .outcome._tag == "Ok"
    and (.outcome.payload.version | type == "string" and length > 0)
  ' <<<"$health" >/dev/null || {
    adb_target -s "$SERIAL" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
    FORWARD_ACTIVE=false
    return 1
  }
}

session_is_idle() {
  local status
  status="$(rpc '{"_tag":"app.session.status","payload":{}}')" || return 1
  jq -e '.outcome._tag == "Ok" and (.outcome.payload.active | not)' <<<"$status" >/dev/null
}

close_exact_acceptance_paths() {
  local failed=false
  local pid=''
  local session=''
  local launch_id=''
  local controls=''
  local close_response=''
  local current_korri_pid=''
  local replacement_observed=false

  # Active local emulation uses its authenticated Quit control; an active
  # stream uses Disconnect to finish the exact Game Activity without
  # terminating the host game. A stray fork RetroArch process is the only
  # package cleanup target.
  if [[ "$RPC_READY" == true ]]; then
    session="$(rpc '{"_tag":"app.session.status","payload":{}}' 2>/dev/null || true)"
    launch_id="$(jq -r '.outcome.payload.active.launchId // empty' <<<"$session" 2>/dev/null || true)"
    if [[ -n "$launch_id" ]]; then
      if ! is_gate_launch "$launch_id"; then
        echo "current launch $launch_id was not recorded by this gate; refusing cleanup" >&2
        failed=true
        replacement_observed=true
      else
        controls="$(controls_for_launch "$launch_id" 2>/dev/null || true)"
        if jq -e --arg id '@korri:retroarch/quit' \
            '[.outcome.payload.groups[].controls[].id] | index($id) != null' <<<"$controls" >/dev/null 2>&1; then
          close_response="$(invoke_control "$launch_id" '@korri:retroarch/quit' 2>/dev/null || true)"
        elif jq -e --arg id '@korri:moonlight/disconnect' \
            '[.outcome.payload.groups[].controls[].id] | index($id) != null' <<<"$controls" >/dev/null 2>&1; then
          close_response="$(invoke_control "$launch_id" '@korri:moonlight/disconnect' 2>/dev/null || true)"
        else
          failed=true
        fi
        if [[ -z "$close_response" ]] \
          || ! jq -e '.outcome._tag == "Ok" and .outcome.payload._tag == "Completed"' \
            <<<"$close_response" >/dev/null 2>&1; then
          failed=true
        fi
        for _ in $(seq 1 20); do
          session_is_idle && break
          sleep 0.25
        done
        session_is_idle || failed=true
      fi
    fi
  fi
  [[ "$replacement_observed" == false ]] || return 1

  pid="$(package_pid "$RETROARCH_PACKAGE" 2>/dev/null || printf probe-failed)"
  if [[ -n "$pid" ]]; then
    if [[ -z "${GATE_RETROARCH_PIDS[$pid]:-}" ]]; then
      echo "current RetroArch PID $pid was not recorded by this gate; refusing cleanup" >&2
      failed=true
      replacement_observed=true
    else
      adb_shell "am force-stop '$RETROARCH_PACKAGE'" >/dev/null 2>&1 || failed=true
    fi
  fi
  [[ "$replacement_observed" == false ]] || return 1
  pid="$(package_pid "$RETROARCH_PACKAGE" 2>/dev/null || printf probe-failed)"
  [[ -z "$pid" ]] || failed=true

  if [[ "$SHELL_BROUGHT_FORWARD" == true ]]; then
    adb_shell "am start --display 0 --activity-clear-top -n '$KORRI_ACTIVITY'" >/dev/null 2>&1 || failed=true
    for _ in $(seq 1 20); do
      assert_top_component "$KORRI_ACTIVITY" && assert_overlay_window absent && break
      sleep 0.25
    done
    assert_top_component "$KORRI_ACTIVITY" || failed=true
    assert_overlay_window absent || failed=true
    current_korri_pid="$(package_pid "$KORRI_PACKAGE" 2>/dev/null || printf probe-failed)"
    [[ -n "$current_korri_pid" && "$current_korri_pid" == "$KORRI_PID" ]] || failed=true
  fi

  [[ "$failed" == false ]]
}

cleanup() {
  local status=$?
  local safe=true
  trap - EXIT

  if [[ -n "$LOCAL_PUBLICATION_COLLECTOR_PID" ]]; then
    kill "$LOCAL_PUBLICATION_COLLECTOR_PID" >/dev/null 2>&1 || true
    wait "$LOCAL_PUBLICATION_COLLECTOR_PID" >/dev/null 2>&1 || true
    LOCAL_PUBLICATION_COLLECTOR_PID=''
  fi

  if [[ "$BACKUP_CLASSIFIED" == true ]] && ! close_exact_acceptance_paths; then
    safe=false
  fi
  if [[ "$BACKUP_CLASSIFIED" == true ]]; then
    if [[ "$safe" != true ]]; then
      echo "could not establish safe quiescence; backup and lock retained at $BACKUP_REMOTE and $LOCK_REMOTE" >&2
      status=1
    elif ! restore_exact_state; then
      status=1
    elif ! release_lock; then
      echo "device state was restored, but the acceptance lock remains at $LOCK_REMOTE" >&2
      status=1
    fi
  else
    remove_owned_preferences_backup || status=1
    remove_owned_external_backup || status=1
    if [[ "$LOCK_ACQUIRED" == true ]]; then
      release_lock || status=1
    fi
  fi
  if [[ "$FORWARD_ACTIVE" == true ]]; then
    adb_target -s "$SERIAL" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
    FORWARD_ACTIVE=false
  fi
  assert_accessibility_service_enabled || status=1
  remove_owned_host_work_directory || status=1
  exit "$status"
}

checkpoint() {
  local token="$1"
  shift
  printf '\nHUMAN CHECKPOINT — %s\n' "$token"
  printf '%s\n' "$@"
  printf 'Type exactly "%s" after observing every item: ' "$token"
  local answer
  IFS= read -r answer
  [[ "$answer" == "$token" ]] || {
    echo "checkpoint not confirmed: $token" >&2
    exit 1
  }
}

announce_scope() {
  printf '\nOVERLAY ACCEPTANCE SCOPE — %s\n' "$ACCEPT_SCOPE"
  if [[ "$ACCEPT_SCOPE" == full ]]; then
    printf 'Every unified overlay stage runs in this scope.\n'
    return 0
  fi
  printf 'SKIPPED STAGES (not observed, not claimed by this run):\n'
  printf '  %s\n' "${STREAM_SCOPE_SKIPPED_STAGES[@]}"
  printf 'Local RetroArch overlay parity evidence comes from the separate ra-accept gate.\n'
  printf 'This run therefore does not by itself establish full unified overlay coverage.\n'
}

scope_runs_local_stages() {
  [[ "$ACCEPT_SCOPE" == full ]]
}

accessibility_snapshot() {
  printf 'accessibility_enabled=%s\n' "$(adb_shell settings get secure accessibility_enabled | tr -d '\r')"
  printf 'enabled_accessibility_services=%s\n' "$(adb_shell settings get secure enabled_accessibility_services | tr -d '\r')"
}

assert_accessibility_service_enabled() {
  local enabled_services
  enabled_services="$(adb_shell settings get secure enabled_accessibility_services | tr -d '\r')" || return 1
  grep -Fq "$KORRI_SERVICE_COMPONENT" <<<"$enabled_services" || {
    echo 'Korri gameplay overlay accessibility service is no longer enabled.' >&2
    echo 'Re-enable it manually in Android Settings; acceptance never writes secure settings.' >&2
    return 1
  }
}

semantic_control_values() {
  jq -cS '[
    .outcome.payload.groups[].controls[]
    | select(.interaction.kind != "command")
    | {id, kind: .interaction.kind, value: .interaction.payload.value}
  ] | sort_by(.id)'
}

assert_overlay_window() {
  local expected="$1"
  local windows
  windows="$(adb_shell dumpsys window windows)"
  if [[ "$expected" == present ]]; then
    grep -Fq 'Korri gameplay overlay' <<<"$windows"
  elif grep -Fq 'Korri gameplay overlay' <<<"$windows"; then
    return 1
  fi
}

assert_top_package() {
  local package="$1"
  local observed
  # An empty observation means no resumed activity at all, usually a sleeping
  # display. Say so rather than failing closed without a reason.
  observed="$(adb_shell "dumpsys activity activities 2>/dev/null | grep -m1 -E '(^|[[:space:]])(topResumedActivity|mResumedActivity)[:=]' || true")" || return 1
  [[ -n "$observed" ]] || {
    echo 'no resumed Android activity was observed; the display may be asleep' >&2
    return 1
  }
  grep -F "$package/" <<<"$observed" >/dev/null || {
    echo "expected foreground package $package; observed: $observed" >&2
    return 1
  }
}

assert_top_component() {
  local component="$1"
  local observed
  # An empty observation means no resumed activity at all, usually a sleeping
  # display. Say so rather than failing closed without a reason.
  observed="$(adb_shell "dumpsys activity activities 2>/dev/null | grep -m1 -E '(^|[[:space:]])(topResumedActivity|mResumedActivity)[:=]' || true")" || return 1
  [[ -n "$observed" ]] || {
    echo 'no resumed Android activity was observed; the display may be asleep' >&2
    return 1
  }
  grep -F "$component" <<<"$observed" >/dev/null || {
    echo "expected foreground component $component; observed: $observed" >&2
    return 1
  }
}

assert_no_game_or_retroarch_activities() {
  local activities
  activities="$(adb_shell dumpsys activity activities | tr -d '\r')" || return 1
  if grep -Fq "$KORRI_GAME_COMPONENT" <<<"$activities" \
    || grep -Eq "($RETROARCH_PACKAGE|$STOCK_RETROARCH_PACKAGE)/" <<<"$activities"; then
    echo 'overlay acceptance requires no Artemis Game, Korri RetroArch, or stock RetroArch Activity before mutation' >&2
    return 1
  fi
}

# Measured on the acceptance device: Korri's RetroArch resolves no LAUNCHER
# activity, so no user action can start it directly. This is the strongest form
# of the direct-launch negative and needs no human step.
assert_korri_retroarch_is_not_user_launchable() {
  local resolved
  resolved="$(adb_shell "cmd package resolve-activity --brief -c android.intent.category.LAUNCHER '$RETROARCH_PACKAGE'" | tr -d '\r')" || return 1
  grep -Fq 'No activity found' <<<"$resolved" || {
    echo "$RETROARCH_PACKAGE must publish no launcher activity; resolved: $resolved" >&2
    return 1
  }
}

assert_direct_package_is_user_launchable() {
  local resolved
  resolved="$(adb_shell "cmd package resolve-activity --brief -c android.intent.category.LAUNCHER '$DIRECT_PACKAGE'" | tr -d '\r')" || return 1
  grep -Fq "$DIRECT_PACKAGE/" <<<"$resolved" || {
    echo "$DIRECT_PACKAGE must be launchable from the device UI; resolved: $resolved" >&2
    return 1
  }
}

assert_pristine_gate_state() {
  local fork_pid
  local stock_pid
  local current_korri_pid
  assert_top_component "$KORRI_ACTIVITY"
  assert_no_game_or_retroarch_activities
  fork_pid="$(package_pid "$RETROARCH_PACKAGE")" || return 1
  stock_pid="$(package_pid "$STOCK_RETROARCH_PACKAGE")" || return 1
  current_korri_pid="$(package_pid "$KORRI_PACKAGE")" || return 1
  [[ -z "$fork_pid" && -z "$stock_pid" ]] || {
    echo 'overlay acceptance requires Korri and stock RetroArch processes to be stopped before mutation' >&2
    return 1
  }
  [[ -n "$current_korri_pid" && "$current_korri_pid" == "$KORRI_PID" ]] || {
    echo 'the already-running Korri process changed during overlay acceptance preflight' >&2
    return 1
  }
}

assert_session_idle() {
  session_is_idle || {
    echo 'an app.session launch is active; end it before overlay acceptance mutates device data' >&2
    return 1
  }
}

revalidate_gate_state_after_mutation() {
  assert_pristine_gate_state
  assert_session_idle
}

new_checkpoint_log_marker() {
  local label="$1"
  printf 'korri-overlay-acceptance-%s-%s' "$label" "$RUN_NONCE"
}

start_local_publication_capture() {
  local marker="$1"
  local destination="$2"
  [[ -z "$LOCAL_PUBLICATION_COLLECTOR_PID" ]] || {
    echo 'local publication capture is already active' >&2
    return 1
  }
  : >"$destination"
  (
    best_count=0
    deadline=$((SECONDS + 1800))
    while (( SECONDS < deadline )); do
      current="$(
        timeout 15 "$ADB_BIN" -s "$SERIAL" logcat -d -v raw \
          -s KorriAcceptance:I KorriLocalLifecycle:I '*:S' 2>/dev/null \
          | awk -v marker="$marker" 'index($0, marker) { found = 1; next } found'
      )"
      current="$(sed '/^[[:space:]]*$/d' <<<"$current")"
      current_count="$(grep -c . <<<"$current" || true)"
      if (( current_count > best_count )); then
        printf '%s\n' "$current" >"$destination"
        best_count="$current_count"
      fi
      sleep 0.25
    done
  ) &
  LOCAL_PUBLICATION_COLLECTOR_PID=$!
  # Start bounded polling before publishing its action boundary marker.
  sleep 0.25
  adb_shell "log -t KorriAcceptance '$marker'" >/dev/null
}

stop_local_publication_capture() {
  [[ -n "$LOCAL_PUBLICATION_COLLECTOR_PID" ]] || return 0
  kill "$LOCAL_PUBLICATION_COLLECTOR_PID" >/dev/null 2>&1 || true
  wait "$LOCAL_PUBLICATION_COLLECTOR_PID" >/dev/null 2>&1 || true
  LOCAL_PUBLICATION_COLLECTOR_PID=''
}

begin_evidence_checkpoint() {
  local label="$1"
  [[ "$RPC_READY" == true ]] || {
    echo "cannot create log boundary before live authority discovery: $label" >&2
    return 1
  }
  [[ -z "$ACTIVE_EVIDENCE_CHECKPOINT" ]] || {
    echo "checkpoint logs were not preserved before the next boundary: $ACTIVE_EVIDENCE_CHECKPOINT" >&2
    return 1
  }
  adb_shell logcat -c
  ACTIVE_EVIDENCE_CHECKPOINT="$label"
}

capture_evidence() {
  local label="$1"
  local rpc_responses="$2"
  local expected_launch_id="$3"
  local evidence_predicate="$4"
  local expected_session="$5"
  local image="$EVIDENCE_DIR/$label.png"
  local sidecar="$EVIDENCE_DIR/$label.txt"
  local required_top_activity
  local required_window_records
  local required_accessibility_records
  local required_active_controls
  local required_lifecycle_records
  local exact_checkpoint_predicate=''
  local enabled_services=''

  [[ "$ACTIVE_EVIDENCE_CHECKPOINT" == "$label" ]] || {
    echo "evidence $label has no fresh matching machine boundary" >&2
    exit 1
  }
  required_top_activity="$(adb_shell "dumpsys activity activities 2>/dev/null | grep -m1 -E '(^|[[:space:]])(topResumedActivity|mResumedActivity)[:=]'")"
  [[ -n "$required_top_activity" ]] || {
    echo "required top-activity observation is empty for $label" >&2
    exit 1
  }
  required_window_records="$(adb_shell dumpsys window windows)"
  if [[ -z "$required_window_records" ]] \
    || ! grep -Eq 'mCurrentFocus|mFocusedApp|Window\{' <<<"$required_window_records"; then
    echo "required window records are empty or unstructured for $label" >&2
    exit 1
  fi
  required_accessibility_records="$(adb_shell dumpsys accessibility)"
  [[ -n "$required_accessibility_records" ]] || {
    echo "required accessibility records are empty for $label" >&2
    exit 1
  }
  if [[ "$expected_session" == active ]]; then
    required_active_controls="$(controls_for_launch "$expected_launch_id")"
    jq -e --arg launchId "$expected_launch_id" '
      .outcome._tag == "Ok"
      and .outcome.payload.launchId == $launchId
      and ([.outcome.payload.groups[].controls[]] | length > 0)
    ' <<<"$required_active_controls" >/dev/null || {
      echo "required active controls do not match $expected_launch_id for $label" >&2
      exit 1
    }
  else
    required_active_controls="$(rpc '{"_tag":"app.session.status","payload":{}}')"
    jq -e '.outcome._tag == "Ok" and (.outcome.payload.active | not)' \
      <<<"$required_active_controls" >/dev/null || {
      echo "required idle session observation is invalid for $label" >&2
      exit 1
    }
  fi
  required_lifecycle_records="$(adb_shell "logcat -d -s KorriOverlay:I KorriGameLifecycle:I KorriSessionLifecycle:I")"

  case "$evidence_predicate" in
    positive-overlay)
      assert_overlay_window present
      # Physical Guide is the production global-overlay path. Exact active
      # controls and the visible window bind identity; these two bounded native
      # events prove accepted Guide-up and successful window creation.
      exact_checkpoint_predicate="$(
        korri_positive_overlay_predicate "$required_lifecycle_records" || true
      )"
      ;;
    stale-rpc)
      assert_overlay_window absent
      jq -e '
        .staleControls.outcome._tag == "Err"
        and .staleControls.outcome.payload.reason == "Unavailable"
        and .staleInvocation.outcome._tag == "Err"
        and .staleInvocation.outcome.payload.reason == "Unavailable"
      ' <<<"$rpc_responses" >/dev/null
      exact_checkpoint_predicate="exact stale controls and invocation responses for launchId=$expected_launch_id"
      ;;
    foreground-suspended)
      assert_overlay_window absent
      exact_checkpoint_predicate="$(grep -E \
        "launchId=$expected_launch_id generation=[^[:space:]]+ event=foreground-mismatch reason=suspended" \
        <<<"$required_lifecycle_records" | tail -1 || true)"
      ;;
    suspended-no-show)
      assert_overlay_window absent
      ! grep -Eq "launchId=$expected_launch_id .*event=request-show reason=accepted" \
        <<<"$required_lifecycle_records"
      exact_checkpoint_predicate="no accepted show for suspended launchId=$expected_launch_id"
      ;;
    direct-no-active-launch)
      assert_overlay_window absent
      ! grep -Eq 'event=request-show reason=accepted' <<<"$required_lifecycle_records"
      exact_checkpoint_predicate='no scoped active launch and no accepted direct show request'
      ;;
    connection-loss)
      assert_overlay_window absent
      exact_checkpoint_predicate="$(grep -E \
        "launchId=$expected_launch_id generation=[^[:space:]]+ event=(connection-terminated|stage-failed) reason=(code-[0-9-]+|retryable|terminal)" \
        <<<"$required_lifecycle_records" | tail -1 || true)"
      ;;
    idle-no-window)
      assert_overlay_window absent
      ! grep -Eq 'event=request-show reason=accepted' <<<"$required_lifecycle_records"
      exact_checkpoint_predicate='idle session with no accepted show in this checkpoint'
      ;;
    terminal-rpc)
      assert_overlay_window absent
      jq -e '
        .hostStop.outcome._tag == "Ok"
        and (.hostStop.outcome.payload.phase == "stopped" or .hostStop.outcome.payload.phase == "pending")
        and .finalStatus.outcome._tag == "Ok"
        and (.finalStatus.outcome.payload.active | not)
      ' <<<"$rpc_responses" >/dev/null
      exact_checkpoint_predicate="exact host stop and idle status for launchId=$expected_launch_id"
      ;;
    service-disabled)
      assert_overlay_window absent
      enabled_services="$(adb_shell settings get secure enabled_accessibility_services | tr -d '\r')"
      ! grep -Fq "$KORRI_SERVICE_COMPONENT" <<<"$enabled_services"
      exact_checkpoint_predicate='service-disabled in secure settings, dumpsys captured, and no overlay window'
      ;;
    *)
      echo "unknown evidence predicate for $label: $evidence_predicate" >&2
      exit 1
      ;;
  esac
  [[ -n "$exact_checkpoint_predicate" ]] || {
    echo "required checkpoint predicate $evidence_predicate is absent for $label" >&2
    exit 1
  }

  adb_target -s "$SERIAL" exec-out screencap -p >"$image"
  {
    printf 'label=%s\nserial=%s\nexpected_model=%s\nactual_model=%s\nexpected_hardware_serial=%s\nactual_hardware_serial=%s\ncaptured_utc=%s\nevidence_predicate=%s\nacceptance_scope=%s\n' \
      "$label" "$SERIAL" "$EXPECTED_MODEL" "$ACTUAL_MODEL" \
      "$EXPECTED_HARDWARE_SERIAL" "$ACTUAL_HARDWARE_SERIAL" \
      "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$evidence_predicate" "$ACCEPT_SCOPE"
    if ! scope_runs_local_stages; then
      printf 'skipped_stages=%s\n' "${STREAM_SCOPE_SKIPPED_STAGES[*]}"
      printf 'local_parity_source=ra-accept\n'
    fi
    printf '\n[top activity]\n%s\n' "$required_top_activity"
    printf '\n[pids]\n'
    for package in "$KORRI_PACKAGE" "$RETROARCH_PACKAGE" "$STOCK_RETROARCH_PACKAGE" "$DIRECT_PACKAGE" "$UNRELATED_PACKAGE"; do
      printf '%s=%s\n' "$package" "$(package_pid "$package")"
    done
    printf '\n[accessibility: read only]\n'
    accessibility_snapshot
    printf '\n[dumpsys accessibility]\n%s\n' "$required_accessibility_records"
    printf '\n[window]\n%s\n' "$required_window_records"
    printf '\n[active controls or idle status]\n%s\n' "$required_active_controls"
    printf '\n[rpc responses]\n%s\n' "$rpc_responses"
    printf '\n[checkpoint-bounded lifecycle records]\n%s\n' "$required_lifecycle_records"
    printf '\n[exact checkpoint predicate]\n%s\n' "$exact_checkpoint_predicate"
  } >"$sidecar"
  if [[ ! -s "$image" || ! -s "$sidecar" ]] \
    || ! grep -Fq "$required_top_activity" "$sidecar" \
    || ! grep -Fq "$exact_checkpoint_predicate" "$sidecar"; then
    echo "incomplete screenshot/structured sidecar pair for $label" >&2
    exit 1
  fi
  ACTIVE_EVIDENCE_CHECKPOINT=''
  echo "evidence pair: $image + $sidecar"
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

wait_for_local_session_end() {
  for _ in $(seq 1 30); do
    if [[ -z "$(package_pid "$RETROARCH_PACKAGE")" ]] \
      && session_is_idle \
      && assert_overlay_window absent; then
      return 0
    fi
    sleep 1
  done
  echo 'local session, process, or overlay did not end after exact Quit' >&2
  return 1
}

assert_ended_launch_unavailable() {
  local response="$1"
  local failure="$2"
  jq -e '.outcome._tag == "Err" and .outcome.payload.reason == "Unavailable"' \
    <<<"$response" >/dev/null || {
      echo "$failure" >&2
      return 1
    }
}

announce_scope

if [[ "$SERIAL" == *:* ]]; then
  timeout 15 "$ADB_BIN" connect "$SERIAL" >/dev/null || true
fi
adb_target -s "$SERIAL" wait-for-device
[[ "$(adb_capture get-state | tr -d '\r\n')" == device ]] || {
  echo "Android target is not ready: $SERIAL" >&2
  exit 1
}
TARGET_SERIAL="$(adb_capture get-serialno | tr -d '\r\n')"
[[ "$TARGET_SERIAL" == "$SERIAL" ]] || {
  echo "device serial mismatch: expected '$SERIAL', got '$TARGET_SERIAL'" >&2
  exit 1
}
ACTUAL_MODEL="$(adb_shell getprop ro.product.model | tr -d '\r\n')"
[[ "$ACTUAL_MODEL" == "$EXPECTED_MODEL" ]] || {
  echo "device model mismatch: expected '$EXPECTED_MODEL', got '$ACTUAL_MODEL'" >&2
  exit 1
}
ACTUAL_HARDWARE_SERIAL="$(adb_shell getprop ro.serialno | tr -d '\r\n')"
[[ "$ACTUAL_HARDWARE_SERIAL" == "$EXPECTED_HARDWARE_SERIAL" ]] || {
  echo "hardware serial mismatch: expected '$EXPECTED_HARDWARE_SERIAL', got '$ACTUAL_HARDWARE_SERIAL'" >&2
  exit 1
}
# Cleanup is armed only after the exact target identity has been proven. A
# typo or wrong device can therefore never trigger even a cleanup mutation.
trap cleanup EXIT
for package in "$KORRI_PACKAGE" "$RETROARCH_PACKAGE" "$DIRECT_PACKAGE" "$UNRELATED_PACKAGE"; do
  require_preinstalled "$package"
done
[[ -z "$(package_pid "$RETROARCH_PACKAGE")" ]] || {
  echo "$RETROARCH_PACKAGE must be stopped before acceptance backs up mutable state" >&2
  exit 1
}
assert_korri_retroarch_is_not_user_launchable
assert_direct_package_is_user_launchable
assert_accessibility_service_enabled
KORRI_PID="$(package_pid "$KORRI_PACKAGE")"
[[ -n "$KORRI_PID" ]] || {
  echo 'Korri must already be running before acceptance; open it normally and leave Shell visible.' >&2
  echo 'Do not force-stop, reinstall, clear, or restart Korri after granting accessibility access.' >&2
  exit 1
}
assert_pristine_gate_state
discover_live_korri_authority || {
  echo 'No already-running live Korri RPC could be discovered without mutating device state.' >&2
  exit 1
}
RPC_READY=true
assert_session_idle
mkdir -p "$EVIDENCE_DIR"
[[ -z "$(find "$EVIDENCE_DIR" -mindepth 1 -maxdepth 1 -print -quit)" ]] || {
  echo "evidence directory must be empty: $EVIDENCE_DIR" >&2
  exit 1
}
initialize_host_work_directory

backup_before_mutation
adb_target -s "$SERIAL" push "$CHECKPOINT_CONFIG" "$CONFIG_REMOTE" >/dev/null
adb_target -s "$SERIAL" push "$CHECKPOINT_LIBRARY" "$LIBRARY_REMOTE" >/dev/null
adb_target -s "$SERIAL" exec-out cat "$CONFIG_REMOTE" | cmp -s "$CHECKPOINT_CONFIG" -
adb_target -s "$SERIAL" exec-out cat "$LIBRARY_REMOTE" | cmp -s "$CHECKPOINT_LIBRARY" -
"$DEBUG_PORTAL_RELOAD_SH" "$SERIAL" "$KORRI_PACKAGE" \
  --expect-game wl4 'Wario Land 4' >/dev/null

# Accessibility is Android-owned. This gate only reads it; permission changes
# below are performed by the device owner in Settings.
assert_accessibility_service_enabled
revalidate_gate_state_after_mutation

if scope_runs_local_stages; then
begin_evidence_checkpoint local-overlay-open
local_publication_marker="$(new_checkpoint_log_marker local-overlay-publication)"
local_publication_capture="$PREFS_WORK_DIR/local-overlay-publication.log"
start_local_publication_capture "$local_publication_marker" "$local_publication_capture"
checkpoint 'LOCAL OVERLAY VERIFIED' \
  'Using only the physical controller, launch Wario Land 4 from Korri.' \
  'After gameplay is visible, press physical Guide. Verify one Shift sheet opens.' \
  'Verify D-pad, A/confirm, B/Back, and supported stick/hat navigation work.' \
  'Verify no input reaches gameplay while the sheet is open; leave it open.'
# The stream captures the publication immediately even when verbose emulator
# logging would evict it while the human completes the interaction checks.
for _ in $(seq 1 20); do
  [[ -s "$local_publication_capture" ]] && break
  sleep 0.1
done
sleep 0.25
stop_local_publication_capture

# This is the first process observed after the physical installed-UI action.
# Record it before fallible RPC evidence so cleanup can reject replacements.
local_pid="$(package_pid "$RETROARCH_PACKAGE")" || {
  echo 'local-overlay publication stage could not inspect the RetroArch process' >&2
  exit 1
}
[[ "$local_pid" =~ ^[0-9]+$ ]] || {
  echo 'local-overlay publication stage did not observe exactly one RetroArch process' >&2
  exit 1
}
record_gate_retroarch_pid "$local_pid"

local_publication_lines="$(sed '/^[[:space:]]*$/d' "$local_publication_capture")"
if grep -qE 'KORRI_CONTROL_TOKEN|control(Token|Port)|capability|authorization([: ]|$)' \
    <<<"$local_publication_lines"; then
  echo 'local-overlay publication stage rejected secret-bearing lifecycle evidence' >&2
  exit 1
fi
local_publication_count="$(grep -c . <<<"$local_publication_lines" || true)"
[[ "$local_publication_count" -gt 0 ]] || {
  echo 'local-overlay publication stage found no action-bounded lifecycle publication' >&2
  exit 1
}
[[ "$local_publication_count" -eq 1 ]] || {
  echo 'local-overlay publication stage found multiple action-bounded lifecycle publications' >&2
  exit 1
}
if ! local_launch_id="$(korri_parse_wario_retroarch_publication "$local_publication_lines")"; then
  echo 'local-overlay publication stage found malformed or mismatched lifecycle evidence' >&2
  exit 1
fi
record_gate_launch "$local_launch_id"

local_controls=''
local_controls_ready=false
for _ in $(seq 1 40); do
  local_controls="$(controls_for_launch "$local_launch_id" 2>/dev/null || true)"
  if jq -e --arg launchId "$local_launch_id" '
    .outcome._tag == "Ok"
    and .outcome.payload.launchId == $launchId
    and ([.outcome.payload.groups[].controls[]] | length >= 2)
    and .outcome.payload.retroarchTelemetry
  ' <<<"$local_controls" >/dev/null 2>&1; then
    local_controls_ready=true
    break
  fi
  sleep 0.25
done
[[ "$local_controls_ready" == true ]] || {
  echo 'local-overlay controls stage did not expose authenticated RetroArch telemetry' >&2
  exit 1
}
assert_overlay_window present || {
  echo 'local-overlay window stage did not retain exactly one visible Shift overlay' >&2
  exit 1
}
capture_evidence local-overlay-open "$local_controls" \
  "$local_launch_id" 'positive-overlay' active

begin_evidence_checkpoint local-mid-overlay-end
checkpoint 'RETROARCH MENU VERIFIED' \
  'Using physical D-pad/A, invoke Open RetroArch menu from the Shift sheet.' \
  'Verify Shift dismisses before RGUI appears and navigate RGUI physically.' \
  'Return to gameplay, open Shift again with physical Guide, and leave Shift visibly open.'
assert_top_package "$RETROARCH_PACKAGE"
assert_overlay_window present
local_quit="$(invoke_control "$local_launch_id" '@korri:retroarch/quit')"
jq -e --arg launchId "$local_launch_id" \
  '.outcome._tag == "Ok" and .outcome.payload._tag == "Completed" and .outcome.payload.payload.launchId == $launchId' \
  <<<"$local_quit" >/dev/null
wait_for_local_session_end
stale_controls="$(controls_for_launch "$local_launch_id")"
assert_ended_launch_unavailable "$stale_controls" \
  'SessionControls after end must be exactly Unavailable'
stale_invocation="$(invoke_control "$local_launch_id" '@korri:retroarch/quit')"
assert_ended_launch_unavailable "$stale_invocation" \
  'Invocation after end must be exactly Unavailable'
wait_for_local_session_end
stale_evidence="$(jq -cn --argjson quit "$local_quit" --argjson controls "$stale_controls" --argjson invocation "$stale_invocation" \
  '{quit:$quit,staleControls:$controls,staleInvocation:$invocation}')"
capture_evidence local-mid-overlay-end "$stale_evidence" \
  "$local_launch_id" 'stale-rpc' idle
checkpoint 'LOCAL MID-OVERLAY END VERIFIED' \
  'The authorized exact-current Quit completed while Shift was visibly open.' \
  'The Shift window disappeared automatically, the exact RetroArch process ended, and the session became idle.' \
  'Old-launch controls and invocation were rejected without relaunching or changing foreground state.'

checkpoint 'ACTIVE KORRI LOCAL SESSION VERIFIED' \
  'From Korri, launch Wario Land 4 again and wait for active gameplay.' \
  'Press physical Guide, verify Shift opens and owns input, then dismiss it with B/Back.'
negative_launch="$(rpc '{"_tag":"app.local-games.launch","payload":{"gameId":"wl4"}}')"
negative_launch_id="$(jq -er '.outcome.payload | select(.disposition == "resume") | .launchId' <<<"$negative_launch")"
record_gate_launch "$negative_launch_id"
negative_controls="$(controls_for_launch "$negative_launch_id")"
jq -e '.outcome._tag == "Ok" and ([.outcome.payload.groups[].controls[]] | length >= 2)' <<<"$negative_controls" >/dev/null
negative_pid="$(package_pid "$RETROARCH_PACKAGE")"
record_gate_retroarch_pid "$negative_pid"
[[ -n "$negative_pid" ]] || { echo 'active Korri local session has no RetroArch process' >&2; exit 1; }
assert_top_package "$RETROARCH_PACKAGE"
assert_overlay_window absent

begin_evidence_checkpoint unrelated-active-session-negative
checkpoint 'UNRELATED ACTIVE-SESSION NEGATIVE VERIFIED' \
  "Foreground the already-approved unrelated app $UNRELATED_PACKAGE without ending Wario." \
  'Press physical Guide once and verify the unrelated app stays foreground.' \
  'Verify no Shift window opens and Guide is not consumed by Korri.'
assert_top_package "$UNRELATED_PACKAGE"
assert_overlay_window absent
[[ "$(package_pid "$RETROARCH_PACKAGE")" == "$negative_pid" ]] || {
  echo 'the exact old local game process did not survive the unrelated foreground check' >&2
  exit 1
}
capture_evidence unrelated-active-session-negative "$negative_controls" \
  "$negative_launch_id" 'foreground-suspended' active

begin_evidence_checkpoint old-game-still-disarmed
checkpoint 'OLD GAME REMAINS DISARMED VERIFIED' \
  'Return directly to the already-running Wario Activity using Android recents; do not pass through Korri.' \
  'Press physical Guide and verify no Shift window opens.'
assert_top_package "$RETROARCH_PACKAGE"
assert_overlay_window absent
[[ "$(package_pid "$RETROARCH_PACKAGE")" == "$negative_pid" ]] || {
  echo 'foreground did not return to the exact old local game process' >&2
  exit 1
}
capture_evidence old-game-still-disarmed "$negative_controls" \
  "$negative_launch_id" 'suspended-no-show' active

begin_evidence_checkpoint fresh-publication-rearmed
checkpoint 'FRESH KORRI PUBLICATION REARMS VERIFIED' \
  'Return to Korri and select Wario Land 4 so Korri freshly resumes and publishes the existing local session.' \
  'After gameplay returns, press physical Guide and verify exactly one Shift window opens; leave it open.'
rearmed_launch="$(rpc '{"_tag":"app.local-games.launch","payload":{"gameId":"wl4"}}')"
rearmed_launch_id="$(jq -er '.outcome.payload | select(.disposition == "resume") | .launchId' <<<"$rearmed_launch")"
[[ "$rearmed_launch_id" == "$negative_launch_id" ]] || {
  echo 'fresh publication did not resume the exact old launch' >&2
  exit 1
}
rearmed_controls="$(controls_for_launch "$rearmed_launch_id")"
jq -e '.outcome._tag == "Ok" and ([.outcome.payload.groups[].controls[]] | length >= 2)' <<<"$rearmed_controls" >/dev/null
assert_top_package "$RETROARCH_PACKAGE"
assert_overlay_window present
capture_evidence fresh-publication-rearmed "$rearmed_controls" \
  "$rearmed_launch_id" 'positive-overlay' active
rearmed_quit="$(invoke_control "$rearmed_launch_id" '@korri:retroarch/quit')"
jq -e '.outcome._tag == "Ok" and .outcome.payload._tag == "Completed"' <<<"$rearmed_quit" >/dev/null
wait_for_local_session_end
fi

begin_evidence_checkpoint direct-launch-negative
checkpoint 'DIRECT NEGATIVE VERIFIED' \
  "Launch exactly the package $DIRECT_PACKAGE directly, outside Korri, using the device UI." \
  "Korri's own RetroArch build cannot be started this way; that is asserted automatically." \
  'Press physical Guide and verify Korri does not consume it and no Shift sheet appears.'
assert_top_package "$DIRECT_PACKAGE"
assert_overlay_window absent
direct_status="$(rpc '{"_tag":"app.session.status","payload":{}}')"
jq -e '.outcome._tag == "Ok" and (.outcome.payload.active | not)' <<<"$direct_status" >/dev/null
capture_evidence direct-launch-negative "$direct_status" \
  none 'direct-no-active-launch' idle
checkpoint 'DIRECT NEGATIVE CLOSED VERIFIED' \
  'Exit the directly launched RetroArch instance through its own UI and return to Korri.'
[[ -z "$(package_pid "$DIRECT_PACKAGE")" ]] || {
  echo 'directly launched RetroArch is still running before stream acceptance' >&2
  exit 1
}
assert_top_component "$KORRI_ACTIVITY"

begin_evidence_checkpoint stream-overlay-open
checkpoint 'STREAM OVERLAY VERIFIED' \
  'Using Korri and the physical controller, start the configured Moonlight stream.' \
  'Wait for moving host frames, then press Guide and verify one Shift sheet overlays the live stream.' \
  'Verify physical D-pad, A, B/Back and supported stick/hat input; no input may leak to the host.' \
  'Leave the Shift sheet open for evidence capture.'
stream_status="$(rpc '{"_tag":"app.session.status","payload":{}}')"
stream_launch_id="$(jq -er '.outcome.payload.active.launchId' <<<"$stream_status")"
record_gate_launch "$stream_launch_id"
stream_controls="$(controls_for_launch "$stream_launch_id")"
jq -e '.outcome._tag == "Ok" and ([.outcome.payload.groups[].controls[]] | length == 19)' <<<"$stream_controls" >/dev/null
assert_overlay_window present
capture_evidence stream-overlay-open "$stream_controls" \
  "$stream_launch_id" 'positive-overlay' active

begin_evidence_checkpoint stream-connection-loss-narrated
checkpoint 'STREAM CONNECTION LOSS READY' \
  'Dismiss Shift and leave the exact recorded stream active.' \
  'The gate will now run the explicitly configured deterministic connection-loss probe.'
connection_loss_probe_output="$("$CONNECTION_LOSS_PROBE" "$stream_launch_id")"
for _ in $(seq 1 30); do
  connection_loss_status="$(rpc '{"_tag":"app.session.status","payload":{}}')"
  jq -e '.outcome._tag == "Ok" and (.outcome.payload.active | not)' \
    <<<"$connection_loss_status" >/dev/null && break
  sleep 1
done
jq -e '.outcome._tag == "Ok" and (.outcome.payload.active | not)' \
  <<<"$connection_loss_status" >/dev/null || {
    echo 'stream session is not idle after the deterministic connection-loss probe' >&2
    exit 1
  }
assert_top_package "$KORRI_PACKAGE"
assert_overlay_window absent
connection_loss_evidence="$(jq -cn \
  --arg probe "$connection_loss_probe_output" \
  --argjson status "$connection_loss_status" \
  '{probeObservation:$probe,status:$status}')"
capture_evidence stream-connection-loss-narrated "$connection_loss_evidence" \
  "$stream_launch_id" 'connection-loss' idle

begin_evidence_checkpoint stream-graceful-return
checkpoint 'STREAM GRACEFUL RETURN VERIFIED' \
  'Use the visible KorriSessionOverlay return action and verify it returns gracefully to the Korri portal.'
assert_top_component "$KORRI_ACTIVITY"
assert_overlay_window absent
graceful_return_status="$(rpc '{"_tag":"app.session.status","payload":{}}')"
jq -e '.outcome._tag == "Ok" and (.outcome.payload.active | not)' <<<"$graceful_return_status" >/dev/null
capture_evidence stream-graceful-return "$graceful_return_status" \
  "$stream_launch_id" 'idle-no-window' idle

checkpoint 'STREAM PARITY STARTED' \
  'Start the configured Moonlight stream again, wait for moving host frames, and leave the stream active.' \
  'Do not exercise any gameplay-overlay control until the gate records original semantic values.'
parity_status="$(rpc '{"_tag":"app.session.status","payload":{}}')"
parity_launch_id="$(jq -er '.outcome.payload.active.launchId' <<<"$parity_status")"
record_gate_launch "$parity_launch_id"
parity_controls_original="$(controls_for_launch "$parity_launch_id")"
jq -e '.outcome._tag == "Ok" and ([.outcome.payload.groups[].controls[]] | length == 19)' \
  <<<"$parity_controls_original" >/dev/null
semantic_original="$(semantic_control_values <<<"$parity_controls_original")"
jq -e 'length == 8 and all(.value != null)' <<<"$semantic_original" >/dev/null || {
  echo 'could not capture all eight reversible gameplay-control values' >&2
  exit 1
}
printf '%s\n' "$semantic_original" >"$EVIDENCE_DIR/stream-controls-original.json"
SEMANTIC_COMPARISON_REQUIRED=true

checkpoint 'STREAM PARITY VERIFIED' \
  'Exercise every non-terminal control in docs/research/unified-android-game-overlay.md through the physical Korri gameplay overlay.' \
  'Restore every toggle, choice, and range to its recorded original value through that same product action; never edit app files.' \
  'Leave the same stream active after every reversible value visibly matches its original value.'
parity_controls_final="$(controls_for_launch "$parity_launch_id")"
semantic_final="$(semantic_control_values <<<"$parity_controls_final")"
printf '%s\n' "$semantic_final" >"$EVIDENCE_DIR/stream-controls-final.json"
if [[ "$semantic_final" != "$semantic_original" ]]; then
  diff -u "$EVIDENCE_DIR/stream-controls-original.json" \
    "$EVIDENCE_DIR/stream-controls-final.json" >"$EVIDENCE_DIR/stream-controls-recovery.diff" || true
  echo 'reversible gameplay controls were not restored to their semantic original values' >&2
  echo "recovery: restore every changed control through the Korri gameplay overlay; evidence is in $EVIDENCE_DIR" >&2
  exit 1
fi
SEMANTIC_VALUES_EQUAL=true

begin_evidence_checkpoint stream-host-stop
checkpoint 'STREAM TERMINAL PROBE READY' \
  'Leave the exact recorded stream active after restoring every reversible value.' \
  'Stop that exact launch through the secure remote-session route.'
host_stop_response="$(rpc "{\"_tag\":\"app.session.stop\",\"payload\":{\"expectedLaunchId\":\"$parity_launch_id\"}}")"
jq -e '.outcome._tag == "Ok" and (.outcome.payload.phase == "stopped" or .outcome.payload.phase == "pending")' \
  <<<"$host_stop_response" >/dev/null || {
    echo "Zao host stop did not accept the exact launch identity: $host_stop_response" >&2
    exit 1
  }
for _ in $(seq 1 20); do
  host_stop_status="$(rpc '{"_tag":"app.session.status","payload":{}}')"
  jq -e '.outcome._tag == "Ok" and (.outcome.payload.active | not)' \
    <<<"$host_stop_status" >/dev/null && break
  sleep 0.25
done
jq -e '.outcome._tag == "Ok" and (.outcome.payload.active | not)' \
  <<<"$host_stop_status" >/dev/null || {
    echo 'stream did not become idle after the exact secure host stop' >&2
    exit 1
  }
assert_overlay_window absent
terminal_evidence="$(jq -cn \
  --argjson stop "$host_stop_response" \
  --argjson finalStatus "$host_stop_status" \
  '{hostStop:$stop,finalStatus:$finalStatus}')"
capture_evidence stream-host-stop "$terminal_evidence" \
  "$parity_launch_id" 'terminal-rpc' idle
printf '%s\n' 'DECODER/HOST FAILURE: REPOSITORY-ONLY — deterministic repository tests cover these failures; this device run does not claim them as passed.'

begin_evidence_checkpoint permission-disabled
checkpoint 'PERMISSION DISABLED BY HUMAN' \
  'Open Android Settings yourself and disable the Korri accessibility service.' \
  'Return to Korri, verify the loss is visible, launch Wario successfully, and verify Guide does not open Shift.' \
  'Gracefully quit Wario. Do not use adb or an automated grant action.'
enabled_services="$(adb_shell settings get secure enabled_accessibility_services | tr -d '\r')"
if grep -Fq 'com.limelight.korri.overlay.KorriOverlayService' <<<"$enabled_services"; then
  echo 'accessibility service still appears enabled after the disabled checkpoint' >&2
  exit 1
fi
[[ -z "$(package_pid "$RETROARCH_PACKAGE")" ]] || { echo 'RetroArch still running after permission-disabled launch' >&2; exit 1; }
permission_disabled_status="$(rpc '{"_tag":"app.session.status","payload":{}}')"
jq -e '.outcome._tag == "Ok" and (.outcome.payload.active | not)' <<<"$permission_disabled_status" >/dev/null
capture_evidence permission-disabled "$permission_disabled_status" \
  none 'service-disabled' idle

begin_evidence_checkpoint permission-recovered
checkpoint 'PERMISSION RECOVERED BY HUMAN' \
  'Open Android Settings yourself and re-enable the Korri accessibility service.' \
  'If Android exposes restricted-settings UI, record what it actually says; do not infer success from opening Settings.' \
  'Return to Korri, launch Wario, verify physical Guide opens Shift without reinstall/restart, and leave Shift open.'
enabled_services="$(adb_shell settings get secure enabled_accessibility_services | tr -d '\r')"
grep -Fq 'com.limelight.korri.overlay.KorriOverlayService' <<<"$enabled_services" || {
  echo 'accessibility service is not enabled after the recovery checkpoint' >&2
  exit 1
}
permission_recovered_status="$(rpc '{"_tag":"app.session.status","payload":{}}')"
permission_recovered_launch_id="$(jq -er '.outcome.payload.active.launchId' <<<"$permission_recovered_status")"
record_gate_launch "$permission_recovered_launch_id"
permission_recovered_controls="$(controls_for_launch "$permission_recovered_launch_id")"
jq -e '.outcome._tag == "Ok" and ([.outcome.payload.groups[].controls[]] | length >= 2)' \
  <<<"$permission_recovered_controls" >/dev/null
permission_recovered_pid="$(package_pid "$RETROARCH_PACKAGE")"
record_gate_retroarch_pid "$permission_recovered_pid"
[[ -n "$permission_recovered_pid" ]] || { echo 'permission recovery launch has no RetroArch process' >&2; exit 1; }
assert_top_package "$RETROARCH_PACKAGE"
assert_overlay_window present
assert_accessibility_service_enabled
permission_recovered_evidence="$(jq -cn \
  --argjson status "$permission_recovered_status" \
  --argjson controls "$permission_recovered_controls" \
  '{status:$status,controls:$controls}')"
capture_evidence permission-recovered "$permission_recovered_evidence" \
  "$permission_recovered_launch_id" 'positive-overlay' active
permission_recovered_quit="$(invoke_control "$permission_recovered_launch_id" '@korri:retroarch/quit')"
jq -e '.outcome._tag == "Ok" and .outcome.payload._tag == "Completed"' \
  <<<"$permission_recovered_quit" >/dev/null
wait_for_local_session_end

{
  printf 'acceptance_scope=%s\n' "$ACCEPT_SCOPE"
  printf 'serial=%s\nexpected_model=%s\nactual_model=%s\n' \
    "$SERIAL" "$EXPECTED_MODEL" "$ACTUAL_MODEL"
  printf 'expected_hardware_serial=%s\nactual_hardware_serial=%s\n' \
    "$EXPECTED_HARDWARE_SERIAL" "$ACTUAL_HARDWARE_SERIAL"
  printf 'completed_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  if scope_runs_local_stages; then
    printf 'skipped_stages=\n'
    printf 'local_parity_source=this-run\n'
  else
    printf 'skipped_stages=%s\n' "${STREAM_SCOPE_SKIPPED_STAGES[*]}"
    printf 'local_parity_source=ra-accept\n'
  fi
} >"$EVIDENCE_DIR/acceptance-scope.txt"

printf '\nPre-cutover overlay acceptance evidence captured at %s\n' "$EVIDENCE_DIR"
printf 'acceptance scope: %s\n' "$ACCEPT_SCOPE"
if ! scope_runs_local_stages; then
  printf 'skipped stages (not claimed by this run): %s\n' "${STREAM_SCOPE_SKIPPED_STAGES[*]}"
  printf 'local RetroArch overlay parity evidence comes from the separate ra-accept gate.\n'
fi
printf 'This run records evidence; it does not authorize cutover or claim parity by itself.\n'
