#!/usr/bin/env bash
# Deterministic review checks for Android device shell gates. These do not
# contact hardware; they guard the safety properties that are otherwise easy to
# regress while preserving the real device gates as the source of journey truth.
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
CRATE="$ROOT/services/korrid"
ANDROID_SMOKE="$CRATE/android-smoke.sh"
ANDROID_APP_ROUTE="$CRATE/android-app-route-check.sh"
ANDROID_GAME_DISCOVERY="$CRATE/android-game-discovery-check.sh"
ANDROID_INSTRUMENTATION_RESULT="$CRATE/android-instrumentation-result.sh"
JOURNEY_RESUME="$CRATE/journey-resume.sh"
OVERLAY_ACCEPTANCE="$ROOT/clients/android/overlay-acceptance.sh"
RETROARCH_ACCEPTANCE="$ROOT/plugins/retroarch/android/device-acceptance.sh"
KORRI_SHELL="$ROOT/clients/android/app/src/main/java/com/limelight/KorriShellActivity.java"
ANDROID_GAME="$ROOT/clients/android/app/src/main/java/com/limelight/Game.java"
OVERLAY_SERVICE="$ROOT/clients/android/app/src/main/java/com/limelight/korri/overlay/KorriOverlayService.java"
DEBUG_AUTHORITY="$CRATE/android-debug-capability.sh"
DEBUG_LAUNCH_LOCAL="$CRATE/android-debug-launch-local.sh"
DEBUG_PORTAL_TARGET="$CRATE/android-debug-portal-target.sh"
DEBUG_PORTAL_RELOAD="$CRATE/android-debug-reload-portal.sh"
DEBUG_PORTAL_FOCUS_GAME="$CRATE/android-debug-focus-portal-game.sh"

# The review's canonical cases must stay deterministic even when a developer's
# shell is primed for an alternate device-gate run. Individual alternate cases
# below set their own overrides explicitly.
for name in "${!KORRI_DEVICE_SCRIPT_REVIEW_@}"; do
  unset "$name"
done
unset \
  ANDROID_SERIAL \
  KORRI_ADB_BIN \
  KORRI_ANDROID_APK \
  KORRI_ANDROID_APP_PACKAGE \
  KORRI_ANDROID_APP_ROUTE_CHECKPOINT_LIBRARY \
  KORRI_ANDROID_APP_ROUTE_HOST_PORT \
  KORRI_ANDROID_APP_ROUTE_JOURNEY_SH \
  KORRI_ANDROID_APP_ROUTE_SMOKE_SH \
  KORRI_ANDROID_DEVICE \
  KORRI_ANDROID_DEBUG_AUTHORITY_JSON \
  KORRI_ANDROID_DEBUG_CAPABILITY \
  KORRI_ANDROID_DEBUG_CAPABILITY_SH \
  KORRI_ANDROID_DEBUG_LAUNCH_LOCAL_SH \
  KORRI_ANDROID_DEBUG_PORTAL_RELOAD_SH \
  KORRI_ANDROID_DEBUG_PORTAL_FOCUS_GAME_SH \
  KORRI_ANDROID_GAME_DISCOVERY_DEVTOOLS_HOST_PORT \
  KORRI_ANDROID_GAME_DISCOVERY_HOST_PORT \
  KORRI_ANDROID_SMOKE_LIBRARY \
  KORRI_ANDROID_UPSTREAMS_CONFIG \
  KORRI_JOURNEY_EXPECTED_TITLE \
  KORRI_MAGICK_BIN \
  KORRI_OVERLAY_ACCEPT_SCOPE \
  KORRI_TESSERACT_BIN \
  SHOTS

# Child gates exercise authenticated assertions with a deterministic atomic
# authority rather than recovering a port from fake or historical logcat.
export KORRI_ANDROID_DEBUG_AUTHORITY_JSON='{"port":43210,"capability":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}'
export KORRI_ANDROID_DEBUG_CAPABILITY=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa

bash -n "$ANDROID_SMOKE" "$ANDROID_APP_ROUTE" "$ANDROID_INSTRUMENTATION_RESULT" "$JOURNEY_RESUME" \
  "$OVERLAY_ACCEPTANCE" "$RETROARCH_ACCEPTANCE" \
  "$CRATE/android-debug-capability.sh" "$CRATE/android-debug-launch-local.sh" \
  "$CRATE/android-debug-portal-target.sh" "$CRATE/android-debug-reload-portal.sh" \
  "$CRATE/android-debug-focus-portal-game.sh" \
  "$CRATE/test-android-debug-capability.sh" \
  "$CRATE/test-android-debug-launch-local.sh" \
  "$CRATE/test-android-game-discovery-authority-recovery.sh" \
  "$CRATE/test-android-debug-reload-portal.sh" \
  "$CRATE/test-android-debug-focus-portal-game.sh" \
  "$CRATE/test-overlay-acceptance-identity.sh" \
  "$CRATE/test-overlay-local-publication.sh" \
  "$CRATE/test-overlay-evidence-predicates.sh" \
  "$ROOT/clients/android/local-launch-publication.sh" \
  "$ROOT/clients/android/overlay-evidence-predicates.sh"
"$CRATE/test-android-debug-capability.sh"
"$CRATE/test-android-debug-launch-local.sh"
"$CRATE/test-android-game-discovery-authority-recovery.sh"
"$CRATE/test-android-debug-reload-portal.sh"
"$CRATE/test-android-debug-focus-portal-game.sh"
"$CRATE/test-overlay-acceptance-identity.sh"
"$CRATE/test-overlay-local-publication.sh"
"$CRATE/test-overlay-evidence-predicates.sh"

# shellcheck source=services/korrid/android-instrumentation-result.sh
source "$ANDROID_INSTRUMENTATION_RESULT"

assert_instrumentation_decision() {
  local name="$1"
  local expected="$2"
  local status="$3"
  local output="$4"
  local log_file=""
  local actual=""
  log_file="$(mktemp)"
  printf '%s\n' "$output" >"$log_file"
  if korri_android_instrumentation_passed "$status" "$log_file"; then
    actual=pass
  else
    actual=fail
  fi
  rm -f "$log_file"
  if [[ "$actual" != "$expected" ]]; then
    echo "instrumentation decision case failed: $name expected $expected got $actual" >&2
    exit 1
  fi
}

assert_instrumentation_decision \
  'status 0 with JUnit completion' \
  pass \
  0 \
  $'INSTRUMENTATION_STATUS: ok\nOK (1 test)'
assert_instrumentation_decision \
  'status 0 with JUnit failures' \
  fail \
  0 \
  $'INSTRUMENTATION_STATUS: ok\nFAILURES!!!\nTests run: 1, Failures: 1'
assert_instrumentation_decision \
  'instrumentation failed marker' \
  fail \
  0 \
  $'INSTRUMENTATION_FAILED: com.limelight.KorriGameDiscoveryDebugTest\nOK (1 test)'
assert_instrumentation_decision \
  'nonzero adb status' \
  fail \
  1 \
  $'OK (1 test)'
assert_instrumentation_decision \
  'missing JUnit completion' \
  fail \
  0 \
  $'INSTRUMENTATION_STATUS: ok\nINSTRUMENTATION_CODE: 0'

local_overlay_discovery="$(sed -n '/begin_evidence_checkpoint local-overlay-open/,/begin_evidence_checkpoint local-mid-overlay-end/p' "$OVERLAY_ACCEPTANCE")"
# shellcheck disable=SC2016 # Literal source-contract needles.
for needle in \
  'new_checkpoint_log_marker local-overlay-publication' \
  'start_local_publication_capture' \
  'korri_parse_wario_retroarch_publication' \
  'record_gate_retroarch_pid "$local_pid"' \
  'record_gate_launch "$local_launch_id"'; do
  grep -F "$needle" <<<"$local_overlay_discovery" >/dev/null || {
    echo "overlay local discovery is missing publication-bound step: $needle" >&2
    exit 1
  }
done
if grep -F 'app.local-games.launch' <<<"$local_overlay_discovery" >/dev/null; then
  echo 'overlay local discovery must not issue a second launch RPC' >&2
  exit 1
fi
grep -F "https://appassets.androidplatform.net/assets/portal/index.html" "$DEBUG_AUTHORITY" >/dev/null
grep -F '({port: KorriNative.korridPort(), capability: KorriNative.korridCapability()})' "$DEBUG_AUTHORITY" >/dev/null
grep -F 'keys == ["capability", "port"]' "$DEBUG_AUTHORITY" >/dev/null
grep -F 'test("^[0-9a-f]{64}$")' "$DEBUG_AUTHORITY" >/dev/null
for helper in "$DEBUG_AUTHORITY" "$DEBUG_LAUNCH_LOCAL" "$DEBUG_PORTAL_RELOAD" "$DEBUG_PORTAL_FOCUS_GAME"; do
  grep -F 'android-debug-portal-target.sh' "$helper" >/dev/null
done
grep -F "const hasNative = typeof native === 'object'" "$DEBUG_PORTAL_TARGET" >/dev/null
grep -F "typeof native.korridPort === 'function'" "$DEBUG_PORTAL_TARGET" >/dev/null
grep -F "typeof native.korridCapability === 'function'" "$DEBUG_PORTAL_TARGET" >/dev/null
grep -F 'shell_count" -eq 1' "$DEBUG_PORTAL_TARGET" >/dev/null
if grep -Eq 'logcat|KorridServer|KorriPortal' "$DEBUG_AUTHORITY"; then
  echo 'debug authority must not depend on logcat readiness or port history' >&2
  exit 1
fi
for acceptance in "$OVERLAY_ACCEPTANCE" "$RETROARCH_ACCEPTANCE"; do
  grep -F 'KORRI_ANDROID_DEBUG_AUTHORITY_JSON' "$acceptance" >/dev/null
  # shellcheck disable=SC2016 # Literal source-contract needle.
  grep -F -- '"$DEBUG_CAPABILITY_SH" "$SERIAL" "$KORRI_PACKAGE" --json' "$acceptance" >/dev/null
  discovery_source="$(sed -n '/^discover_live_korri_authority() {/,/^}/p' "$acceptance")"
  if grep -Eq 'logcat|KorridServer|KorriPortal|listening on 127' <<<"$discovery_source"; then
    echo "authority discovery must not depend on historical logcat: $acceptance" >&2
    exit 1
  fi
  # shellcheck disable=SC2016 # Literal source-contract pattern.
  if grep -Eq '(echo|printf)[^[:cntrl:]]*\$(authority_json|CAPABILITY|capability)' "$acceptance"; then
    echo "acceptance must never print its debug authority: $acceptance" >&2
    exit 1
  fi
done
grep -F "https://appassets.androidplatform.net/assets/portal/index.html" "$DEBUG_PORTAL_RELOAD" >/dev/null
grep -F 'korri_debug_select_main_portal_socket' "$DEBUG_PORTAL_RELOAD" >/dev/null
# Android WebView may preserve performance.timeOrigin across a real reload, so
# the reload proof must be a random own-Window marker that cannot survive a
# fresh document. Timing-origin comparison must never come back as the witness.
grep -E '__korriReloadProbe_|reload_marker_key' "$DEBUG_PORTAL_RELOAD" >/dev/null
grep -F 'od -An -N16 -tx1 /dev/urandom' "$DEBUG_PORTAL_RELOAD" >/dev/null
grep -F '.markerSet == true' "$DEBUG_PORTAL_RELOAD" >/dev/null
grep -F '.reloadMarkerPresent == false' "$DEBUG_PORTAL_RELOAD" >/dev/null
if grep -F "performance.timeOrigin" "$DEBUG_PORTAL_RELOAD" >/dev/null; then
  echo 'debug portal reload must not prove a fresh document with performance.timeOrigin' >&2
  exit 1
fi
grep -F "navigationType" "$DEBUG_PORTAL_RELOAD" >/dev/null
grep -F "data-shift-game-id" "$DEBUG_PORTAL_RELOAD" >/dev/null
if grep -Eq 'KorriNative|korridCapability|surface=overlay' "$DEBUG_PORTAL_RELOAD"; then
  echo 'debug portal reload must never inspect a capability or select the overlay page' >&2
  exit 1
fi
if grep -Eq 'force-stop|am[[:space:]]+kill|pm[[:space:]]+clear|install|uninstall' "$DEBUG_PORTAL_RELOAD"; then
  echo 'debug portal reload must never restart, clear, or reinstall Korri' >&2
  exit 1
fi
grep -F "https://appassets.androidplatform.net/assets/portal/index.html" "$DEBUG_PORTAL_FOCUS_GAME" >/dev/null
grep -F 'korri_debug_select_main_portal_socket' "$DEBUG_PORTAL_FOCUS_GAME" >/dev/null
grep -F "document.querySelectorAll('[data-shift-library]').length !== 1" "$DEBUG_PORTAL_FOCUS_GAME" >/dev/null
grep -F 'target.focus()' "$DEBUG_PORTAL_FOCUS_GAME" >/dev/null
grep -F 'shift.cine-library-tile' "$DEBUG_PORTAL_FOCUS_GAME" >/dev/null
grep -F -- '--verify-library' "$DEBUG_PORTAL_FOCUS_GAME" >/dev/null
if grep -Eq 'KorriNative|korridCapability|surface=overlay|\.click\(|dispatchEvent\(|fetch\(|XMLHttpRequest' "$DEBUG_PORTAL_FOCUS_GAME"; then
  echo 'debug portal focus must not activate controls, dispatch input, inspect capabilities, use network, or select overlay pages' >&2
  exit 1
fi
if grep -Eq 'force-stop|am[[:space:]]+kill|pm[[:space:]]+(clear|install|uninstall)|adb[[:space:]]+(install|uninstall)' "$DEBUG_PORTAL_FOCUS_GAME"; then
  echo 'debug portal focus must never restart, clear, or reinstall Korri' >&2
  exit 1
fi
for acceptance in "$OVERLAY_ACCEPTANCE" "$RETROARCH_ACCEPTANCE"; do
  grep -F 'DEBUG_PORTAL_RELOAD_SH=' "$acceptance" >/dev/null
  grep -F -- "--expect-game wl4 'Wario Land 4'" "$acceptance" >/dev/null
  grep -F -- '--expect-portal' "$acceptance" >/dev/null
done
grep -F 'DEBUG_PORTAL_FOCUS_GAME_SH=' "$RETROARCH_ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F -- '"$SERIAL" "$KORRI_PACKAGE" --library' "$RETROARCH_ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F -- '"$SERIAL" "$KORRI_PACKAGE" --verify-library' "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F -- "--game 'local-game:wl4' 'Wario Land 4'" "$RETROARCH_ACCEPTANCE" >/dev/null
if grep -F 'KEYCODE_BUTTON_A' "$RETROARCH_ACCEPTANCE" >/dev/null; then
  echo 'RetroArch acceptance must not assume physical A after DevTools focus' >&2
  exit 1
fi
library_focus_source="$(sed -n '/^focus_wario_in_installed_library() {/,/^}/p' "$RETROARCH_ACCEPTANCE")"
if grep -F 'KEYCODE_DPAD_RIGHT' <<<"$library_focus_source" >/dev/null; then
  echo 'RetroArch acceptance must not assume retained Home focus before opening Library' >&2
  exit 1
fi
[[ "$(grep -Fc 'shell input tap' <<<"$library_focus_source")" -eq 1 ]]
# The measured Library tile reaches within a few pixels of the RG405M system
# taskbar, so its activation point must stay in the verified upper quarter of
# the exact tile rather than its center.
# shellcheck disable=SC2016 # Literal source-contract needles.
grep -F 'verified_library_system_edge_avoiding_point "$navigation_json"' \
  <<<"$library_focus_source" >/dev/null
if grep -F 'verified_element_center "$navigation_json"' \
  <<<"$library_focus_source" >/dev/null; then
  echo 'installed Library activation must avoid the system edge, not use the tile center' >&2
  exit 1
fi
grep -F 'Physical A activation is retained by the human unified-overlay gate' \
  <<<"$library_focus_source" >/dev/null

