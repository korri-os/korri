#!/usr/bin/env nix-shell
#! nix-shell -i bash -p diffutils bash android-tools coreutils curl gnugrep gnused imagemagick jq tesseract websocat
# shellcheck shell=bash
# Canonical installed Android application route proof.
#
# This gate is intentionally device-only: it installs the current Korri APK,
# copies the reviewed readable checkpoint into Korri's existing Android storage
# root, proves the protected RPC route/signature, then launches the configured
# Android app route through the portal and verifies Android's real foreground/task
# behavior. It never installs, uninstalls, clears, or otherwise mutates the
# user's installed game package, and it restores any pre-existing fixed config
# files before exiting.
set -euo pipefail

SERIAL="${1:?usage: android-app-route-check.sh <adb-serial>}"
GAME="${KORRI_ANDROID_APP_PACKAGE:-com.playdigious.tmnt}"
KORRI_PACKAGE="${KORRI_ANDROID_PACKAGE:-com.simonwjackson.korri.debug}"
HOST_PORT="${KORRI_ANDROID_APP_ROUTE_HOST_PORT:-43120}"
ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
ANDROID_STORAGE_ROOT="/sdcard/korri"
CHECKPOINT_DEVICE="${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_DEVICE:-$ROOT/docs/research/retroarch-plugin-route/device.yaml}"
CHECKPOINT_GAMES="${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_GAMES:-$ROOT/docs/research/retroarch-plugin-route/catalog/games.yaml}"
CHECKPOINT_RELEASES="${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_RELEASES:-$ROOT/docs/research/retroarch-plugin-route/catalog/releases.yaml}"
if [[ -n "${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_DEVICE:-}${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_GAMES:-}${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_RELEASES:-}" ]]; then
  : "${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_DEVICE:?alternate checkpoint requires device.yaml}"
  : "${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_GAMES:?alternate checkpoint requires catalog/games.yaml}"
  : "${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_RELEASES:?alternate checkpoint requires catalog/releases.yaml}"
fi
for checkpoint in "$CHECKPOINT_DEVICE" "$CHECKPOINT_GAMES" "$CHECKPOINT_RELEASES"; do
  [[ -f "$checkpoint" ]] || { echo "Missing checkpoint file: $checkpoint" >&2; exit 1; }
done
EXPECT_RETROARCH_ROUTE=true
if [[ -n "${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_GAMES:-}" ]]; then
  EXPECT_RETROARCH_ROUTE=false
fi
DEVICE_REMOTE="$ANDROID_STORAGE_ROOT/device.yaml"
GAMES_REMOTE="$ANDROID_STORAGE_ROOT/catalog/games.yaml"
RELEASES_REMOTE="$ANDROID_STORAGE_ROOT/catalog/releases.yaml"
CHECKPOINT_BACKUP_DIR="$ANDROID_STORAGE_ROOT/.android-app-route-check-backup-$$"
LOCK_REMOTE="$ANDROID_STORAGE_ROOT/.android-app-route-check.lock"
LOCK_OWNER_REMOTE="$LOCK_REMOTE/owner"
ANDROID_SMOKE="${KORRI_ANDROID_APP_ROUTE_SMOKE_SH:-$ROOT/services/korrid/android-smoke.sh}"
JOURNEY_RESUME="${KORRI_ANDROID_APP_ROUTE_JOURNEY_SH:-$ROOT/services/korrid/journey-resume.sh}"
DEBUG_CAPABILITY_SH="${KORRI_ANDROID_DEBUG_CAPABILITY_SH:-$ROOT/services/korrid/android-debug-capability.sh}"
ADB_BIN="${KORRI_ADB_BIN:-$(command -v adb)}"
CURL=(curl --connect-timeout 2 --max-time 5 --retry 2 --retry-connrefused)
DEVICE_WAS_PRESENT=false
GAMES_WAS_PRESENT=false
RELEASES_WAS_PRESENT=false
CATALOG_DIR_WAS_PRESENT=false
BACKUP_CREATED=false
CHECKPOINT_RESTORE_NEEDED=false
FORWARD_ACTIVE=false
LOCK_ACQUIRED=false

adb_target() {
  if ! timeout 15 "$ADB_BIN" "$@"; then
    echo "adb command failed or timed out: $*" >&2
    return 1
  fi
}

adb_capture() {
  timeout 15 "$ADB_BIN" -s "$SERIAL" "$@"
}

adb_shell_capture() {
  adb_capture shell "$@"
}