# Unified-overlay acceptance remains human-led and state restoring. These
# source contracts deliberately do not substitute for the device gate.
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'EXPECTED_MODEL="$2"' "$OVERLAY_ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'EXPECTED_HARDWARE_SERIAL="$3"' "$OVERLAY_ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'DIRECT_PACKAGE="$4"' "$OVERLAY_ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'UNRELATED_PACKAGE="$5"' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'ACTUAL_MODEL=' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'ACTUAL_HARDWARE_SERIAL=' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'getprop ro.serialno' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'expected_hardware_serial=%s' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'actual_hardware_serial=%s' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'ANDROID_PACKAGE_PATTERN=' "$OVERLAY_ACCEPTANCE" >/dev/null
# Korri's RetroArch publishes no launcher activity, so the direct-launch
# negative must use a user-launchable emulator while the fork's own
# non-launchability is asserted automatically.
grep -F 'it cannot be launched outside Korri' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'assert_korri_retroarch_is_not_user_launchable' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'assert_direct_package_is_user_launchable' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'must publish no launcher activity' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'TARGET_SERIAL=' "$OVERLAY_ACCEPTANCE" >/dev/null
model_check_line="$(grep -nF 'device model mismatch:' "$OVERLAY_ACCEPTANCE" | cut -d: -f1)"
hardware_check_line="$(grep -nF 'hardware serial mismatch:' "$OVERLAY_ACCEPTANCE" | cut -d: -f1)"
trap_line="$(grep -nF 'trap cleanup EXIT' "$OVERLAY_ACCEPTANCE" | cut -d: -f1)"
[[ -n "$model_check_line" && -n "$hardware_check_line" && -n "$trap_line" \
  && "$trap_line" -gt "$model_check_line" \
  && "$trap_line" -gt "$hardware_check_line" ]] || {
  echo 'overlay acceptance must verify exact ADB endpoint, model, and hardware serial before installing its EXIT trap' >&2
  exit 1
}
if grep -F '/sdcard/korri-overlay-acceptance.png' "$OVERLAY_ACCEPTANCE" >/dev/null; then
  echo 'overlay acceptance must not delete or reuse a generic remote screenshot path' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal source-contract needles.
grep -F 'timeout 15 "$ADB_BIN"' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'require_preinstalled' "$OVERLAY_ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needles.
grep -F 'LOCK_REMOTE="$STORAGE_ROOT/.android-app-route-check.lock"' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'backup_before_mutation' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'trap cleanup EXIT' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'retroarch.cfg' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'wl4.state.auto' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'wl4.srm' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'PREFS_BACKUP=' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'RUN_NONCE=' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F '/dev/urandom' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F '^[0-9a-f]{32}$' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'BACKUP_OWNER_REMOTE=' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'PREFS_BACKUP_OWNER=' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'BACKUP_CREATED=false' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'PREFS_BACKUP_CREATED=false' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'remove_owned_external_backup' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'remove_owned_preferences_backup' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'refusing pre-existing external backup directory' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'refusing pre-existing app-private backup directory' "$OVERLAY_ACCEPTANCE" >/dev/null
backup_path_sources="$(grep -E '^(BACKUP_REMOTE|PREFS_BACKUP|PREFS_WORK_DIR)=' "$OVERLAY_ACCEPTANCE")"
if grep -Eq '\$\$|\$\{?(BASHPID|PPID)\}?' <<<"$backup_path_sources"; then
  echo 'overlay acceptance backup and work paths must never be PID-derived' >&2
  exit 1
fi
grep -F 'shared-preferences-snapshot.py' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'PREFS_SEMANTIC_BEFORE=' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'PREFS_SEMANTIC_AFTER=' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'required materialized SharedPreferences keys are absent' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'KORRI_STREAM_CONNECTION_LOSS_PROBE' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'GATE_LAUNCH_IDS' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'discover_live_korri_authority' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'assert_pristine_gate_state' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'assert_session_idle' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'revalidate_gate_state_after_mutation' "$OVERLAY_ACCEPTANCE" >/dev/null
overlay_discovery_line="$(grep -nF 'discover_live_korri_authority ||' "$OVERLAY_ACCEPTANCE" | head -1 | cut -d: -f1)"
overlay_pristine_line="$(grep -nF 'assert_pristine_gate_state' "$OVERLAY_ACCEPTANCE" | tail -1 | cut -d: -f1)"
overlay_idle_line="$(grep -nF 'assert_session_idle' "$OVERLAY_ACCEPTANCE" | tail -1 | cut -d: -f1)"
overlay_mutation_line="$(grep -nF 'backup_before_mutation' "$OVERLAY_ACCEPTANCE" | tail -1 | cut -d: -f1)"
[[ -n "$overlay_discovery_line" && -n "$overlay_pristine_line" \
  && -n "$overlay_idle_line" && -n "$overlay_mutation_line" \
  && "$overlay_discovery_line" -lt "$overlay_mutation_line" \
  && "$overlay_pristine_line" -lt "$overlay_mutation_line" \
  && "$overlay_idle_line" -lt "$overlay_mutation_line" ]] || {
  echo 'overlay acceptance must prove live RPC, idle session, activities, processes, and foreground before mutation' >&2
  exit 1
}
if sed '/^[[:space:]]*#/d' "$OVERLAY_ACCEPTANCE" | grep -Eq 'settings[[:space:]]+(put|delete)'; then
  echo 'overlay acceptance must not rewrite Android settings during grant-sensitive acceptance' >&2
  exit 1
fi
grep -F 'enabled_accessibility_services' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'assert_accessibility_service_enabled' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'semantic_control_values' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'value: .interaction.payload.value' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'close_exact_acceptance_paths' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'recovery: restore every changed control through the Korri gameplay overlay' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'capture_evidence' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'required_top_activity' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'required_window_records' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'required_active_controls' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'required_lifecycle_records' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'evidence_predicate=' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'begin_evidence_checkpoint' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F '[rpc responses]' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F '[checkpoint-bounded lifecycle records]' "$OVERLAY_ACCEPTANCE" >/dev/null
capture_evidence_source="$(sed -n '/^capture_evidence() {/,/^}/p' "$OVERLAY_ACCEPTANCE")"
# Collecting an optional checkpoint predicate may tolerate no match, because an
# empty predicate is rejected explicitly straight afterwards. Every device or
# RPC probe in this function is required evidence and must still fail closed.
if grep -E '(adb_shell|adb_target|dumpsys|screencap|controls_for_launch|package_pid|rpc )' \
  <<<"$capture_evidence_source" | grep -F '|| true' >/dev/null; then
  echo 'overlay acceptance must not mask a required structured evidence probe' >&2
  exit 1
fi
if grep -F '|| true' <<<"$capture_evidence_source" \
  | grep -Ev '(predicate|required_lifecycle_records)' >/dev/null; then
  echo 'overlay acceptance may tolerate no match only while collecting a checkpoint predicate' >&2
  exit 1
fi
grep -F 'required checkpoint predicate' "$OVERLAY_ACCEPTANCE" >/dev/null
if grep -Eq 'logcat[^[:cntrl:]]*(-t[ =]?1000|tail[[:space:]]+-n?[[:space:]]*1000)' "$OVERLAY_ACCEPTANCE"; then
  echo 'overlay acceptance must not satisfy checkpoints from a stale generic last-1000 log search' >&2
  exit 1
fi
for predicate in \
  positive-overlay \
  stale-rpc \
  foreground-suspended \
  direct-no-active-launch \
  service-disabled; do
  grep -F "'$predicate'" "$OVERLAY_ACCEPTANCE" >/dev/null || {
    echo "overlay acceptance is missing checkpoint-specific evidence predicate: $predicate" >&2
    exit 1
  }
done
if grep -A2 -E 'capture_evidence (unrelated-active-session-negative|direct-launch-negative|permission-disabled)' "$OVERLAY_ACCEPTANCE" \
  | grep -Eq "'request-show'[[:space:]]+'accepted'"; then
  echo 'negative overlay checkpoints must not require a stale prior positive event' >&2
  exit 1
fi

# Scope must be explicit, fail-closed, and recorded, so a narrowed run can never
# read as full unified coverage.
# shellcheck disable=SC2016 # Literal source-contract needles.
for needle in \
  'ACCEPT_SCOPE="${KORRI_OVERLAY_ACCEPT_SCOPE:-full}"' \
  'unknown KORRI_OVERLAY_ACCEPT_SCOPE:' \
  'STREAM_SCOPE_SKIPPED_STAGES=(' \
  'announce_scope' \
  'scope_runs_local_stages' \
  'acceptance_scope=%s' \
  'acceptance-scope.txt'; do
  grep -F "$needle" "$OVERLAY_ACCEPTANCE" >/dev/null || {
    echo "overlay acceptance is missing scope contract: $needle" >&2
    exit 1
  }
done
# Both the per-checkpoint sidecar and the final summary must attribute local
# parity to ra-accept, so neither artifact alone can imply full coverage.
overlay_sidecar_source="$(sed -n '/^capture_evidence() {/,/^}/p' "$OVERLAY_ACCEPTANCE")"
overlay_summary_end_line="$(grep -nF '} >"$EVIDENCE_DIR/acceptance-scope.txt"' "$OVERLAY_ACCEPTANCE" | cut -d: -f1)"
[[ -n "$overlay_summary_end_line" ]] || {
  echo 'overlay acceptance must write a final acceptance-scope summary artifact' >&2
  exit 1
}
overlay_summary_source="$(awk -v stop="$overlay_summary_end_line" '
  NR <= stop { buffer = buffer $0 "\n" }
  /^\{$/ && NR <= stop { buffer = $0 "\n" }
  END { printf "%s", buffer }
' "$OVERLAY_ACCEPTANCE")"
for scope_artifact in sidecar summary; do
  case "$scope_artifact" in
    sidecar) scope_artifact_source="$overlay_sidecar_source" ;;
    summary) scope_artifact_source="$overlay_summary_source" ;;
  esac
  grep -F 'acceptance_scope=' <<<"$scope_artifact_source" >/dev/null || {
    echo "overlay acceptance $scope_artifact must record its acceptance scope" >&2
    exit 1
  }
  grep -F 'local_parity_source=ra-accept' <<<"$scope_artifact_source" >/dev/null || {
    echo "overlay acceptance $scope_artifact must attribute narrowed-scope local parity to ra-accept" >&2
    exit 1
  }
  grep -F 'skipped_stages=' <<<"$scope_artifact_source" >/dev/null || {
    echo "overlay acceptance $scope_artifact must record which stages were skipped" >&2
    exit 1
  }
done
overlay_scope_case="$(sed -n '/^ACCEPT_SCOPE="\${KORRI_OVERLAY_ACCEPT_SCOPE:-full}"$/,/^esac$/p' "$OVERLAY_ACCEPTANCE")"
grep -E '^[[:space:]]*full\|stream\)' <<<"$overlay_scope_case" >/dev/null || {
  echo 'overlay acceptance must accept exactly the full and stream scopes' >&2
  exit 1
}
grep -F 'exit 2' <<<"$overlay_scope_case" >/dev/null || {
  echo 'overlay acceptance must reject unknown scopes before device contact' >&2
  exit 1
}
overlay_scope_declaration_line="$(grep -nF 'ACCEPT_SCOPE="${KORRI_OVERLAY_ACCEPT_SCOPE:-full}"' "$OVERLAY_ACCEPTANCE" | cut -d: -f1)"
overlay_connect_line="$(grep -nF 'timeout 15 "$ADB_BIN" connect "$SERIAL"' "$OVERLAY_ACCEPTANCE" | head -1 | cut -d: -f1)"
[[ -n "$overlay_scope_declaration_line" && -n "$overlay_connect_line" \
  && "$overlay_scope_declaration_line" -lt "$overlay_connect_line" ]] || {
  echo 'overlay acceptance must validate its scope before contacting the device' >&2
  exit 1
}
overlay_local_gate_line="$(grep -nF 'if scope_runs_local_stages; then' "$OVERLAY_ACCEPTANCE" | head -1 | cut -d: -f1)"
overlay_local_open_line="$(grep -nF 'begin_evidence_checkpoint local-overlay-open' "$OVERLAY_ACCEPTANCE" | cut -d: -f1)"
overlay_direct_line="$(grep -nF 'begin_evidence_checkpoint direct-launch-negative' "$OVERLAY_ACCEPTANCE" | cut -d: -f1)"
[[ -n "$overlay_local_gate_line" && -n "$overlay_local_open_line" && -n "$overlay_direct_line" \
  && "$overlay_local_gate_line" -lt "$overlay_local_open_line" \
  && "$overlay_local_open_line" -lt "$overlay_direct_line" ]] || {
  echo 'overlay acceptance must gate the local RetroArch stages before the direct-launch negative' >&2
  exit 1
}
for skipped_stage in \
  local-overlay-open \
  local-mid-overlay-end \
  unrelated-active-session-negative \
  old-game-still-disarmed \
  fresh-publication-rearmed; do
  awk -v stage="$skipped_stage" -v start="$overlay_local_gate_line" -v stop="$overlay_direct_line" '
    NR > start && NR < stop && index($0, "capture_evidence " stage) == 1 { found = 1 }
    END { exit(found ? 0 : 1) }
  ' "$OVERLAY_ACCEPTANCE" || {
    echo "stream scope must skip local stage evidence: $skipped_stage" >&2
    exit 1
  }
  grep -F "  $skipped_stage" "$OVERLAY_ACCEPTANCE" >/dev/null || {
    echo "stream scope must name its skipped stage in the banner list: $skipped_stage" >&2
    exit 1
  }
done
for retained_stage in \
  direct-launch-negative \
  stream-overlay-open \
  stream-connection-loss-narrated \
  stream-graceful-return \
  stream-host-stop \
  permission-disabled \
  permission-recovered; do
  awk -v stage="$retained_stage" -v stop="$overlay_direct_line" '
    NR >= stop && index($0, "capture_evidence " stage) == 1 { found = 1 }
    END { exit(found ? 0 : 1) }
  ' "$OVERLAY_ACCEPTANCE" || {
    echo "stream scope must retain stage: $retained_stage" >&2
    exit 1
  }
done
grep -F '\"expectedLaunchId\":\"$parity_launch_id\"' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'exact secure host stop' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F '"KorriGameLifecycle"' "$ANDROID_GAME" >/dev/null
grep -F '"KorriOverlay"' "$OVERLAY_SERVICE" >/dev/null
grep -F 'logVisibility(null, suspendedLaunchId, "foreground-mismatch", "suspended")' "$OVERLAY_SERVICE" >/dev/null
grep -F 'could not establish safe quiescence; backup and lock retained' "$OVERLAY_ACCEPTANCE" >/dev/null
for token in \
  'LOCAL OVERLAY VERIFIED' \
  'LOCAL MID-OVERLAY END VERIFIED' \
  'UNRELATED ACTIVE-SESSION NEGATIVE VERIFIED' \
  'OLD GAME REMAINS DISARMED VERIFIED' \
  'FRESH KORRI PUBLICATION REARMS VERIFIED' \
  'STREAM OVERLAY VERIFIED' \
  'STREAM CONNECTION LOSS READY' \
  'STREAM GRACEFUL RETURN VERIFIED' \
  'DIRECT NEGATIVE VERIFIED' \
  'DIRECT NEGATIVE CLOSED VERIFIED' \
  'PERMISSION DISABLED BY HUMAN' \
  'PERMISSION RECOVERED BY HUMAN'; do
  grep -F "$token" "$OVERLAY_ACCEPTANCE" >/dev/null
done
# shellcheck disable=SC2016 # Literal exact-current invocation source contract.
grep -F 'invoke_control "$local_launch_id" '\''@korri:retroarch/quit'\''' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'SessionControls after end must be exactly Unavailable' "$OVERLAY_ACCEPTANCE" >/dev/null
grep -F 'Invocation after end must be exactly Unavailable' "$OVERLAY_ACCEPTANCE" >/dev/null
stale_predicate_source="$(sed -n '/stale-rpc)/,/;;/p' "$OVERLAY_ACCEPTANCE")"
if grep -Eq 'StaleSession| or ' <<<"$stale_predicate_source"; then
  echo 'ended-launch controls must require their exact Unavailable response' >&2
  exit 1
fi
grep -F 'DECODER/HOST FAILURE: REPOSITORY-ONLY' "$OVERLAY_ACCEPTANCE" >/dev/null
if grep -F "|| printf '{\"expected\"" "$OVERLAY_ACCEPTANCE" >/dev/null; then
  echo 'overlay acceptance must never synthesize expected RPC evidence after transport failure' >&2
  exit 1
fi
if grep -Eq 'telemetry":"|human-confirmed|asserted (idle|session|portal)' "$OVERLAY_ACCEPTANCE"; then
  echo 'overlay acceptance sidecars must not contain handwritten telemetry claims' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'EXPECTED_MODEL="$2"' "$RETROARCH_ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'EXPECTED_HARDWARE_SERIAL="$3"' "$RETROARCH_ACCEPTANCE" >/dev/null
# Measured Toybox nc returns either 0 or 124 after a successful no-response
# probe, so delivery is proven by empty output plus exactly one authenticated
# rejection rather than by one exit status.
grep -F 'unauthenticated UDP probe must report empty output and remote rc 0 or 124' \
  "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F 'remote_nc_rc=0 remote_nc_output=' "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F 'remote_nc_rc=124 remote_nc_output=' "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F 'UDP probe transport failed before its remote completion marker' "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F 'assert_no_artemis_game_activity' "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F 'assert_pristine_gate_state' "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F 'assert_session_idle' "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F 'revalidate_gate_state_after_mutation' "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F 'BACKUP_CREATED=false' "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F 'remove_owned_backup' "$RETROARCH_ACCEPTANCE" >/dev/null
grep -F 'assert_korri_process_unchanged' "$RETROARCH_ACCEPTANCE" >/dev/null
if grep -Eq 'launch_spec=|udp_unauthenticated.*\|\| true|pull /sdcard/korri-acceptance.png|rm -f /sdcard/korri-acceptance.png' "$RETROARCH_ACCEPTANCE"; then
  echo 'RetroArch acceptance contains speculative launch, masked UDP, or generic screenshot handling' >&2
  exit 1
fi

for acceptance_script in "$OVERLAY_ACCEPTANCE" "$RETROARCH_ACCEPTANCE"; do
  if sed '/^[[:space:]]*#/d' "$acceptance_script" \
    | grep -Eq 'pm[[:space:]]+(install|uninstall|clear|grant)([;&|[:space:]]|$)|adb[^[:cntrl:]]+[[:space:]]install([;&|[:space:]]|$)|(^|[[:space:]"])install[[:space:]]+-'; then
    echo "$(basename "$acceptance_script") must not install, uninstall, clear, or grant packages/permissions" >&2
    exit 1
  fi
  if sed '/^[[:space:]]*#/d' "$acceptance_script" \
    | grep -Eq '(force-stop|am[[:space:]]+kill|kill[[:space:]]+[^-]).*(KORRI_PACKAGE|KORRI_PID|com\.simonwjackson\.korri)'; then
    echo "$(basename "$acceptance_script") must never force-stop or kill Korri" >&2
    exit 1
  fi
done
if sed '/^[[:space:]]*#/d' "$OVERLAY_ACCEPTANCE" | grep -F 'rm -rf shared_prefs' >/dev/null \
  || sed '/^[[:space:]]*#/d' "$OVERLAY_ACCEPTANCE" | grep -F "cp -R '\$PREFS_BACKUP/shared_prefs' shared_prefs" >/dev/null; then
  echo 'overlay acceptance must keep its SharedPreferences backup read-only and restore controls through product actions' >&2
  exit 1
fi
final_service_assertion="$(sed -n '/cleanup() {/,/^}/p' "$OVERLAY_ACCEPTANCE")"
if ! grep -F 'assert_accessibility_service_enabled' <<<"$final_service_assertion" >/dev/null; then
  echo 'overlay acceptance cleanup must finally assert that the accessibility service remains enabled' >&2
  exit 1
fi
if sed '/^[[:space:]]*#/d' "$OVERLAY_ACCEPTANCE" \
  | grep -Eq 'settings[[:space:]]+(put|delete)[[:space:]]+secure[[:space:]]+(enabled_accessibility_services|accessibility_enabled)'; then
  echo 'overlay acceptance must never write Android accessibility settings' >&2
  exit 1
fi
if sed '/^[[:space:]]*#/d' "$OVERLAY_ACCEPTANCE" \
  | grep -Eq 'shell[[:space:]]+input|input[[:space:]]+(keyevent|tap|swipe)'; then
  echo 'overlay acceptance must use human physical-input checkpoints, not adb input' >&2
  exit 1
fi
if ! sed -n '/overlay-accept = {/,/^    };/p' "$ROOT/nix/tasks.nix" \
  | grep -F 'clients/android/overlay-acceptance.sh' >/dev/null; then
  echo 'overlay-accept Nix task must execute the reviewed acceptance script' >&2
  exit 1
fi

if grep -F 'debug capability=' "$KORRI_SHELL" "$ANDROID_SMOKE" "$ANDROID_APP_ROUTE" "$CRATE/brain-service-check.sh" >/dev/null; then
  echo 'Android runtime and device gates must never log or recover the full RPC capability from logcat' >&2
  exit 1
fi

bash -n "$ANDROID_SMOKE" "$ANDROID_APP_ROUTE" "$ANDROID_GAME_DISCOVERY" \
  "$ANDROID_INSTRUMENTATION_RESULT" "$JOURNEY_RESUME" "$DEBUG_LAUNCH_LOCAL"

if ! grep -F 'usage: android-game-discovery-check.sh --serial <adb-serial>' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must require an explicit --serial argument' >&2
  exit 1
fi
if grep -E 'SERIAL="?\$\{1|SERIAL="?\$\{KORRI_ANDROID_DEVICE|ANDROID_SERIAL' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must not default to a positional, KORRI_ANDROID_DEVICE, or ANDROID_SERIAL target' >&2
  exit 1
fi
if ! grep -F 'trap cleanup EXIT' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must clean up through a robust EXIT trap' >&2
  exit 1
fi
if ! grep -F 'run-as '\''$PKG'\''' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must isolate private discovery/credential state through run-as without reading tokens' >&2
  exit 1
fi
if grep -E 'steamgriddb\.credential.*cat|cat .*steamgriddb\.credential|Bearer .*SteamGridDB|KORRI_STEAMGRIDDB' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must never read, print, or inject a real SteamGridDB token' >&2
  exit 1
fi
if grep -F 'am broadcast' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must not add or use a broadcast/intent path bypass' >&2
  exit 1
fi
if ! grep -F 'am instrument -w' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must use the androidTest-only instrumentation seam for picker bypass' >&2
  exit 1
fi
if ! grep -F 'KorriGameDiscoveryDebugTest' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must target the debug discovery instrumentation test' >&2
  exit 1
fi
if grep -F 'launchSpecJson' "$ANDROID_GAME_DISCOVERY" "$ROOT/clients/android/app/src/androidTest/java/com/limelight/KorriGameDiscoveryDebugTest.java" >/dev/null \
  || grep -F 'launchSpecBase64' "$ANDROID_GAME_DISCOVERY" "$ROOT/clients/android/app/src/androidTest/java/com/limelight/KorriGameDiscoveryDebugTest.java" >/dev/null \
  || grep -F '"launchLocal".equals(action)' "$ROOT/clients/android/app/src/androidTest/java/com/limelight/KorriGameDiscoveryDebugTest.java" >/dev/null \
  || grep -F 'run_discovery_instrumentation launchLocal' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery launchLocal must not use instrumentation or argv payload transport' >&2
  exit 1
fi
mutation_block="$(sed -n '/launch_expression=/,/^launch_result=/p' "$DEBUG_LAUNCH_LOCAL")"
if ! grep -F 'DEBUG_LAUNCH_LOCAL_SH="${KORRI_ANDROID_DEBUG_LAUNCH_LOCAL_SH:-$CRATE/android-debug-launch-local.sh}"' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'expectedSigner: {' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'capability: $parts[2]' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F "printf '%s' \"\$envelope_json\"" "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F '| "$DEBUG_LAUNCH_LOCAL_SH" "$SERIAL" "$PKG" "$DEVTOOLS_HOST_PORT"' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'Trusted same-process portal launchLocal schedule ack: LaunchScheduled' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'assert_retroarch_not_resumed' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'wait_retroarch_resumed_after_launch' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'envelope_input="$(cat)"' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'keys == ["expectedSigner", "spec"]' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'keys == ["capability", "port"]' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'readiness_matches_expected "$readiness"' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'native.korridPort()' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'native.korridCapability()' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'base64 -w 0' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'source "$SCRIPT_DIR/android-debug-portal-target.sh"' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'korri_debug_select_main_portal_socket "$targets"' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'setTimeout(() => {' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F "return {_tag: 'LaunchScheduled'}" "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'const expectedPort = $expected_port;' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'const expectedCapability = $expected_capability_js;' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || [[ "$(grep -Fc 'signerMatches()' "$DEBUG_LAUNCH_LOCAL")" -lt 2 ]] \
  || [[ "$(grep -Fo 'launchLocal(specJson)' "$DEBUG_LAUNCH_LOCAL" | wc -l | tr -d ' ')" -ne 1 ]] \
  || ! grep -F 'FORWARD_ACTIVE=true' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F '"$TIMEOUT_BIN" 10 "${ADB[@]}"' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'pidof stderr:' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'failed to remove trusted portal DevTools forward during cleanup' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || grep -F 'LaunchFailed' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'korri_debug_evaluate_once "$socket" "$launch_expression"' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || [[ "$(grep -Fc 'korri_debug_evaluate_once "$socket" "$launch_expression"' "$DEBUG_LAUNCH_LOCAL")" -ne 1 ]] \
  || ! grep -F 'trusted portal launchLocal schedule ack was lost or refused; not retrying' "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! grep -F 'korri_debug_evaluate_once()' "$DEBUG_PORTAL_TARGET" >/dev/null \
  || grep -Eq 'for |while |until |sleep ' <<<"$mutation_block"; then
  echo 'launchLocal proof must use trusted same-process DevTools stdin envelope helper with expected signer binding and exactly one scheduled mutation evaluation' >&2
  exit 1
fi
if ! awk '
  /assert_two_u9_games_listable "after all-files recovery"/ { recovery = NR }
  /launch_local_spec "\$launch_response"/ { launch = NR; after_launch = 1; next }
  after_launch && /wait_retroarch_resumed_after_launch/ { foreground = NR }
  after_launch && (/rpc "/ || /rescan "/ || /wait_discovery_idle/ || /assert_two_u9_games_listable/) {
    rpc_dependent_after_launch = 1
  }
  /Android game discovery check passed on/ { success = NR }
  END { exit !(recovery && launch > recovery && foreground > launch && success > foreground && !rpc_dependent_after_launch) }
' "$ANDROID_GAME_DISCOVERY"; then
  echo 'android-game-discovery launch must remain the terminal proof after every RPC-based recovery assertion' >&2
  exit 1
fi
if [[ "$(grep -Fc '|Resumed:)' "$ANDROID_GAME_DISCOVERY")" -lt 2 ]]; then
  echo 'android-game-discovery-check.sh must recognize TrebleDroid current-activity Resumed evidence in preflight and launch verification' >&2
  exit 1
fi
if ! grep -F 'app.discovery.registerReceipt' "$ROOT/clients/android/app/src/androidTest/java/com/limelight/KorriGameDiscoveryDebugTest.java" >/dev/null; then
  echo 'KorriGameDiscoveryDebugTest must register through the production registerReceipt RPC' >&2
  exit 1
fi
if ! grep -F 'app.discovery.snapshot' "$ROOT/clients/android/app/src/androidTest/java/com/limelight/KorriGameDiscoveryDebugTest.java" >/dev/null \
  || ! grep -F 'app.local-games.list' "$ROOT/clients/android/app/src/androidTest/java/com/limelight/KorriGameDiscoveryDebugTest.java" >/dev/null \
  || ! grep -F 'Discovery should finish Idle before instrumentation returns' "$ROOT/clients/android/app/src/androidTest/java/com/limelight/KorriGameDiscoveryDebugTest.java" >/dev/null; then
  echo 'KorriGameDiscoveryDebugTest must wait for Idle discovery and visible local games before register instrumentation exits' >&2
  exit 1
fi
if ! grep -F 'issueFolderSelectionReceipt' "$ROOT/clients/android/app/src/androidTest/java/com/limelight/KorriGameDiscoveryDebugTest.java" >/dev/null; then
  echo 'KorriGameDiscoveryDebugTest must use the same JNI receipt issuer as the picker' >&2
  exit 1
fi
if grep -F 'ACTION_OPEN_DOCUMENT_TREE' "$ROOT/clients/android/app/src/androidTest/java/com/limelight/KorriGameDiscoveryDebugTest.java" >/dev/null; then
  echo 'KorriGameDiscoveryDebugTest must not automate the Android system folder picker' >&2
  exit 1
fi
if ! grep -F 'KorriDiscovery' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must capture structured scan metrics from the narrow debug-safe log seam' >&2
  exit 1
fi
if ! grep -F 'korri.discovery.scanMetrics.v1' "$ROOT/services/korrid/src/discovery/coordinator.rs" >/dev/null; then
  echo 'discovery coordinator must keep the structured scan metric schema for the device gate' >&2
  exit 1
fi
if ! grep -F 'android-game-discovery-check = {' "$ROOT/nix/tasks.nix" >/dev/null; then
  echo 'nix/tasks.nix must expose android-game-discovery-check as a Nix app' >&2
  exit 1
fi
if ! sed -n '/android-game-discovery-check = {/,/^    };/p' "$ROOT/nix/tasks.nix" | grep -F 'pkgs.jq' >/dev/null; then
  echo 'android-game-discovery-check task must put jq on PATH for structured RPC/log assertions' >&2
  exit 1
fi
if ! grep -F 'adb_failure_is_transient()' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'adb_reconnect_and_wait()' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'adb_command true' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must retry safe adb commands through bounded reconnect/wait handling' >&2
  exit 1
fi
if ! grep -F 'adb_target_once -s "$SERIAL" shell am instrument -w' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must not blindly retry non-idempotent instrumentation RPC mutations' >&2
  exit 1
fi
recovery_block="$(sed -n '/recover_rpc_details()/,/^}/p' "$ANDROID_GAME_DISCOVERY")"
if ! grep -F 'DEBUG_CAPABILITY_SH="${KORRI_ANDROID_DEBUG_CAPABILITY_SH:-$CRATE/android-debug-capability.sh}"' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'for authority_attempt in 1 2 3; do' <<<"$recovery_block" >/dev/null \
  || ! grep -F 'if [[ "$authority_attempt" -eq 3 ]]; then' <<<"$recovery_block" >/dev/null \
  || ! grep -F '"$DEBUG_CAPABILITY_SH" "$SERIAL" "$PKG" --json "$DEVTOOLS_HOST_PORT"' <<<"$recovery_block" >/dev/null \
  || ! grep -F 'keys == ["capability", "port"]' <<<"$recovery_block" >/dev/null \
  || ! grep -F 'test("^[0-9a-f]{64}$")' <<<"$recovery_block" >/dev/null \
  || ! grep -F 'host tcp:%s -> device tcp:%s via trusted portal DevTools' <<<"$recovery_block" >/dev/null; then
  echo 'android-game-discovery-check.sh must recover current RPC authority through trusted portal DevTools helper JSON' >&2
  exit 1
fi
if grep -Eq 'debug capability=|KorridServer|KorriPortal|listening on 127|logcat' <<<"$recovery_block"; then
  echo 'android-game-discovery-check.sh recover_rpc_details must not read RPC authority from logcat or historical server logs' >&2
  exit 1
fi
if grep -Eq '(^|[[:space:]])(sleep|while|until)([[:space:]]|$)' <<<"$recovery_block"; then
  echo 'android-game-discovery-check.sh recover_rpc_details must use bounded helper retry attempts without sleeps or unbounded loops' >&2
  exit 1