resumed_component_from_line() {
  local line="${1:-$(cat)}"
  sed -nE 's/.*[[:space:]]u[0-9]+[[:space:]]([^[:space:]}]+\/[^[:space:]}]+)\}?[[:space:]].*/\1/p' <<<"$line" | tr -d '\r\n'
}

package_from_component() {
  local component="$1"
  printf '%s' "${component%%/*}"
}

acquire_device_lock() {
  if ! adb_target -s "$SERIAL" shell "mkdir -p '$ANDROID_STORAGE_ROOT' && if mkdir '$LOCK_REMOTE' 2>/dev/null; then printf '%s\n' 'pid=$$ started=$(date -u +%Y-%m-%dT%H:%M:%SZ)' > '$LOCK_OWNER_REMOTE'; else echo 'Android app route check lock is held at $LOCK_REMOTE. If this is stale, remove it manually only after verifying no route check is running.' >&2; exit 75; fi"; then
    echo "Android app route check could not acquire the device config lock at $LOCK_REMOTE" >&2
    exit 1
  fi
  LOCK_ACQUIRED=true
}

release_device_lock() {
  if [[ "$LOCK_ACQUIRED" != true ]]; then
    return 0
  fi
  if ! adb_target -s "$SERIAL" shell "rm -rf '$LOCK_REMOTE'" >/dev/null 2>&1; then
    echo "Android app route check failed to release the device config lock at $LOCK_REMOTE" >&2
    return 1
  fi
  LOCK_ACQUIRED=false
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
    if ! adb_target -s "$SERIAL" shell "cp '$CHECKPOINT_BACKUP_DIR/device.yaml' '$DEVICE_REMOTE' && cmp -s '$CHECKPOINT_BACKUP_DIR/device.yaml' '$DEVICE_REMOTE'" >/dev/null 2>&1; then
      echo "Android app route check failed to restore prior device.yaml" >&2
      restore_failed=true
    fi
  else
    if ! adb_target -s "$SERIAL" shell "rm -f '$DEVICE_REMOTE' && test ! -e '$DEVICE_REMOTE'" >/dev/null 2>&1; then
      echo "Android app route check failed to remove created device.yaml" >&2
      restore_failed=true
    fi
  fi

  if [[ "$GAMES_WAS_PRESENT" == true ]]; then
    if ! adb_target -s "$SERIAL" shell "cp '$CHECKPOINT_BACKUP_DIR/games.yaml' '$GAMES_REMOTE' && cmp -s '$CHECKPOINT_BACKUP_DIR/games.yaml' '$GAMES_REMOTE'" >/dev/null 2>&1; then
      echo "Android app route check failed to restore prior catalog/games.yaml" >&2
      restore_failed=true
    fi
  else
    if ! adb_target -s "$SERIAL" shell "rm -f '$GAMES_REMOTE' && test ! -e '$GAMES_REMOTE'" >/dev/null 2>&1; then
      echo "Android app route check failed to remove created catalog/games.yaml" >&2
      restore_failed=true
    fi
  fi

  if [[ "$RELEASES_WAS_PRESENT" == true ]]; then
    if ! adb_target -s "$SERIAL" shell "cp '$CHECKPOINT_BACKUP_DIR/releases.yaml' '$RELEASES_REMOTE' && cmp -s '$CHECKPOINT_BACKUP_DIR/releases.yaml' '$RELEASES_REMOTE'" >/dev/null 2>&1; then
      echo "Android app route check failed to restore prior catalog/releases.yaml" >&2
      restore_failed=true
    fi
  else
    if ! adb_target -s "$SERIAL" shell "rm -f '$RELEASES_REMOTE' && test ! -e '$RELEASES_REMOTE'" >/dev/null 2>&1; then
      echo "Android app route check failed to remove created catalog/releases.yaml" >&2
      restore_failed=true
    fi
  fi

  if [[ "$CATALOG_DIR_WAS_PRESENT" != true ]]; then
    adb_target -s "$SERIAL" shell "rmdir '$ANDROID_STORAGE_ROOT/catalog' 2>/dev/null || test ! -e '$ANDROID_STORAGE_ROOT/catalog'" >/dev/null 2>&1 || restore_failed=true
  fi
  if [[ "$restore_failed" == true ]]; then
    echo "Checkpoint restore failed; backup and lock retained" >&2
    return 1
  fi

  if ! adb_target -s "$SERIAL" shell "rm -rf '$CHECKPOINT_BACKUP_DIR'" >/dev/null 2>&1; then
    echo "Android app route check failed to remove checkpoint backup directory $CHECKPOINT_BACKUP_DIR" >&2
    restore_failed=true
  fi

  [[ "$restore_failed" == false ]]
}