fi
if grep -Eq 'force-stop|am[[:space:]]+(start|kill)|pm[[:space:]]+(clear|install|uninstall)|adb[[:space:]]+(install|uninstall)' <<<"$recovery_block"; then
  echo 'android-game-discovery-check.sh recover_rpc_details must not restart, clear, install, or uninstall during helper recovery' >&2
  exit 1
fi
if grep -Eq '(echo|printf)[^[:cntrl:]]*\$(authority_json|RPC_CAPABILITY|capability)' <<<"$recovery_block"; then
  echo 'android-game-discovery-check.sh must never print recovered RPC capability material' >&2
  exit 1
fi
if ! grep -F 'DEVTOOLS_HOST_PORT="${KORRI_ANDROID_GAME_DISCOVERY_DEVTOOLS_HOST_PORT:-43120}"' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'validate_host_forward_port KORRI_ANDROID_GAME_DISCOVERY_HOST_PORT "$HOST_PORT"' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'validate_host_forward_port KORRI_ANDROID_GAME_DISCOVERY_DEVTOOLS_HOST_PORT "$DEVTOOLS_HOST_PORT"' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'if [[ "$HOST_PORT" == "$DEVTOOLS_HOST_PORT" ]]; then' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must use bounded distinct host ports for RPC and DevTools forwards' >&2
  exit 1
fi
if ! grep -F 'websocat' "$DEBUG_AUTHORITY" "$DEBUG_LAUNCH_LOCAL" >/dev/null \
  || ! sed -n '/android-game-discovery-check = {/,/^    };/p' "$ROOT/nix/tasks.nix" | grep -F 'pkgs.websocat' >/dev/null; then
  echo 'android-game-discovery-check task must put websocat on PATH for trusted portal DevTools helpers' >&2
  exit 1
fi
restart_block="$(sed -n '/restart_portal_and_recover()/,/^}/p' "$ANDROID_GAME_DISCOVERY")"
if ! grep -F 'restart_portal_and_recover()' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'clear_rpc_forward' <<<"$restart_block" >/dev/null \
  || grep -F 'logcat -c' <<<"$restart_block" >/dev/null \
  || ! grep -F "am force-stop '\$PKG'" <<<"$restart_block" >/dev/null \
  || ! grep -F "am start -n '\$PKG/com.limelight.KorriShellActivity'" <<<"$restart_block" >/dev/null \
  || ! grep -F 'recover_rpc_details "$label"' <<<"$restart_block" >/dev/null; then
  echo 'android-game-discovery-check.sh must stop the instrumentation target, preserve scan logs, restart KorriShellActivity, and recover fresh RPC details' >&2
  exit 1
fi
if ! awk '
  /register_folder "\$FIXTURE_A"/ { saw_a = NR }
  /restart_portal_and_recover "after first folder registration"/ { if (saw_a && !wait_a) restart_a = NR }
  /wait_discovery_idle "after first folder registration"/ { wait_a = NR }
  /register_folder "\$FIXTURE_B"/ { saw_b = NR }
  /restart_portal_and_recover "after second folder registration"/ { if (saw_b && !wait_b) restart_b = NR }
  /wait_discovery_idle "after second folder registration"/ { wait_b = NR }
  END { exit !(saw_a && restart_a > saw_a && wait_a > restart_a && saw_b && restart_b > saw_b && wait_b > restart_b) }
' "$ANDROID_GAME_DISCOVERY"; then
  echo 'android-game-discovery-check.sh must recover portal/RPC after each registration before polling discovery idle' >&2
  exit 1
fi
if ! awk '
  /set_appop_and_require_effective_mode ignore ignore/ { deny = NR }
  /move_fixture_roots_aside/ { unavailable = NR }
  /restart_portal_and_recover "after selected locations become unavailable"/ { deny_restart = NR }
  /rescan "with selected locations unavailable"/ { denied_rescan = NR }
  /assert_two_u9_games_unavailable "while selected locations are unavailable"/ { unavailable_routes = NR }
  /restore_fixture_roots/ { restored = NR }
  /set_appop_and_require_effective_mode allow allow/ { allow = NR }
  /restart_portal_and_recover "after all-files recovery"/ { allow_restart = NR }
  /rescan "after all-files recovery"/ { recovery_rescan = NR }
  END { exit !(deny && unavailable > deny && deny_restart > unavailable && denied_rescan > deny_restart && unavailable_routes > denied_rescan && restored > unavailable_routes && allow > restored && allow_restart > allow && recovery_rescan > allow_restart) }
' "$ANDROID_GAME_DISCOVERY"; then
  echo 'android-game-discovery-check.sh must make only its fixtures unavailable, report unavailable routes, restore fixtures, and recover fresh RPC authority' >&2
  exit 1
fi
if ! grep -F 'local log_file="$RUN_DIR/instrument-$action.log"' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'output="$(adb_target_once -s "$SERIAL" shell am instrument -w' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'source "$CRATE/android-instrumentation-result.sh"' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'korri_android_instrumentation_passed "$status" "$log_file"' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F "OK \\([[:space:]]*1 test[s]?\\)" "$ANDROID_INSTRUMENTATION_RESULT" >/dev/null \
  || ! grep -F 'INSTRUMENTATION_FAILED' "$ANDROID_INSTRUMENTATION_RESULT" >/dev/null \
  || grep -F 'INSTRUMENTATION_CODE: -?[1-9]' "$ANDROID_GAME_DISCOVERY" "$ANDROID_INSTRUMENTATION_RESULT" >/dev/null; then
  echo 'android-game-discovery-check.sh must capture instrumentation output and require the shared JUnit success decision instead of trusting adb exit status' >&2
  exit 1
fi
if grep -F -- '--retry' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must not apply blind curl retries to RPC mutations' >&2
  exit 1
fi
if ! grep -F 'set_appop_and_require_effective_mode ignore ignore' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must request and require effective MANAGE_EXTERNAL_STORAGE ignore before the denial rescan' >&2
  exit 1
fi
if ! grep -F 'set_appop_and_require_effective_mode allow allow' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must require effective MANAGE_EXTERNAL_STORAGE allow before normal discovery scans' >&2
  exit 1
fi
if ! grep -F 'selectedLocationIds' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'StorageUnavailable' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'EntryUnavailable' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'DiscoveryStorageUnavailable' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'index($locationId)' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must assert selected-location storage diagnostics while its fixture roots are unavailable under ignored all-files access' >&2
  exit 1
fi
if ! grep -F 'Primary Android game discovery check status was' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'cleanup also failed; preserving primary status' "$ANDROID_GAME_DISCOVERY" >/dev/null; then
  echo 'android-game-discovery-check.sh must keep primary failure and cleanup diagnostics distinct' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
cleanup_block="$(sed -n '/cleanup()/,/^}/p' "$ANDROID_GAME_DISCOVERY")"
if ! grep -F 'cmd appops set --uid '\''$PKG'\'' MANAGE_EXTERNAL_STORAGE '\''$PRIOR_UID_APPOP_MODE'\''' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'cmd appops set --uid '\''$PKG'\'' MANAGE_EXTERNAL_STORAGE '\''$requested'\''' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'Uid mode: MANAGE_EXTERNAL_STORAGE:' "$ANDROID_GAME_DISCOVERY" >/dev/null \
  || ! grep -F 'rm -rf '\''$FIXTURE_A'\'' '\''$FIXTURE_B'\'' '\''$FIXTURE_A_UNAVAILABLE'\'' '\''$FIXTURE_B_UNAVAILABLE'\''' <<<"$cleanup_block" >/dev/null; then
  echo 'android-game-discovery-check.sh must capture, set, verify, and restore the MANAGE_EXTERNAL_STORAGE UID app-op mode' >&2
  exit 1
fi

for resumed_activity_script in \
  "$ANDROID_APP_ROUTE" \
  "$ANDROID_GAME_DISCOVERY" \
  "$CRATE/journey-compare.sh" \
  "$JOURNEY_RESUME" \
  "$CRATE/journey-switch.sh" \
  "$CRATE/storage-notice-check.sh"; do
  if sed '/^[[:space:]]*#/d' "$resumed_activity_script" \
    | grep -E 'grep .*ResumedActivity' \
    | grep -Fv '(topResumedActivity|mResumedActivity)' >/dev/null; then
    echo "$(basename "$resumed_activity_script") must match only topResumedActivity/mResumedActivity, not broad ResumedActivity history" >&2
    exit 1
  fi
done

# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if sed '/^[[:space:]]*#/d' "$ANDROID_SMOKE" | grep -E 'push "\$CHECKPOINT_(CONFIG|LIBRARY)"' >/dev/null; then
  echo 'android-smoke.sh must not push checkpoint config.yaml/library.yaml in the general device smoke path' >&2
  exit 1
fi
if ! grep -F -- '--expect-installed-route' "$ANDROID_SMOKE" >/dev/null; then
  echo 'android-smoke.sh must keep installed-route assertions behind --expect-installed-route' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F -- '--expect-installed-route "$SERIAL"' "$ANDROID_APP_ROUTE" >/dev/null; then
  echo 'android-app-route-check.sh must opt in to installed-route smoke assertions explicitly' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F 'CHECKPOINT_LIBRARY="${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_LIBRARY:-$ROOT/docs/research/retroarch-plugin-route/library.yaml}"' "$ANDROID_APP_ROUTE" >/dev/null; then
  echo 'android-app-route-check.sh must expose an override for the checkpoint library while keeping the canonical default' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F 'CHECKPOINT_LIBRARY="${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_LIBRARY:-$ROOT/docs/research/retroarch-plugin-route/library.yaml}"' "$ANDROID_SMOKE" >/dev/null; then
  echo 'android-smoke.sh must byte-check the same overrideable checkpoint library as the dedicated installed-route gate' >&2
  exit 1
fi
if ! grep -F 'PRIOR_USER=' "$JOURNEY_RESUME" >/dev/null; then
  echo 'journey-resume.sh must capture user_rotation before pinning portrait' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F 'settings put system user_rotation ${PRIOR_USER:-0}' "$JOURNEY_RESUME" >/dev/null; then
  echo 'journey-resume.sh must restore user_rotation on exit' >&2
  exit 1
fi
if ! grep -F 'assert_portal_exposes_title' "$JOURNEY_RESUME" >/dev/null; then
  echo 'journey-resume.sh must verify the portal exposes the expected title before D-pad activation' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F 'KORRI_ACTIVITY="$KORRI/com.limelight.KorriShellActivity"' "$JOURNEY_RESUME" >/dev/null; then
  echo 'journey-resume.sh must target KorriShellActivity explicitly when foregrounding Korri' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F '"$JOURNEY_RESUME" "$SERIAL" "$GAME"' "$ANDROID_APP_ROUTE" >/dev/null; then
  echo 'android-app-route-check.sh must pass the configured Android app package into journey-resume.sh' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F 'adb_shell_capture "pm path $GAME"' "$ANDROID_APP_ROUTE" >/dev/null; then
  echo 'android-app-route-check.sh must run the package probe through the bounded adb helper' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F 'timeout 15 "$ADB_BIN" connect "$SERIAL"' "$JOURNEY_RESUME" >/dev/null; then
  echo 'journey-resume.sh must bound wireless adb connect attempts' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F 'pid_of() { adb_shell "pidof $GAME' "$JOURNEY_RESUME" >/dev/null; then
  echo 'journey-resume.sh must route pid_of through the bounded adb shell helper' >&2
  exit 1
fi
if sed '/^[[:space:]]*#/d' "$JOURNEY_RESUME" | grep -F 'monkey -p' >/dev/null; then
  echo 'journey-resume.sh must not use monkey launcher activation for Korri foregrounding' >&2
  exit 1
fi
foreground_health_check="$(sed -n '/health_response=/,/local_games_response=/p' "$ANDROID_APP_ROUTE")"
if ! grep -F '._tag == "system.health"' <<<"$foreground_health_check" >/dev/null; then
  echo 'android-app-route-check.sh must semantically assert foreground health top-level system.health tag' >&2
  exit 1
fi
if ! grep -F '.outcome._tag == "Ok"' <<<"$foreground_health_check" >/dev/null; then
  echo 'android-app-route-check.sh must semantically assert foreground health outcome Ok' >&2
  exit 1
fi
if ! grep -F '.outcome.payload.version | type == "string" and length > 0' <<<"$foreground_health_check" >/dev/null; then
  echo 'android-app-route-check.sh must semantically assert foreground health response version is a non-empty string' >&2
  exit 1
fi

TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT
CONNECTION_LOSS_PROBE="$TMP/connection-loss-probe"
printf '#!/usr/bin/env bash\nprintf "observed connection loss\\n"\n' >"$CONNECTION_LOSS_PROBE"
chmod +x "$CONNECTION_LOSS_PROBE"
export KORRI_STREAM_CONNECTION_LOSS_PROBE="$CONNECTION_LOSS_PROBE"
SEMANTIC_FUNCTION="$TMP/semantic-control-values.sh"
sed -n '/^semantic_control_values() {/,/^}/p' "$OVERLAY_ACCEPTANCE" >"$SEMANTIC_FUNCTION"
# shellcheck source=/dev/null
source "$SEMANTIC_FUNCTION"
semantic_fixture='{"outcome":{"payload":{"groups":[{"controls":[{"id":"command","interaction":{"kind":"command"}},{"id":"range","interaction":{"kind":"range","payload":{"value":7,"min":0,"max":10,"step":1}}},{"id":"toggle","interaction":{"kind":"toggle","payload":{"value":false,"trueLabel":"On","falseLabel":"Off"}}}]}]}}}'
semantic_values="$(semantic_control_values <<<"$semantic_fixture")"
jq -e '
  length == 2
  and .[0] == {id:"range",kind:"range",value:7}
  and .[1] == {id:"toggle",kind:"toggle",value:false}
' <<<"$semantic_values" >/dev/null || {
  echo 'overlay acceptance semantic control comparison lost typed values or included commands' >&2
  exit 1
}
ADB_LOG="$TMP/adb.log"
CHILD_LOG="$TMP/children.log"
FAKE_ADB="$TMP/adb"

OVERLAY_PREFLIGHT_LOG="$TMP/overlay-preflight-adb.log"
OVERLAY_PREFLIGHT_ADB="$TMP/overlay-preflight-adb"
cat >"$OVERLAY_PREFLIGHT_ADB" <<'OVERLAY_ADB'
#!/usr/bin/env bash
set -euo pipefail
printf '%q ' "$@" >>"$KORRI_DEVICE_SCRIPT_REVIEW_OVERLAY_ADB_LOG"
printf '\n' >>"$KORRI_DEVICE_SCRIPT_REVIEW_OVERLAY_ADB_LOG"
while [[ $# -gt 0 ]]; do
  case "$1" in
    -s)
      shift 2
      ;;
    *)
      break
      ;;
  esac
done
case "${1:-}" in
  wait-for-device)
    ;;
  get-state)
    printf 'device\n'
    ;;
  get-serialno)
    printf '%s\n' "${KORRI_DEVICE_SCRIPT_REVIEW_TARGET_SERIAL:-device-1}"
    ;;
  shell)
    shift
    if [[ "$*" == 'getprop ro.product.model' ]]; then
      printf '%s\n' "${KORRI_DEVICE_SCRIPT_REVIEW_TARGET_MODEL:-Wrong Model}"
    else
      echo "unexpected preflight shell command: $*" >&2
      exit 70
    fi
    ;;
  *)
    echo "unexpected preflight adb command: $*" >&2
    exit 71
    ;;
esac
OVERLAY_ADB
chmod +x "$OVERLAY_PREFLIGHT_ADB"
export KORRI_DEVICE_SCRIPT_REVIEW_OVERLAY_ADB_LOG="$OVERLAY_PREFLIGHT_LOG"

for invalid_case in \
  'KORRI_PACKAGE=bad-package' \
  'KORRI_RETROARCH_PACKAGE=bad-package'; do
  : >"$OVERLAY_PREFLIGHT_LOG"
  if env "$invalid_case" KORRI_ADB_BIN="$OVERLAY_PREFLIGHT_ADB" \
    bash "$OVERLAY_ACCEPTANCE" device-1 'Expected Model' \
      com.retroarch.aarch64 com.example.unrelated "$TMP/evidence" >/dev/null 2>&1; then
    echo "overlay acceptance accepted invalid package override: $invalid_case" >&2
    exit 1
  fi
  [[ ! -s "$OVERLAY_PREFLIGHT_LOG" ]] || {
    echo "overlay acceptance contacted adb before rejecting $invalid_case" >&2
    exit 1
  }
done
for invalid_package_args in \
  'bad-package com.example.unrelated' \
  'com.retroarch.aarch64 bad-package' \
  'com.example.other com.example.unrelated'; do
  read -r direct_package unrelated_package <<<"$invalid_package_args"
  : >"$OVERLAY_PREFLIGHT_LOG"
  if KORRI_ADB_BIN="$OVERLAY_PREFLIGHT_ADB" \
    bash "$OVERLAY_ACCEPTANCE" device-1 'Expected Model' \
      "$direct_package" "$unrelated_package" "$TMP/evidence" >/dev/null 2>&1; then
    echo "overlay acceptance accepted unsafe package arguments: $invalid_package_args" >&2
    exit 1
  fi
  [[ ! -s "$OVERLAY_PREFLIGHT_LOG" ]] || {
    echo "overlay acceptance contacted adb before rejecting: $invalid_package_args" >&2
    exit 1
  }
done

: >"$OVERLAY_PREFLIGHT_LOG"
KORRI_DEVICE_SCRIPT_REVIEW_TARGET_SERIAL=other-device \
  KORRI_ADB_BIN="$OVERLAY_PREFLIGHT_ADB" \
  bash "$OVERLAY_ACCEPTANCE" device-1 'Expected Model' \
    com.retroarch.aarch64 com.example.unrelated "$TMP/evidence" >/dev/null 2>&1 && {
      echo 'overlay acceptance accepted an adb serial mismatch' >&2
      exit 1
    }
if grep -F 'shell ' "$OVERLAY_PREFLIGHT_LOG" >/dev/null; then
  echo 'overlay acceptance used adb shell before exact serial verification' >&2
  exit 1
fi

: >"$OVERLAY_PREFLIGHT_LOG"
KORRI_DEVICE_SCRIPT_REVIEW_TARGET_MODEL='Wrong Model' \
  KORRI_ADB_BIN="$OVERLAY_PREFLIGHT_ADB" \
  bash "$OVERLAY_ACCEPTANCE" device-1 'Expected Model' \
    com.retroarch.aarch64 com.example.unrelated "$TMP/evidence" >/dev/null 2>&1 && {
      echo 'overlay acceptance accepted a device model mismatch' >&2
      exit 1
    }
if grep -E '(^|[[:space:]])(push|rm|mkdir|settings|am)([[:space:]]|$)' "$OVERLAY_PREFLIGHT_LOG" >/dev/null; then
  echo 'overlay acceptance mutated a wrong-model device' >&2
  exit 1
fi

PID_OF_FUNCTION="$TMP/journey-pid-of.sh"
grep -E '^pid_of\(\) \{' "$JOURNEY_RESUME" >"$PID_OF_FUNCTION"
if [[ "$(wc -l <"$PID_OF_FUNCTION")" -ne 1 ]]; then
  echo 'journey-resume.sh must keep exactly one pid_of function for deterministic review' >&2
  exit 1
fi
PIDOF_BIN="$TMP/pidof-bin"
PIDOF_ADB="$TMP/pidof-adb"
mkdir -p "$PIDOF_BIN"
cat >"$PIDOF_BIN/pidof" <<'PIDOF'
#!/usr/bin/env bash
set -euo pipefail
case "${KORRI_DEVICE_SCRIPT_REVIEW_PIDOF_MODE:-missing}" in
  missing)
    exit 1
    ;;
  present)
    printf '12345\r\n'
    ;;
  error)
    exit 2
    ;;
  *)
    exit 64
    ;;
esac
PIDOF
chmod +x "$PIDOF_BIN/pidof"
cat >"$PIDOF_ADB" <<'PIDOF_ADB'
#!/usr/bin/env bash
set -euo pipefail
while [[ $# -gt 0 ]]; do
  case "$1" in
    -s)
      shift 2
      ;;
    *)
      break
      ;;
  esac
done
subcommand="${1:-}"
if [[ $# -gt 0 ]]; then
  shift
fi
if [[ "$subcommand" != shell ]]; then
  exit 1
fi
PATH="$KORRI_DEVICE_SCRIPT_REVIEW_PIDOF_BIN:$PATH" bash -c "$*"
PIDOF_ADB
chmod +x "$PIDOF_ADB"
# shellcheck disable=SC2034 # Used by the sourced journey-resume.sh pid_of function.
GAME=com.playdigious.tmnt
adb_shell() {
  "$PIDOF_ADB" -s device-1 shell "$@"
}
export KORRI_DEVICE_SCRIPT_REVIEW_PIDOF_BIN="$PIDOF_BIN"
# shellcheck source=/dev/null
source "$PID_OF_FUNCTION"
export KORRI_DEVICE_SCRIPT_REVIEW_PIDOF_MODE=missing
if ! empty_pid="$(pid_of)"; then
  echo 'journey-resume.sh pid_of must treat pidof exit 1 as an empty process result' >&2
  exit 1
fi
if [[ -n "$empty_pid" ]]; then
  echo "journey-resume.sh pid_of returned output for an absent process: $empty_pid" >&2
  exit 1
fi
export KORRI_DEVICE_SCRIPT_REVIEW_PIDOF_MODE=present
if [[ "$(pid_of)" != 12345 ]]; then
  echo 'journey-resume.sh pid_of must trim CR/LF while returning a real pid' >&2
  exit 1
fi
export KORRI_DEVICE_SCRIPT_REVIEW_PIDOF_MODE=error
set +e
pid_of >"$TMP/pid-error.out" 2>"$TMP/pid-error.err"
pid_error_status=$?
set -e
if [[ "$pid_error_status" -eq 0 ]]; then
  echo 'journey-resume.sh pid_of must not mask non-empty pidof failures' >&2
  exit 1
fi

# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F 'MAGICK_BIN="${KORRI_MAGICK_BIN:-magick}"' "$JOURNEY_RESUME" >/dev/null; then
  echo 'journey-resume.sh must expose a magick binary override seam for deterministic review' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F 'TESSERACT_BIN="${KORRI_TESSERACT_BIN:-tesseract}"' "$JOURNEY_RESUME" >/dev/null; then
  echo 'journey-resume.sh must expose a tesseract binary override seam for deterministic review' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal grep needle; this reviews script text.
if ! grep -F 'ocr_shot "$label"' "$JOURNEY_RESUME" >/dev/null; then
  echo 'journey-resume.sh must OCR the portal screenshot before D-pad activation' >&2
  exit 1
fi
if ! sed -n '/android-app-route-check = {/,/^    };/p' "$ROOT/nix/tasks.nix" | grep -F 'pkgs.imagemagick' >/dev/null; then
  echo 'android-app-route-check task must put ImageMagick on PATH for the journey OCR gate' >&2
  exit 1
fi
if ! sed -n '/android-app-route-check = {/,/^    };/p' "$ROOT/nix/tasks.nix" | grep -F 'pkgs.tesseract' >/dev/null; then
  echo 'android-app-route-check task must put tesseract on PATH for the journey gate' >&2
  exit 1
fi
if ! sed -n '/journey-resume = deviceScript/,/^    };/p' "$ROOT/nix/tasks.nix" | grep -F 'pkgs.imagemagick' >/dev/null; then
  echo 'journey-resume task must put ImageMagick on PATH for the portal OCR gate' >&2
  exit 1
fi
if ! sed -n '/journey-resume = deviceScript/,/^    };/p' "$ROOT/nix/tasks.nix" | grep -F 'pkgs.tesseract' >/dev/null; then
  echo 'journey-resume task must put tesseract on PATH for the portal OCR gate' >&2
  exit 1
fi

JOURNEY_REVIEW_BIN="$TMP/journey-bin"
JOURNEY_REVIEW_ADB="$TMP/journey-adb"
JOURNEY_REVIEW_MAGICK="$TMP/journey-magick"
JOURNEY_REVIEW_TESSERACT="$TMP/journey-tesseract"
JOURNEY_REVIEW_SLEEP="$JOURNEY_REVIEW_BIN/sleep"
# Seed the stale-screenshot hazard under this review's temp dir; the source
# guard below keeps the deterministic review from reading the live external dir.
AMBIENT_CONVENTIONAL_SHOTS="$TMP/ambient/korri-journey"
AMBIENT_CONVENTIONAL_HOME="$AMBIENT_CONVENTIONAL_SHOTS/1-korri-home.png"
mkdir -p "$JOURNEY_REVIEW_BIN" "$AMBIENT_CONVENTIONAL_SHOTS"
printf 'stale black screenshot placeholder\n' >"$AMBIENT_CONVENTIONAL_HOME"
external_shots_root="/tmp/korri""-journey"
if sed '/^[[:space:]]*#/d' "${BASH_SOURCE[0]}" | grep -F "$external_shots_root" >/dev/null; then
  echo 'android-device-script-review.sh must not inspect the live journey screenshot directory' >&2
  exit 1
fi
cat >"$JOURNEY_REVIEW_SLEEP" <<'JOURNEY_SLEEP'
#!/usr/bin/env bash
set -euo pipefail
exit 0
JOURNEY_SLEEP
chmod +x "$JOURNEY_REVIEW_SLEEP"
cat >"$JOURNEY_REVIEW_MAGICK" <<'JOURNEY_MAGICK'
#!/usr/bin/env bash
set -euo pipefail
input="${1:?}"
shift
if [[ "$input" == "${KORRI_DEVICE_SCRIPT_REVIEW_AMBIENT_SCREENSHOT:-}" ]]; then
  echo 'deterministic review attempted deskew on an ambient journey screenshot' >&2
  exit 97
fi
printf '%s %s\n' "$input" "$*" >>"$KORRI_DEVICE_SCRIPT_REVIEW_MAGICK_LOG"
output="${@: -1}"
cp "$input" "$output"
JOURNEY_MAGICK
chmod +x "$JOURNEY_REVIEW_MAGICK"
cat >"$JOURNEY_REVIEW_TESSERACT" <<'JOURNEY_TESSERACT'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "${KORRI_DEVICE_SCRIPT_REVIEW_AMBIENT_SCREENSHOT:-}" ]]; then
  echo 'deterministic review attempted OCR on an ambient journey screenshot' >&2
  exit 97
fi
printf '%s\n' "$*" >>"$KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_LOG"
printf '%s\n' "${KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_TEXT:-}"
JOURNEY_TESSERACT
chmod +x "$JOURNEY_REVIEW_TESSERACT"
cat >"$JOURNEY_REVIEW_ADB" <<'JOURNEY_ADB'
#!/usr/bin/env bash
set -euo pipefail
while [[ $# -gt 0 ]]; do
  case "$1" in
    -s)
      shift 2
      ;;
    *)
      break
      ;;
  esac
done
subcommand="${1:-}"
if [[ $# -gt 0 ]]; then
  shift
fi
state_file="$KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_STATE"
case "$subcommand" in
  wait-for-device|connect)
    exit 0
    ;;
  pull)
    source_path="${1:?}"
    destination="${2:?}"
    mkdir -p "$(dirname "$destination")"
    case "$source_path" in
      /sdcard/j.png)
        printf 'review png\n' >"$destination"
        ;;
      /sdcard/j.xml)
        printf '<hierarchy><node class="android.webkit.WebView" /></hierarchy>\n' >"$destination"
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  shell)
    shell_command="$*"
    if [[ -n "${KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_ADB_LOG:-}" ]]; then
      printf 'shell:%s\n' "$shell_command" >>"$KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_ADB_LOG"
    fi
    case "$shell_command" in
      pm\ path*)
        printf 'package:/data/app/%s/base.apk\n' "${KORRI_DEVICE_SCRIPT_REVIEW_GAME:-review.game}"
        ;;
      settings\ get\ system*)
        printf '0\r\n'
        ;;
      settings\ put\ system*)
        ;;
      pidof\ *)
        if [[ "$(cat "$state_file" 2>/dev/null || true)" == game ]]; then
          printf '12345\r\n'
        fi
        ;;
      dumpsys\ activity\ activities*)
        resumed_activity_line() {
          local component="$1"
          case "${KORRI_DEVICE_SCRIPT_REVIEW_RESUMED_ACTIVITY_FORMAT:-modern}" in
            modern)
              printf 'topResumedActivity=ActivityRecord{1 u0 %s t1}\n' "$component"
              ;;
            android13)
              printf 'topResumedActivity=ActivityRecord{1 u0 %s} t10}\n' "$component"
              ;;
            android12)
              printf '  mResumedActivity: ActivityRecord{1 u0 %s t1}\n' "$component"
              ;;
            *)
              exit 64
              ;;
          esac
        }
        case "$(cat "$state_file" 2>/dev/null || true)" in
          game)
            activity_line="$(resumed_activity_line "${KORRI_DEVICE_SCRIPT_REVIEW_GAME:-review.game}/.MainActivity")"
            ;;
          home)
            activity_line="$(resumed_activity_line 'com.android.launcher/.Launcher')"
            ;;
          korri)
            activity_line="$(resumed_activity_line 'com.simonwjackson.korri.debug/com.limelight.KorriShellActivity')"
            ;;
          *)
            activity_line="$(resumed_activity_line 'com.android.launcher/.Launcher')"
            ;;
        esac
        if [[ "$shell_command" == *"grep -m1 topResumedActivity"* && "$activity_line" != *topResumedActivity* ]]; then
          exit 1
        fi
        printf '%s\n' "$activity_line"
        ;;
      screencap\ -p\ /sdcard/j.png)
        ;;
      uiautomator\ dump\ /sdcard/j.xml)
        ;;
      am\ start\ -n\ com.simonwjackson.korri.debug/com.limelight.KorriShellActivity)
        count_file="${KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_COUNT:?}"
        count="$(cat "$count_file" 2>/dev/null || printf '0')"
        count="$((count + 1))"
        printf '%s\n' "$count" >"$count_file"
        case "${KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_MODE:-retry}" in
          retry)
            if ((count % 2 == 0)); then
              printf 'korri\n' >"$state_file"
            fi
            ;;
          fail-once)
            if ((count % 2 == 1)); then
              printf 'review am start failure\n' >&2
              exit 23
            fi
            printf 'korri\n' >"$state_file"
            ;;
          always-fail)
            printf 'review am start failure\n' >&2
            exit 23
            ;;
          never)
            ;;
          *)
            exit 64
            ;;
        esac
        ;;
      wm\ dismiss-keyguard)
        ;;
      input\ keyevent\ KEYCODE_DPAD_CENTER)
        printf 'game\n' >"$state_file"
        ;;
      input\ keyevent\ KEYCODE_HOME)
        printf 'home\n' >"$state_file"
        ;;
      am\ force-stop*)
        printf 'stopped\n' >"$state_file"
        ;;
      *)
        ;;
    esac
    ;;
  *)
    exit 0
    ;;
esac
JOURNEY_ADB
chmod +x "$JOURNEY_REVIEW_ADB"