cleanup() {
  local status=$?
  local cleanup_failed=false

  if [[ "$FORWARD_ACTIVE" == true ]]; then
    adb_target -s "$SERIAL" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
  fi
  if restore_checkpoint_files; then
    release_device_lock || cleanup_failed=true
  else
    cleanup_failed=true
  fi

  if [[ "$cleanup_failed" == true && "$status" -eq 0 ]]; then
    echo "Android app route check cleanup failed after successful run; fixed config may need manual inspection" >&2
    exit 1
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

provision_checkpoint_files() {
  acquire_device_lock
  adb_target -s "$SERIAL" shell "mkdir '$CHECKPOINT_BACKUP_DIR'"
  BACKUP_CREATED=true
  local state
  state="$(remote_state "$ANDROID_STORAGE_ROOT/catalog")" || return 1
  [[ "$state" != present ]] || CATALOG_DIR_WAS_PRESENT=true
  state="$(remote_state "$DEVICE_REMOTE")" || return 1
  if [[ "$state" == present ]]; then
    adb_target -s "$SERIAL" shell "cp '$DEVICE_REMOTE' '$CHECKPOINT_BACKUP_DIR/device.yaml' && cmp -s '$DEVICE_REMOTE' '$CHECKPOINT_BACKUP_DIR/device.yaml'" || return 1
    DEVICE_WAS_PRESENT=true
  fi
  state="$(remote_state "$GAMES_REMOTE")" || return 1
  if [[ "$state" == present ]]; then
    adb_target -s "$SERIAL" shell "cp '$GAMES_REMOTE' '$CHECKPOINT_BACKUP_DIR/games.yaml' && cmp -s '$GAMES_REMOTE' '$CHECKPOINT_BACKUP_DIR/games.yaml'" || return 1
    GAMES_WAS_PRESENT=true
  fi
  state="$(remote_state "$RELEASES_REMOTE")" || return 1
  if [[ "$state" == present ]]; then
    adb_target -s "$SERIAL" shell "cp '$RELEASES_REMOTE' '$CHECKPOINT_BACKUP_DIR/releases.yaml' && cmp -s '$RELEASES_REMOTE' '$CHECKPOINT_BACKUP_DIR/releases.yaml'" || return 1
    RELEASES_WAS_PRESENT=true
  fi
  # Only a complete, verified backup permits mutation or absence-based cleanup.
  CHECKPOINT_RESTORE_NEEDED=true
  adb_target -s "$SERIAL" shell "mkdir -p '$ANDROID_STORAGE_ROOT/catalog'"
  adb_target -s "$SERIAL" push "$CHECKPOINT_DEVICE" "$DEVICE_REMOTE" >/dev/null
  adb_target -s "$SERIAL" push "$CHECKPOINT_GAMES" "$GAMES_REMOTE" >/dev/null
  adb_target -s "$SERIAL" push "$CHECKPOINT_RELEASES" "$RELEASES_REMOTE" >/dev/null
  if ! adb_target -s "$SERIAL" exec-out cat "$DEVICE_REMOTE" | cmp -s "$CHECKPOINT_DEVICE" -; then
    echo "Device device.yaml does not match the reviewed checkpoint bytes" >&2
    exit 1
  fi
  if ! adb_target -s "$SERIAL" exec-out cat "$GAMES_REMOTE" | cmp -s "$CHECKPOINT_GAMES" -; then
    echo "Device catalog/games.yaml does not match the reviewed checkpoint bytes" >&2
    exit 1
  fi
  if ! adb_target -s "$SERIAL" exec-out cat "$RELEASES_REMOTE" | cmp -s "$CHECKPOINT_RELEASES" -; then
    echo "Device catalog/releases.yaml does not match the reviewed checkpoint bytes" >&2
    exit 1
  fi
}

if [[ "$SERIAL" == *:* ]]; then
  timeout 15 "$ADB_BIN" connect "$SERIAL" >/dev/null || true
fi
if ! timeout 15 "$ADB_BIN" -s "$SERIAL" wait-for-device; then
  echo "Android target is not reachable: $SERIAL" >&2
  exit 1
fi
if ! adb_shell_capture "pm path $GAME" | grep -q '^package:'; then
  echo "Required Android package is not installed: $GAME" >&2
  echo "Install it on the target device, then rerun this check. The check will not install, uninstall, clear, or otherwise mutate the game package." >&2
  exit 1
fi

# Only the dedicated installed-route gate provisions the reviewed checkpoint.
# The general android-smoke/korrid-check-device path must leave user
# device.yaml and both catalog files untouched.
provision_checkpoint_files

# The smoke script installs Korri and proves protected RPC list/launch
# signatures for the configured Android app route and, for the canonical
# checkpoint, the plugin-backed WL4 route. Keep this call first so the portal
# journey below drives the same configured app state that RPC just observed.
KORRI_EXPECT_RETROARCH_ROUTE="$EXPECT_RETROARCH_ROUTE" \
  "$ANDROID_SMOKE" --expect-installed-route "$SERIAL"

# Drive the real portal/native bridge path. This uses Home plus relaunching
# Korri as the measured return path; Back is never used as resume evidence.
"$JOURNEY_RESUME" "$SERIAL" "$GAME"

if ! top_activity="$(adb_shell_capture "dumpsys activity activities 2>/dev/null | grep -m1 -E '(^|[[:space:]])(topResumedActivity|mResumedActivity)[:=]'" | tr -d '\r')"; then
  echo "Android app route check could not read the resumed activity from the device" >&2
  exit 1
fi
top_component="$(resumed_component_from_line "$top_activity")"
top_package="$(package_from_component "$top_component")"
if [[ "$top_package" != "$GAME" ]]; then
  echo "Android app route check ended without $GAME top-resumed: $top_activity" >&2
  exit 1
fi
if ! pid="$(adb_shell_capture "pidof $GAME || { status=\$?; [ \"\$status\" -eq 1 ] && exit 0; exit \"\$status\"; }" 2>/dev/null | tr -d '\r\n')"; then
  echo "Android app route check could not read process evidence for $GAME" >&2
  exit 1
fi
if [[ -z "$pid" ]]; then
  echo "Android app route check ended with $GAME top-resumed but no process evidence" >&2
  exit 1
fi

port=""
for _ in $(seq 1 10); do
  logcat_output=""
  if logcat_output="$(adb_capture logcat -d -s KorridServer:I 2>/dev/null)"; then
    line="$(grep 'listening on 127.0.0.1:' <<<"$logcat_output" | tail -1 || true)"
    port="$(printf '%s' "$line" | sed -n 's/.*127\.0\.0\.1:\([0-9][0-9]*\).*/\1/p')"
  fi
  [[ -n "$port" ]] && break
  sleep 1
done
if [[ -z "$port" ]]; then
  echo "Could not recover embedded brain readiness after portal journey" >&2
  exit 1
fi
capability="${KORRI_ANDROID_DEBUG_CAPABILITY:-}"
if [[ -z "$capability" ]]; then
  capability="$("$DEBUG_CAPABILITY_SH" "$SERIAL" "$KORRI_PACKAGE" 43121)"
fi

adb_target -s "$SERIAL" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
adb_target -s "$SERIAL" forward "tcp:$HOST_PORT" "tcp:$port"
FORWARD_ACTIVE=true

health_response="$("${CURL[@]}" --fail --silent \
  -H 'content-type: application/json' \
  -H "authorization: Bearer $capability" \
  -d '{"_tag":"system.health","payload":{}}' \
  "http://127.0.0.1:$HOST_PORT/rpc")"
if ! jq -e '
  ._tag == "system.health"
  and .outcome._tag == "Ok"
  and (.outcome.payload.version | type == "string" and length > 0)
' <<<"$health_response" >/dev/null; then
  echo "Embedded brain health while game foreground was not a valid Ok health response: $health_response" >&2
  exit 1
fi
local_games_response="$("${CURL[@]}" --fail --silent \
  -H 'content-type: application/json' \
  -H "authorization: Bearer $capability" \
  -d '{"_tag":"app.local-games.list","payload":{}}' \
  "http://127.0.0.1:$HOST_PORT/rpc")"
if ! jq -e '
  .outcome._tag == "Ok"
  and .outcome.payload.games[0].id == "01K4J6K8Y00000000000000001"
' <<<"$local_games_response" >/dev/null; then
  echo "Embedded brain survived but Android route state did not: $local_games_response" >&2
  exit 1
fi
if [[ "$EXPECT_RETROARCH_ROUTE" == true ]] && ! jq -e '
  any(.outcome.payload.games[]; .id == "01K4J6K8Y00000000000000002")
' <<<"$local_games_response" >/dev/null; then
  echo "Embedded brain lost the canonical RetroArch route: $local_games_response" >&2
  exit 1
fi

printf 'Android app route package: %s pid=%s\n' "$GAME" "$pid"
printf 'Android app route topResumedActivity: %s\n' "$top_activity"
printf 'Android app route health while game foreground: %s\n' "$health_response"
printf 'Android app route local games while game foreground: %s\n' "$local_games_response"