assert_journey_wake_dismiss_precede_explicit_start() {
  local log="$1"
  awk '
    /^shell:input keyevent KEYCODE_WAKEUP$/ {
      saw_wake = NR
      open_has_start = 0
    }
    /^shell:wm dismiss-keyguard$/ { saw_dismiss = NR }
    /^shell:am start -n com\.simonwjackson\.korri\.debug\/com\.limelight\.KorriShellActivity$/ {
      if (!(saw_wake && saw_dismiss && saw_wake < saw_dismiss && saw_dismiss < NR)) {
        failed = 1
        printf "journey-resume.sh explicitly started Korri before wake/dismiss (line %d)\n", NR > "/dev/stderr"
        exit 1
      }
      starts += 1
      if (!open_has_start) {
        opens += 1
        open_has_start = 1
      }
    }
    index($0, "shell:dumpsys activity activities 2>/dev/null | grep -m1 -E ") == 1 && index($0, "topResumedActivity|mResumedActivity") {
      if (open_has_start) {
        top_polls_after_start += 1
      }
    }
    /^shell:input keyevent KEYCODE_DPAD_CENTER$/ || /^shell:input keyevent KEYCODE_HOME$/ {
      saw_wake = 0
      saw_dismiss = 0
      open_has_start = 0
    }
    END {
      if (!failed && opens < 2) {
        printf "journey-resume.sh review saw %d Korri open phases, expected at least 2\n", opens > "/dev/stderr"
        exit 1
      }
      if (!failed && starts < 4) {
        printf "journey-resume.sh review saw %d explicit Korri starts, expected retry evidence\n", starts > "/dev/stderr"
        exit 1
      }
      if (!failed && top_polls_after_start < starts) {
        printf "journey-resume.sh review saw %d top polls after %d explicit starts\n", top_polls_after_start, starts > "/dev/stderr"
        exit 1
      }
    }
  ' "$log"
}

assert_journey_tmnt_launch_navigation() {
  local log="$1"
  local expected_downs="$2"
  local label="$3"
  awk -v expected_downs="$expected_downs" -v label="$label" '
    /^shell:input keyevent KEYCODE_DPAD_UP$/ {
      ups += 1
      next
    }
    /^shell:input keyevent KEYCODE_DPAD_DOWN$/ {
      downs += 1
      next
    }
    /^shell:input keyevent KEYCODE_DPAD_CENTER$/ {
      launches += 1
      if (ups != 12) {
        printf "%s launch %d reset with %d DPAD_UP events, expected 12\n", label, launches, ups > "/dev/stderr"
        exit 1
      }
      if (downs != expected_downs) {
        printf "%s launch %d used %d DPAD_DOWN events, expected %d\n", label, launches, downs, expected_downs > "/dev/stderr"
        exit 1
      }
      ups = 0
      downs = 0
      next
    }
    END {
      if (launches != 2) {
        printf "%s review saw %d TMNT launch confirmations, expected 2\n", label, launches > "/dev/stderr"
        exit 1
      }
    }
  ' "$log"
}

review_title='Review OCR Title'
now_playing_marker='RESUMES'
review_game='review.android.game'
review_shots="$TMP/journey-success"
review_state="$TMP/journey-success.state"
review_magick_log="$TMP/journey-success-magick.log"
review_tesseract_log="$TMP/journey-success-tesseract.log"
review_adb_log="$TMP/journey-success-adb.log"
review_start_count="$TMP/journey-success-start-count"
printf 'korri\n' >"$review_state"
printf '0\n' >"$review_start_count"
review_ocr_with_banner="tmnt
shredder
revenge
${now_playing_marker}"
PATH="$JOURNEY_REVIEW_BIN:$PATH" \
KORRI_ADB_BIN="$JOURNEY_REVIEW_ADB" \
KORRI_MAGICK_BIN="$JOURNEY_REVIEW_MAGICK" \
KORRI_TESSERACT_BIN="$JOURNEY_REVIEW_TESSERACT" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_STATE="$review_state" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_COUNT="$review_start_count" \
KORRI_DEVICE_SCRIPT_REVIEW_MAGICK_LOG="$review_magick_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_LOG="$review_tesseract_log" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_ADB_LOG="$review_adb_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_TEXT="$review_ocr_with_banner" \
KORRI_DEVICE_SCRIPT_REVIEW_AMBIENT_SCREENSHOT="$AMBIENT_CONVENTIONAL_HOME" \
KORRI_DEVICE_SCRIPT_REVIEW_GAME="$review_game" \
SHOTS="$review_shots" \
  "$JOURNEY_RESUME" device-1 "$review_game" >"$TMP/journey-success.out" 2>"$TMP/journey-success.err" || {
    cat "$TMP/journey-success.out" >&2
    cat "$TMP/journey-success.err" >&2
    exit 1
  }
assert_journey_wake_dismiss_precede_explicit_start "$review_adb_log"
assert_journey_tmnt_launch_navigation "$review_adb_log" 1 'journey-resume.sh active-session banner'
if ! grep -F -- "$review_shots/1-korri-home.png -deskew 40% $review_shots/1-korri-home.ocr.png" "$review_magick_log" >/dev/null; then
  echo 'journey-resume.sh did not deskew the captured portal screenshot before OCR' >&2
  exit 1
fi
if ! grep -F -- "$review_shots/1-korri-home.ocr.png stdout --psm 6" "$review_tesseract_log" >/dev/null; then
  echo 'journey-resume.sh did not OCR the deskewed portal screenshot with fixed page segmentation' >&2
  exit 1
fi
for token in tmnt shredder revenge; do
  if ! grep -Fxi -- "$token" "$review_shots/1-korri-home.ocr.txt" >/dev/null; then
    echo "journey-resume.sh did not save portal OCR token beside screenshot evidence: $token" >&2
    exit 1
  fi
done
if ! test -f "$review_shots/1-korri-home.ocr.png"; then
  echo 'journey-resume.sh did not keep deskewed OCR image evidence' >&2
  exit 1
fi
if ! test -f "$review_shots/1-korri-home.xml"; then
  echo 'journey-resume.sh did not keep UIAutomator XML evidence while using OCR for assertion' >&2
  exit 1
fi

review_shots="$TMP/journey-android13-resumed-component"
review_state="$TMP/journey-android13-resumed-component.state"
review_magick_log="$TMP/journey-android13-resumed-component-magick.log"
review_tesseract_log="$TMP/journey-android13-resumed-component-tesseract.log"
review_adb_log="$TMP/journey-android13-resumed-component-adb.log"
review_start_count="$TMP/journey-android13-resumed-component-start-count"
printf 'korri\n' >"$review_state"
printf '0\n' >"$review_start_count"
PATH="$JOURNEY_REVIEW_BIN:$PATH" \
KORRI_ADB_BIN="$JOURNEY_REVIEW_ADB" \
KORRI_MAGICK_BIN="$JOURNEY_REVIEW_MAGICK" \
KORRI_TESSERACT_BIN="$JOURNEY_REVIEW_TESSERACT" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_STATE="$review_state" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_COUNT="$review_start_count" \
KORRI_DEVICE_SCRIPT_REVIEW_RESUMED_ACTIVITY_FORMAT=android13 \
KORRI_DEVICE_SCRIPT_REVIEW_MAGICK_LOG="$review_magick_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_LOG="$review_tesseract_log" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_ADB_LOG="$review_adb_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_TEXT="$review_ocr_with_banner" \
KORRI_DEVICE_SCRIPT_REVIEW_AMBIENT_SCREENSHOT="$AMBIENT_CONVENTIONAL_HOME" \
KORRI_DEVICE_SCRIPT_REVIEW_GAME="$review_game" \
SHOTS="$review_shots" \
  "$JOURNEY_RESUME" device-1 "$review_game" >"$TMP/journey-android13-resumed-component.out" 2>"$TMP/journey-android13-resumed-component.err" || {
    cat "$TMP/journey-android13-resumed-component.out" >&2
    cat "$TMP/journey-android13-resumed-component.err" >&2
    exit 1
  }
if grep -F 'top=com.simonwjackson.korri.debug/com.limelight.KorriShellActivity}' "$TMP/journey-android13-resumed-component.out" >/dev/null; then
  echo 'journey-resume.sh left a trailing Android 13 activity-record brace on the parsed Korri component' >&2
  exit 1
fi
if grep -F "top=$review_game/.MainActivity}" "$TMP/journey-android13-resumed-component.out" >/dev/null; then
  echo 'journey-resume.sh left a trailing Android 13 activity-record brace on the parsed game component' >&2
  exit 1
fi

review_shots="$TMP/journey-start-failure-retry"
review_state="$TMP/journey-start-failure-retry.state"
review_magick_log="$TMP/journey-start-failure-retry-magick.log"
review_tesseract_log="$TMP/journey-start-failure-retry-tesseract.log"
review_adb_log="$TMP/journey-start-failure-retry-adb.log"
review_start_count="$TMP/journey-start-failure-retry-start-count"
printf 'korri\n' >"$review_state"
printf '0\n' >"$review_start_count"
PATH="$JOURNEY_REVIEW_BIN:$PATH" \
KORRI_ADB_BIN="$JOURNEY_REVIEW_ADB" \
KORRI_MAGICK_BIN="$JOURNEY_REVIEW_MAGICK" \
KORRI_TESSERACT_BIN="$JOURNEY_REVIEW_TESSERACT" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_STATE="$review_state" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_COUNT="$review_start_count" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_MODE=fail-once \
KORRI_DEVICE_SCRIPT_REVIEW_MAGICK_LOG="$review_magick_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_LOG="$review_tesseract_log" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_ADB_LOG="$review_adb_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_TEXT="$review_ocr_with_banner" \
KORRI_DEVICE_SCRIPT_REVIEW_AMBIENT_SCREENSHOT="$AMBIENT_CONVENTIONAL_HOME" \
KORRI_DEVICE_SCRIPT_REVIEW_GAME="$review_game" \
SHOTS="$review_shots" \
  "$JOURNEY_RESUME" device-1 "$review_game" >"$TMP/journey-start-failure-retry.out" 2>"$TMP/journey-start-failure-retry.err"
assert_journey_wake_dismiss_precede_explicit_start "$review_adb_log"
if [[ "$(cat "$review_start_count")" -lt 4 ]]; then
  echo 'journey-resume.sh did not retry after nonzero explicit am start failures' >&2
  exit 1
fi

review_shots="$TMP/journey-no-banner"
review_state="$TMP/journey-no-banner.state"
review_magick_log="$TMP/journey-no-banner-magick.log"
review_tesseract_log="$TMP/journey-no-banner-tesseract.log"
review_adb_log="$TMP/journey-no-banner-adb.log"
review_start_count="$TMP/journey-no-banner-start-count"
printf 'korri\n' >"$review_state"
printf '0\n' >"$review_start_count"
PATH="$JOURNEY_REVIEW_BIN:$PATH" \
KORRI_ADB_BIN="$JOURNEY_REVIEW_ADB" \
KORRI_MAGICK_BIN="$JOURNEY_REVIEW_MAGICK" \
KORRI_TESSERACT_BIN="$JOURNEY_REVIEW_TESSERACT" \
KORRI_JOURNEY_EXPECTED_TITLE="$review_title" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_STATE="$review_state" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_COUNT="$review_start_count" \
KORRI_DEVICE_SCRIPT_REVIEW_RESUMED_ACTIVITY_FORMAT=android12 \
KORRI_DEVICE_SCRIPT_REVIEW_MAGICK_LOG="$review_magick_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_LOG="$review_tesseract_log" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_ADB_LOG="$review_adb_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_TEXT="review ocr title" \
KORRI_DEVICE_SCRIPT_REVIEW_AMBIENT_SCREENSHOT="$AMBIENT_CONVENTIONAL_HOME" \
KORRI_DEVICE_SCRIPT_REVIEW_GAME="$review_game" \
SHOTS="$review_shots" \
  "$JOURNEY_RESUME" device-1 "$review_game" >"$TMP/journey-no-banner.out" 2>"$TMP/journey-no-banner.err"
assert_journey_tmnt_launch_navigation "$review_adb_log" 0 'journey-resume.sh no active-session banner'

review_shots="$TMP/journey-foreground-timeout"
review_state="$TMP/journey-foreground-timeout.state"
review_magick_log="$TMP/journey-foreground-timeout-magick.log"
review_tesseract_log="$TMP/journey-foreground-timeout-tesseract.log"
review_start_count="$TMP/journey-foreground-timeout-start-count"
printf 'korri\n' >"$review_state"
printf '0\n' >"$review_start_count"
set +e
PATH="$JOURNEY_REVIEW_BIN:$PATH" \
KORRI_ADB_BIN="$JOURNEY_REVIEW_ADB" \
KORRI_MAGICK_BIN="$JOURNEY_REVIEW_MAGICK" \
KORRI_TESSERACT_BIN="$JOURNEY_REVIEW_TESSERACT" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_STATE="$review_state" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_COUNT="$review_start_count" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_MODE=always-fail \
KORRI_DEVICE_SCRIPT_REVIEW_MAGICK_LOG="$review_magick_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_LOG="$review_tesseract_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_TEXT="$review_ocr_with_banner" \
KORRI_DEVICE_SCRIPT_REVIEW_AMBIENT_SCREENSHOT="$AMBIENT_CONVENTIONAL_HOME" \
KORRI_DEVICE_SCRIPT_REVIEW_GAME="$review_game" \
SHOTS="$review_shots" \
  "$JOURNEY_RESUME" device-1 "$review_game" >"$TMP/journey-foreground-timeout.out" 2>"$TMP/journey-foreground-timeout.err"
journey_foreground_timeout_status=$?
set -e
if [[ "$journey_foreground_timeout_status" -eq 0 ]]; then
  echo 'journey-resume.sh accepted a Korri foreground timeout' >&2
  exit 1
fi
journey_foreground_timeout_evidence="$TMP/journey-foreground-timeout.evidence"
cat "$TMP/journey-foreground-timeout.out" "$TMP/journey-foreground-timeout.err" >"$journey_foreground_timeout_evidence"
if ! grep -F 'FAILED: 1-korri-home did not bring Korri activity to foreground' "$journey_foreground_timeout_evidence" >/dev/null; then
  echo 'journey-resume.sh foreground timeout did not report the failed open label' >&2
  exit 1
fi
if ! grep -F 'top=com.android.launcher/.Launcher' "$journey_foreground_timeout_evidence" >/dev/null; then
  echo 'journey-resume.sh foreground timeout did not preserve top activity evidence' >&2
  exit 1
fi
if ! grep -F 'am_start_status=23' "$journey_foreground_timeout_evidence" >/dev/null; then
  echo 'journey-resume.sh foreground timeout did not report the nonzero am start status' >&2
  exit 1
fi
if ! grep -F 'am start output: review am start failure' "$journey_foreground_timeout_evidence" >/dev/null; then
  echo 'journey-resume.sh foreground timeout did not report the nonzero am start output' >&2
  exit 1
fi
if [[ "$(cat "$review_start_count")" -ne 4 ]]; then
  echo 'journey-resume.sh did not exhaust bounded retries after nonzero am start failures' >&2
  exit 1
fi
if ! grep -F -- "$review_shots/1-korri-home.png" "$journey_foreground_timeout_evidence" >/dev/null; then
  echo 'journey-resume.sh foreground timeout did not print screenshot evidence path' >&2
  exit 1
fi
if ! test -f "$review_shots/1-korri-home.png"; then
  echo 'journey-resume.sh foreground timeout did not capture screenshot evidence' >&2
  exit 1
fi

review_shots="$TMP/journey-failure"
review_state="$TMP/journey-failure.state"
review_magick_log="$TMP/journey-failure-magick.log"
review_tesseract_log="$TMP/journey-failure-tesseract.log"
review_start_count="$TMP/journey-failure-start-count"
printf 'korri\n' >"$review_state"
printf '0\n' >"$review_start_count"
set +e
PATH="$JOURNEY_REVIEW_BIN:$PATH" \
KORRI_ADB_BIN="$JOURNEY_REVIEW_ADB" \
KORRI_MAGICK_BIN="$JOURNEY_REVIEW_MAGICK" \
KORRI_TESSERACT_BIN="$JOURNEY_REVIEW_TESSERACT" \
KORRI_JOURNEY_EXPECTED_TITLE="$review_title" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_STATE="$review_state" \
KORRI_DEVICE_SCRIPT_REVIEW_JOURNEY_START_COUNT="$review_start_count" \
KORRI_DEVICE_SCRIPT_REVIEW_MAGICK_LOG="$review_magick_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_LOG="$review_tesseract_log" \
KORRI_DEVICE_SCRIPT_REVIEW_TESSERACT_TEXT='different review text' \
KORRI_DEVICE_SCRIPT_REVIEW_AMBIENT_SCREENSHOT="$AMBIENT_CONVENTIONAL_HOME" \
KORRI_DEVICE_SCRIPT_REVIEW_GAME="$review_game" \
SHOTS="$review_shots" \
  "$JOURNEY_RESUME" device-1 "$review_game" >"$TMP/journey-failure.out" 2>"$TMP/journey-failure.err"
journey_failure_status=$?
set -e
if [[ "$journey_failure_status" -eq 0 ]]; then
  echo 'journey-resume.sh accepted a portal screenshot OCR result without the expected title' >&2
  exit 1
fi
journey_failure_evidence="$TMP/journey-failure.evidence"
cat "$TMP/journey-failure.out" "$TMP/journey-failure.err" >"$journey_failure_evidence"
for evidence_path in \
  "$review_shots/1-korri-home.png" \
  "$review_shots/1-korri-home.xml" \
  "$review_shots/1-korri-home.ocr.png" \
  "$review_shots/1-korri-home.ocr.txt"; do
  if ! grep -F -- "$evidence_path" "$journey_failure_evidence" >/dev/null; then
    echo "journey-resume.sh failure did not print evidence path: $evidence_path" >&2
    exit 1
  fi
done

if grep -F -- "$AMBIENT_CONVENTIONAL_HOME" "$TMP"/journey-*-magick.log "$TMP"/journey-*-tesseract.log >/dev/null; then
  echo 'android-device-script-review.sh used an ambient journey screenshot instead of fresh review artifacts' >&2
  exit 1
fi

# shellcheck source=/dev/null
KORRI_ANDROID_SMOKE_LIBRARY=true source "$ANDROID_SMOKE"

ALT_ANDROID_APP_RESPONSE="$(jq -n --arg package 'review.android.game' '{
  _tag: "app.local-games.launch",
  outcome: {
    _tag: "Ok",
    payload: {
      launcherId: "android-app",
      component: {
        packageName: $package,
        className: ""
      },
      extras: {},
      directories: [],
      files: [],
      integrity: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    }
  }
}')"
CANONICAL_ANDROID_APP_RESPONSE="$(jq -n --arg package 'com.playdigious.tmnt' '{
  _tag: "app.local-games.launch",
  outcome: {
    _tag: "Ok",
    payload: {
      launcherId: "android-app",
      component: {
        packageName: $package,
        className: ""
      },
      extras: {},
      directories: [],
      files: [],
      integrity: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    }
  }
}')"
if ! KORRI_ANDROID_APP_PACKAGE=review.android.game require_android_app_launch_response "$ALT_ANDROID_APP_RESPONSE"; then
  echo 'android-smoke.sh rejected the configured alternate Android app package in the protected launch response' >&2
  exit 1
fi
set +e
KORRI_ANDROID_APP_PACKAGE=review.android.game require_android_app_launch_response "$CANONICAL_ANDROID_APP_RESPONSE" >"$TMP/android-app-canonical-package.out" 2>"$TMP/android-app-canonical-package.err"
canonical_package_status=$?
set -e
if [[ "$canonical_package_status" -eq 0 ]]; then
  echo 'android-smoke.sh accepted the canonical package when an alternate Android app package was configured' >&2
  exit 1
fi

assert_executed_library_reaches_usage() {
  local label="$1"
  shift
  local out="$TMP/$label.out"
  local err="$TMP/$label.err"
  local status

  set +e
  env -u KORRI_ANDROID_DEVICE -u ANDROID_SERIAL KORRI_ANDROID_SMOKE_LIBRARY=true "$@" >"$out" 2>"$err"
  status=$?
  set -e

  if [[ "$status" -eq 0 ]]; then
    echo 'android-smoke.sh returned early when library mode was set during execution' >&2
    exit 1
  fi
  if ! grep -F 'usage: android-smoke.sh' "$err" >/dev/null; then
    echo 'android-smoke.sh executed library-mode failure did not reach the normal usage guard' >&2
    exit 1
  fi
}

assert_executed_library_reaches_usage executed-library bash "$ANDROID_SMOKE"
KORRI_ANDROID_DEVICE=review-inherited-device ANDROID_SERIAL=review-inherited-serial \
  assert_executed_library_reaches_usage executed-library-inherited-device bash "$ANDROID_SMOKE"

ADB_RESOLVE_LOG="$TMP/adb-resolve.log"
adb() {
  printf '%s\n' "$*" >>"$ADB_RESOLVE_LOG"
  if [[ "$*" == "-s device-1 shell mkdir -p '/sdcard/korri'" ]]; then
    return 0
  fi
  if [[ "$*" == "-s device-1 shell cd '/sdcard/korri' && pwd -P" ]]; then
    printf '/storage/emulated/0/korri\r\n'
    return 0
  fi
  return 1
}
ANDROID_STORAGE_ROOT="/sdcard/korri"
resolve_android_storage_root device-1 "/sdcard/korri"
if [[ "$ANDROID_STORAGE_ROOT" != "/storage/emulated/0/korri" ]]; then
  echo "android-smoke.sh did not canonicalize the Android storage root: $ANDROID_STORAGE_ROOT" >&2
  exit 1
fi
if ! grep -F -- "-s device-1 shell cd '/sdcard/korri' && pwd -P" "$ADB_RESOLVE_LOG" >/dev/null; then
  echo 'android-smoke.sh did not resolve the storage root through adb shell pwd -P' >&2
  exit 1
fi

SIGNED_WL4_RESPONSE="$(jq -n --arg root '/storage/emulated/0/korri' '{
  _tag: "app.local-games.launch",
  outcome: {
    _tag: "Ok",
    payload: {
      launchId: "0123456789abcdef0123456789abcdef",
      launcherId: "retroarch",
      disposition: "fresh",
      context: {
        gameId: "wl4",
        title: "Wario Land 4",
        contributors: [
          {kind: "launcher", id: "@korri:retroarch/retroarch"},
          {kind: "runtime", id: "@korri:mgba/mgba"}
        ],
        executor: {id: "retroarch-control", available: true},
        foreground: {
          kind: "component",
          packageName: "com.korri.retroarch",
          className: "com.retroarch.browser.retroactivity.RetroActivityFuture"
        }
      },
      component: {
        packageName: "com.korri.retroarch",
        className: "com.retroarch.browser.retroactivity.RetroActivityFuture"
      },
      extras: {
        ROM: ($root + "/roms/wl4.gba"),
        LIBRETRO: "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so",
        CONFIGFILE: ($root + "/retroarch.cfg")
      },
      directories: (["system", "saves", "states", "screenshots"] | map($root + "/" + .)),
      files: [{
        path: ($root + "/retroarch.cfg"),
        content: "video_driver = \"gl\"\nkiosk_mode_enable = \"true\"\nnetwork_cmd_enable = \"true\"\nnetwork_cmd_port = \"50000\""
      }],
      integrity: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }
  }
}')"
MISSING_WL4_RESPONSE="$(jq -n --arg root '/storage/emulated/0/korri' '{
  _tag: "app.local-games.launch",
  outcome: {
    _tag: "Err",
    payload: {
      code: "LocalRomMissing",
      message: ("local ROM is missing: " + $root + "/roms/wl4.gba")
    }
  }
}')"
ALIAS_WL4_RESPONSE="$(jq -n --arg root '/sdcard/korri' '{
  _tag: "app.local-games.launch",
  outcome: {
    _tag: "Err",
    payload: {
      code: "LocalRomMissing",
      message: ("local ROM is missing: " + $root + "/roms/wl4.gba")
    }
  }
}')"
BAD_WL4_RESPONSE="$(jq -n '{
  _tag: "app.local-games.launch",
  outcome: {
    _tag: "Err",
    payload: {
      code: "LocalRomMissing",
      message: "local ROM is missing: /tmp/host-root/roms/wl4.gba"
    }
  }
}')"
EXTRA_HOST_PATH_RESPONSE="$(jq '.outcome.payload.extras.HOST_PATH = "/tmp/host-root/roms/wl4.gba"' <<<"$SIGNED_WL4_RESPONSE")"
EXTRA_ERR_PATH_RESPONSE="$(jq '.outcome.payload.hostPath = "/tmp/host-root/roms/wl4.gba"' <<<"$MISSING_WL4_RESPONSE")"
if ! require_wl4_local_launch_response "$SIGNED_WL4_RESPONSE"; then
  echo 'android-smoke.sh rejected the signed deferred WL4 RetroArch launch branch' >&2
  exit 1
fi
if ! require_wl4_local_launch_response "$MISSING_WL4_RESPONSE"; then
  echo 'android-smoke.sh rejected the stable WL4 LocalRomMissing branch' >&2
  exit 1
fi
set +e
require_wl4_local_launch_response "$ALIAS_WL4_RESPONSE" >"$TMP/alias-wl4.out" 2>"$TMP/alias-wl4.err"
alias_wl4_status=$?
require_wl4_local_launch_response "$BAD_WL4_RESPONSE" >"$TMP/bad-wl4.out" 2>"$TMP/bad-wl4.err"
bad_wl4_status=$?
require_wl4_local_launch_response "$EXTRA_HOST_PATH_RESPONSE" >"$TMP/extra-host-path.out" 2>"$TMP/extra-host-path.err"
extra_host_path_status=$?
require_wl4_local_launch_response "$EXTRA_ERR_PATH_RESPONSE" >"$TMP/extra-err-path.out" 2>"$TMP/extra-err-path.err"
extra_err_path_status=$?
set -e
if [[ "$alias_wl4_status" -eq 0 ]]; then
  echo 'android-smoke.sh accepted the /sdcard alias after canonical root resolution' >&2
  exit 1
fi
if [[ "$bad_wl4_status" -eq 0 ]]; then
  echo 'android-smoke.sh accepted a WL4 missing-ROM error with an unsanitized path' >&2
  exit 1
fi
if [[ "$extra_host_path_status" -eq 0 ]]; then
  echo 'android-smoke.sh accepted a signed WL4 response with an injected extras.HOST_PATH' >&2
  exit 1
fi
if [[ "$extra_err_path_status" -eq 0 ]]; then
  echo 'android-smoke.sh accepted a WL4 missing-ROM error with an injected path field' >&2
  exit 1
fi

cat >"$FAKE_ADB" <<'FAKE_ADB'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$KORRI_DEVICE_SCRIPT_REVIEW_ADB_LOG"
while [[ $# -gt 0 ]]; do
  case "$1" in
    -s)
      shift 2
      ;;
    *)
      break
      ;;
  esac
done
subcommand="${1:-}"
if [[ $# -gt 0 ]]; then
  shift
fi
case "$subcommand" in
  connect|wait-for-device|push|forward)
    exit 0
    ;;
  shell)
    shell_command="$*"
    if [[ "$shell_command" == *"mkdir '/sdcard/korri/.android-app-route-check.lock'"* ]]; then
      if [[ "${KORRI_DEVICE_SCRIPT_REVIEW_ROUTE_LOCK_HELD:-false}" == true ]]; then
        echo 'Android app route check lock is held at /sdcard/korri/.android-app-route-check.lock. If this is stale, remove it manually only after verifying no route check is running.' >&2
        exit 75
      fi
    fi
    if [[ "${KORRI_DEVICE_SCRIPT_REVIEW_CLEANUP_FAIL:-}" == restore && "$shell_command" == *"cp '/sdcard/korri/.android-app-route-check-backup-"*"/config.yaml' '/sdcard/korri/config.yaml'"* ]]; then
      echo 'fake adb: restore config failed' >&2
      exit 66
    fi
    if [[ "${KORRI_DEVICE_SCRIPT_REVIEW_CLEANUP_FAIL:-}" == unlock && "$shell_command" == "rm -rf '/sdcard/korri/.android-app-route-check.lock'" ]]; then
      echo 'fake adb: unlock failed' >&2
      exit 67
    fi
    case "$shell_command" in
      pm\ path*)
        package="${shell_command#pm path }"
        printf 'package:/data/app/%s/base.apk\n' "$package"
        ;;
      "test -e '/sdcard/korri/config.yaml'")
        exit 0
        ;;
      "test -e '/sdcard/korri/library.yaml'")
        exit 1
        ;;
      dumpsys\ activity\ activities*)
        case "${KORRI_DEVICE_SCRIPT_REVIEW_RESUMED_ACTIVITY_FORMAT:-modern}" in
          modern)
            activity_line="topResumedActivity=ActivityRecord{1 u0 ${KORRI_ANDROID_APP_PACKAGE:-com.playdigious.tmnt}/.MainActivity t1}"
            ;;
          android13)
            activity_line="topResumedActivity=ActivityRecord{1 u0 ${KORRI_ANDROID_APP_PACKAGE:-com.playdigious.tmnt}/.MainActivity} t10}"
            ;;
          android12)
            activity_line="  mResumedActivity: ActivityRecord{1 u0 ${KORRI_ANDROID_APP_PACKAGE:-com.playdigious.tmnt}/.MainActivity t1}"
            ;;
          *)
            exit 64
            ;;
        esac
        if [[ "$shell_command" == *"grep -m1 topResumedActivity"* && "$activity_line" != *topResumedActivity* ]]; then
          exit 1
        fi
        printf '%s\n' "$activity_line"
        ;;
      pidof\ *)
        printf '12345\r\n'
        ;;
    esac
    exit 0
    ;;
  exec-out)
    if [[ "${1:-}" == cat && "${2:-}" == /sdcard/korri/config.yaml ]]; then
      cat "$KORRI_ROOT/docs/research/retroarch-plugin-route/config.yaml"
      exit 0
    fi
    if [[ "${1:-}" == cat && "${2:-}" == /sdcard/korri/library.yaml ]]; then
      cat "${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_LIBRARY:-$KORRI_ROOT/docs/research/retroarch-plugin-route/library.yaml}"
      exit 0
    fi
    exit 0
    ;;
  logcat)
    printf '08-01 00:00:00.000 I/KorridServer: listening on 127.0.0.1:43210\n'
    ;;
  *)
    exit 0
    ;;
esac
FAKE_ADB
chmod +x "$FAKE_ADB"

SMOKE="$TMP/smoke.sh"
cat >"$SMOKE" <<'SMOKE'
#!/usr/bin/env bash
set -euo pipefail
printf 'smoke:%s package=%s library=%s retro=%s\n' "$*" "${KORRI_ANDROID_APP_PACKAGE:-}" "${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_LIBRARY:-}" "${KORRI_EXPECT_RETROARCH_ROUTE:-}" >>"$KORRI_DEVICE_SCRIPT_REVIEW_CHILD_LOG"
exit 42
SMOKE
chmod +x "$SMOKE"

JOURNEY="$TMP/journey.sh"
cat >"$JOURNEY" <<'JOURNEY'
#!/usr/bin/env bash
set -euo pipefail
printf 'journey:%s\n' "$*" >>"$KORRI_DEVICE_SCRIPT_REVIEW_CHILD_LOG"
JOURNEY
chmod +x "$JOURNEY"

SMOKE_SUCCESS="$TMP/smoke-success.sh"
cat >"$SMOKE_SUCCESS" <<'SMOKE_SUCCESS'
#!/usr/bin/env bash
set -euo pipefail
printf 'smoke-success:%s package=%s library=%s\n' "$*" "${KORRI_ANDROID_APP_PACKAGE:-}" "${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_LIBRARY:-}" >>"$KORRI_DEVICE_SCRIPT_REVIEW_CHILD_LOG"
SMOKE_SUCCESS
chmod +x "$SMOKE_SUCCESS"

ROUTE_REVIEW_BIN="$TMP/route-bin"
mkdir -p "$ROUTE_REVIEW_BIN"
cat >"$ROUTE_REVIEW_BIN/curl" <<'ROUTE_CURL'
#!/usr/bin/env bash
set -euo pipefail
request="$*"
case "$request" in
  *system.health*)
    printf '{"_tag":"system.health","outcome":{"_tag":"Ok","payload":{"version":"review"}}}\n'
    ;;
  *app.local-games.list*)
    printf '{"_tag":"app.local-games.list","outcome":{"_tag":"Ok","payload":{"games":[{"id":"tmnt-shredders-revenge"},{"id":"wl4"}]}}}\n'
    ;;
  *)
    exit 64
    ;;
esac
ROUTE_CURL
chmod +x "$ROUTE_REVIEW_BIN/curl"

ALT_CHECKPOINT_LIBRARY="$TMP/alternate-library.yaml"
printf 'alternate installed app checkpoint library\n' >"$ALT_CHECKPOINT_LIBRARY"

set +e
KORRI_ADB_BIN="$FAKE_ADB" \
KORRI_ANDROID_APP_ROUTE_SMOKE_SH="$SMOKE" \
KORRI_ANDROID_APP_ROUTE_JOURNEY_SH="$JOURNEY" \
KORRI_DEVICE_SCRIPT_REVIEW_ADB_LOG="$ADB_LOG" \
KORRI_DEVICE_SCRIPT_REVIEW_CHILD_LOG="$CHILD_LOG" \
KORRI_ROOT="$ROOT" \
  bash "$ANDROID_APP_ROUTE" device-1 >"$TMP/route.out" 2>"$TMP/route.err"
status=$?
set -e
if [[ "$status" -ne 42 ]]; then
  echo "android-app-route-check.sh restore seam expected child exit 42, got $status" >&2
  cat "$TMP/route.out" >&2
  cat "$TMP/route.err" >&2
  exit 1
fi
if ! grep -F -- 'smoke:--expect-installed-route device-1 package= library= retro=true' "$CHILD_LOG" >/dev/null; then
  echo 'android-app-route-check.sh did not invoke canonical smoke with the RetroArch route enabled' >&2
  exit 1
fi
if ! grep -F -- "push $ROOT/docs/research/retroarch-plugin-route/config.yaml /sdcard/korri/config.yaml" "$ADB_LOG" >/dev/null; then
  echo 'android-app-route-check.sh did not provision checkpoint config.yaml in the dedicated gate' >&2
  exit 1
fi
if ! grep -F -- "push $ROOT/docs/research/retroarch-plugin-route/library.yaml /sdcard/korri/library.yaml" "$ADB_LOG" >/dev/null; then
  echo 'android-app-route-check.sh did not provision checkpoint library.yaml in the dedicated gate' >&2
  exit 1
fi
if ! grep -F -- "cp '/sdcard/korri/config.yaml' '/sdcard/korri/.android-app-route-check-backup-" "$ADB_LOG" >/dev/null; then
  echo 'android-app-route-check.sh did not back up a pre-existing config.yaml before provisioning' >&2
  exit 1
fi
if ! grep -F -- "cp '/sdcard/korri/.android-app-route-check-backup-" "$ADB_LOG" | grep -F -- "/config.yaml' '/sdcard/korri/config.yaml'" >/dev/null; then
  echo 'android-app-route-check.sh did not restore a pre-existing config.yaml after failure' >&2
  exit 1
fi
if ! grep -F -- "rm -f '/sdcard/korri/library.yaml'" "$ADB_LOG" >/dev/null; then
  echo 'android-app-route-check.sh did not remove a library.yaml it created after failure' >&2
  exit 1
fi
lock_line="$(grep -nF -- "mkdir '/sdcard/korri/.android-app-route-check.lock'" "$ADB_LOG" | head -1 | cut -d: -f1)"
backup_line="$(grep -nF -- "cp '/sdcard/korri/config.yaml' '/sdcard/korri/.android-app-route-check-backup-" "$ADB_LOG" | head -1 | cut -d: -f1)"
restore_line="$(grep -nF -- "/config.yaml' '/sdcard/korri/config.yaml'" "$ADB_LOG" | tail -1 | cut -d: -f1)"
unlock_line="$(grep -nF -- "rm -rf '/sdcard/korri/.android-app-route-check.lock'" "$ADB_LOG" | tail -1 | cut -d: -f1)"
if [[ -z "$lock_line" || -z "$backup_line" || -z "$restore_line" || -z "$unlock_line" ]]; then
  echo 'android-app-route-check.sh did not acquire and release the route-check lock around config backup/restore' >&2
  exit 1
fi
if (( lock_line >= backup_line )); then
  echo 'android-app-route-check.sh must acquire the device lock before backing up fixed config files' >&2
  exit 1
fi
if (( unlock_line <= restore_line )); then
  echo 'android-app-route-check.sh must release the device lock only after restoring fixed config files' >&2
  exit 1
fi

: >"$ADB_LOG"
: >"$CHILD_LOG"
set +e
KORRI_ADB_BIN="$FAKE_ADB" \
KORRI_ANDROID_APP_ROUTE_SMOKE_SH="$SMOKE" \
KORRI_ANDROID_APP_ROUTE_JOURNEY_SH="$JOURNEY" \
KORRI_DEVICE_SCRIPT_REVIEW_ADB_LOG="$ADB_LOG" \
KORRI_DEVICE_SCRIPT_REVIEW_CHILD_LOG="$CHILD_LOG" \
KORRI_DEVICE_SCRIPT_REVIEW_ROUTE_LOCK_HELD=true \
KORRI_ROOT="$ROOT" \
  bash "$ANDROID_APP_ROUTE" device-1 >"$TMP/route-held-lock.out" 2>"$TMP/route-held-lock.err"
held_lock_status=$?
set -e
if [[ "$held_lock_status" -eq 0 ]]; then
  echo 'android-app-route-check.sh accepted a held route-check device lock' >&2
  exit 1
fi
if ! grep -F -- 'If this is stale, remove it manually only after verifying no route check is running.' "$TMP/route-held-lock.err" >/dev/null; then
  echo 'android-app-route-check.sh held-lock failure did not explain manual stale-lock recovery' >&2
  cat "$TMP/route-held-lock.err" >&2
  exit 1
fi
if grep -E -- "push .* /sdcard/korri/(config|library)\.yaml|cp '/sdcard/korri/(config|library)\.yaml'" "$ADB_LOG" >/dev/null; then
  echo 'android-app-route-check.sh mutated fixed config files after failing to acquire the device lock' >&2
  exit 1
fi

: >"$ADB_LOG"
: >"$CHILD_LOG"
set +e
KORRI_ADB_BIN="$FAKE_ADB" \
KORRI_ANDROID_APP_ROUTE_SMOKE_SH="$SMOKE" \
KORRI_ANDROID_APP_ROUTE_JOURNEY_SH="$JOURNEY" \
KORRI_DEVICE_SCRIPT_REVIEW_ADB_LOG="$ADB_LOG" \
KORRI_DEVICE_SCRIPT_REVIEW_CHILD_LOG="$CHILD_LOG" \
KORRI_DEVICE_SCRIPT_REVIEW_CLEANUP_FAIL=restore \
KORRI_ROOT="$ROOT" \
  bash "$ANDROID_APP_ROUTE" device-1 >"$TMP/route-original-failure-cleanup.out" 2>"$TMP/route-original-failure-cleanup.err"
original_cleanup_status=$?
set -e
if [[ "$original_cleanup_status" -ne 42 ]]; then
  echo "android-app-route-check.sh failed to preserve original nonzero status when cleanup also failed (got $original_cleanup_status)" >&2
  cat "$TMP/route-original-failure-cleanup.out" >&2
  cat "$TMP/route-original-failure-cleanup.err" >&2
  exit 1
fi
if ! grep -F -- 'Android app route check failed to restore prior config.yaml' "$TMP/route-original-failure-cleanup.err" >/dev/null; then
  echo 'android-app-route-check.sh did not emit a clear restore failure while preserving original failure status' >&2
  cat "$TMP/route-original-failure-cleanup.err" >&2
  exit 1
fi
if grep -F -- 'cleanup failed after successful run' "$TMP/route-original-failure-cleanup.err" >/dev/null; then
  echo 'android-app-route-check.sh treated an original failure as a successful-main cleanup failure' >&2
  cat "$TMP/route-original-failure-cleanup.err" >&2
  exit 1
fi

: >"$ADB_LOG"
: >"$CHILD_LOG"
set +e
PATH="$ROUTE_REVIEW_BIN:$PATH" \
KORRI_ADB_BIN="$FAKE_ADB" \
KORRI_ANDROID_APP_ROUTE_SMOKE_SH="$SMOKE_SUCCESS" \
KORRI_ANDROID_APP_ROUTE_JOURNEY_SH="$JOURNEY" \
KORRI_DEVICE_SCRIPT_REVIEW_ADB_LOG="$ADB_LOG" \
KORRI_DEVICE_SCRIPT_REVIEW_CHILD_LOG="$CHILD_LOG" \
KORRI_DEVICE_SCRIPT_REVIEW_CLEANUP_FAIL=restore \
KORRI_ROOT="$ROOT" \
  bash "$ANDROID_APP_ROUTE" device-1 >"$TMP/route-success-cleanup-failure.out" 2>"$TMP/route-success-cleanup-failure.err"
success_cleanup_status=$?
set -e
if [[ "$success_cleanup_status" -eq 0 ]]; then
  echo 'android-app-route-check.sh reported success even though cleanup restore failed after a successful run' >&2
  cat "$TMP/route-success-cleanup-failure.out" >&2
  cat "$TMP/route-success-cleanup-failure.err" >&2
  exit 1
fi
if ! grep -F -- 'Android app route check failed to restore prior config.yaml' "$TMP/route-success-cleanup-failure.err" >/dev/null; then
  echo 'android-app-route-check.sh did not emit a clear successful-main restore failure' >&2
  cat "$TMP/route-success-cleanup-failure.err" >&2
  exit 1
fi
if ! grep -F -- 'Android app route check cleanup failed after successful run' "$TMP/route-success-cleanup-failure.err" >/dev/null; then
  echo 'android-app-route-check.sh did not turn successful-main cleanup failure into an explicit nonzero failure' >&2
  cat "$TMP/route-success-cleanup-failure.err" >&2
  exit 1
fi
if ! grep -F -- "rm -rf '/sdcard/korri/.android-app-route-check.lock'" "$ADB_LOG" >/dev/null; then
  echo 'android-app-route-check.sh did not attempt to release the route-check lock after a restore cleanup failure' >&2
  exit 1
fi

: >"$ADB_LOG"
: >"$CHILD_LOG"
set +e
PATH="$ROUTE_REVIEW_BIN:$PATH" \
KORRI_ADB_BIN="$FAKE_ADB" \
KORRI_ANDROID_APP_ROUTE_SMOKE_SH="$SMOKE_SUCCESS" \
KORRI_ANDROID_APP_ROUTE_JOURNEY_SH="$JOURNEY" \
KORRI_DEVICE_SCRIPT_REVIEW_ADB_LOG="$ADB_LOG" \
KORRI_DEVICE_SCRIPT_REVIEW_CHILD_LOG="$CHILD_LOG" \
KORRI_DEVICE_SCRIPT_REVIEW_CLEANUP_FAIL=unlock \
KORRI_ROOT="$ROOT" \
  bash "$ANDROID_APP_ROUTE" device-1 >"$TMP/route-success-unlock-failure.out" 2>"$TMP/route-success-unlock-failure.err"
success_unlock_status=$?
set -e
if [[ "$success_unlock_status" -eq 0 ]]; then
  echo 'android-app-route-check.sh reported success even though cleanup unlock failed after a successful run' >&2
  cat "$TMP/route-success-unlock-failure.out" >&2
  cat "$TMP/route-success-unlock-failure.err" >&2
  exit 1
fi
if ! grep -F -- 'Android app route check failed to release the device config lock' "$TMP/route-success-unlock-failure.err" >/dev/null; then
  echo 'android-app-route-check.sh did not emit a clear successful-main unlock failure' >&2
  cat "$TMP/route-success-unlock-failure.err" >&2
  exit 1
fi
if ! grep -F -- 'Android app route check cleanup failed after successful run' "$TMP/route-success-unlock-failure.err" >/dev/null; then
  echo 'android-app-route-check.sh did not turn successful-main unlock failure into an explicit nonzero failure' >&2
  cat "$TMP/route-success-unlock-failure.err" >&2
  exit 1
fi

: >"$ADB_LOG"
: >"$CHILD_LOG"
set +e
KORRI_ADB_BIN="$FAKE_ADB" \
KORRI_ANDROID_APP_PACKAGE=review.android.game \
KORRI_ANDROID_APP_ROUTE_CHECKPOINT_LIBRARY="$ALT_CHECKPOINT_LIBRARY" \
KORRI_ANDROID_APP_ROUTE_SMOKE_SH="$SMOKE" \
KORRI_ANDROID_APP_ROUTE_JOURNEY_SH="$JOURNEY" \
KORRI_DEVICE_SCRIPT_REVIEW_ADB_LOG="$ADB_LOG" \
KORRI_DEVICE_SCRIPT_REVIEW_CHILD_LOG="$CHILD_LOG" \
KORRI_ROOT="$ROOT" \
  bash "$ANDROID_APP_ROUTE" device-1 >"$TMP/route-alternate.out" 2>"$TMP/route-alternate.err"
alternate_status=$?
set -e
if [[ "$alternate_status" -ne 42 ]]; then
  echo "android-app-route-check.sh alternate route seam expected child exit 42, got $alternate_status" >&2
  cat "$TMP/route-alternate.out" >&2
  cat "$TMP/route-alternate.err" >&2
  exit 1
fi
if ! grep -F -- 'pm path review.android.game' "$ADB_LOG" >/dev/null; then
  echo 'android-app-route-check.sh did not require the configured alternate Android app package' >&2
  exit 1
fi
if ! grep -F -- "push $ALT_CHECKPOINT_LIBRARY /sdcard/korri/library.yaml" "$ADB_LOG" >/dev/null; then
  echo 'android-app-route-check.sh did not provision the configured alternate checkpoint library path' >&2
  exit 1
fi
if grep -F -- "push $ROOT/docs/research/retroarch-plugin-route/library.yaml /sdcard/korri/library.yaml" "$ADB_LOG" >/dev/null; then
  echo 'android-app-route-check.sh ignored the alternate checkpoint library path and pushed the canonical library' >&2
  exit 1
fi
if ! grep -F -- "smoke:--expect-installed-route device-1 package=review.android.game library=$ALT_CHECKPOINT_LIBRARY retro=false" "$CHILD_LOG" >/dev/null; then
  echo 'android-app-route-check.sh did not isolate an alternate Android fixture from the canonical RetroArch route' >&2
  exit 1
fi

run_route_resumed_activity_review() {
  local format="$1"
  local expected_field="$2"
  local label="$3"
  local out="$TMP/route-$label.out"
  local err="$TMP/route-$label.err"
  : >"$ADB_LOG"
  : >"$CHILD_LOG"
  PATH="$ROUTE_REVIEW_BIN:$PATH" \
  KORRI_ADB_BIN="$FAKE_ADB" \
  KORRI_ANDROID_APP_ROUTE_SMOKE_SH="$SMOKE_SUCCESS" \
  KORRI_ANDROID_APP_ROUTE_JOURNEY_SH="$JOURNEY" \
  KORRI_DEVICE_SCRIPT_REVIEW_ADB_LOG="$ADB_LOG" \
  KORRI_DEVICE_SCRIPT_REVIEW_CHILD_LOG="$CHILD_LOG" \
  KORRI_DEVICE_SCRIPT_REVIEW_RESUMED_ACTIVITY_FORMAT="$format" \
  KORRI_ROOT="$ROOT" \
    bash "$ANDROID_APP_ROUTE" device-1 >"$out" 2>"$err"
  if ! grep -F -- "$expected_field" "$out" >/dev/null; then
    echo "android-app-route-check.sh did not preserve $expected_field foreground evidence" >&2
    cat "$out" >&2
    cat "$err" >&2
    exit 1
  fi
  if ! grep -F -- 'Android app route health while game foreground:' "$out" >/dev/null; then
    echo 'android-app-route-check.sh did not complete foreground health assertions under fake adb review' >&2
    cat "$out" >&2
    cat "$err" >&2
    exit 1
  fi
}

run_route_resumed_activity_review modern topResumedActivity modern
run_route_resumed_activity_review android13 topResumedActivity android13
run_route_resumed_activity_review android12 mResumedActivity android12

printf 'Android device script review: ok\n'
