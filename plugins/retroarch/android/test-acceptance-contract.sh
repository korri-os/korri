#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash coreutils curl diffutils findutils gawk git gnugrep gnused imagemagick tesseract jq python3 python3Packages.pyyaml
# shellcheck shell=bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ACCEPTANCE="$HERE/device-acceptance.sh"
ROOT="$(cd "$HERE/../../.." && pwd)"
WL4_GAMES="$ROOT/docs/research/retroarch-plugin-route/catalog/games.yaml"

[[ -f "$WL4_GAMES" ]]
# shellcheck disable=SC2016 # Literal source-contract needles.
grep -F 'CHECKPOINT_GAMES="$ROOT/docs/research/retroarch-plugin-route/catalog/games.yaml"' "$ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needles.
grep -F 'LOCK_REMOTE="$ANDROID_STORAGE_ROOT/.android-app-route-check.lock"' "$ACCEPTANCE" >/dev/null
grep -F 'if restore_checkpoint_files; then' "$ACCEPTANCE" >/dev/null
grep -F 'remote_state()' "$ACCEPTANCE" >/dev/null
grep -F 'Only now may cleanup interpret false flags as original absence' "$ACCEPTANCE" >/dev/null
grep -F 'rmdir ' "$ACCEPTANCE" >/dev/null
grep -F 'retroarch.cfg' "$ACCEPTANCE" >/dev/null
grep -F 'wl4.state.auto' "$ACCEPTANCE" >/dev/null
grep -F 'wl4.srm' "$ACCEPTANCE" >/dev/null
grep -F 'must be stopped before acceptance can back up save state' "$ACCEPTANCE" >/dev/null
grep -F 'could not quiesce the exact recorded launch; backup and lock retained' "$ACCEPTANCE" >/dev/null
for stage in \
  preflight fixture portal-card portal-detail portal-location-launch wait-playing \
  udp-negative overlay-menu save-pause relaunch quit-stale restoration; do
  grep -F "STAGE=\"$stage\"" "$ACCEPTANCE" >/dev/null || {
    echo "RetroArch acceptance is missing diagnostic stage: $stage" >&2
    exit 1
  }
done
save_pause_source="$(sed -n '/^STAGE="save-pause"/,/^STAGE="relaunch"/p' "$ACCEPTANCE")"
# shellcheck disable=SC2016 # Literal source-contract needles.
pause_mtime_proof_line="$(grep -nF '[[ "${after_mtime:-0}" -gt "$before_mtime" && "${state_size:-0}" -gt 0 ]]' <<<"$save_pause_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needles.
paused_launch_line="$(grep -nF 'paused_launch_id="$GATE_CURRENT_LAUNCH"' <<<"$save_pause_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needles.
pause_quit_line="$(grep -nF 'pause_quit="$(invoke_control "$paused_launch_id" '\''@korri:retroarch/quit'\'')"' <<<"$save_pause_source" | cut -d: -f1)"
pause_completed_line="$(grep -nF '.outcome._tag == "Ok" and .outcome.payload._tag == "Completed"' <<<"$save_pause_source" | cut -d: -f1)"
wait_stopped_pause_line="$(grep -nF 'wait_stopped' <<<"$save_pause_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needles.
wait_stale_pause_line="$(grep -nF 'wait_old_launch_stale "$paused_launch_id"' <<<"$save_pause_source" | cut -d: -f1)"
quiesced_pause_line="$(grep -nF 'GATE_CURRENT_LAUNCH_QUIESCED=true' <<<"$save_pause_source" | cut -d: -f1)"
relaunch_stage_line="$(grep -nF 'STAGE="relaunch"' <<<"$save_pause_source" | cut -d: -f1)"
[[ -n "$pause_mtime_proof_line" && -n "$paused_launch_line" \
  && -n "$pause_quit_line" && -n "$pause_completed_line" \
  && -n "$wait_stopped_pause_line" && -n "$wait_stale_pause_line" \
  && -n "$quiesced_pause_line" && -n "$relaunch_stage_line" \
  && "$pause_mtime_proof_line" -lt "$paused_launch_line" \
  && "$paused_launch_line" -lt "$pause_quit_line" \
  && "$pause_quit_line" -lt "$pause_completed_line" \
  && "$pause_completed_line" -lt "$wait_stopped_pause_line" \
  && "$wait_stopped_pause_line" -lt "$wait_stale_pause_line" \
  && "$wait_stale_pause_line" -lt "$quiesced_pause_line" \
  && "$quiesced_pause_line" -lt "$relaunch_stage_line" ]] || {
  echo 'RetroArch save-pause must prove pause persistence, receive authenticated Quit Completed, then prove stop/stale before relaunch' >&2
  exit 1
}
if grep -F 'shell am force-stop "$FORK_PACKAGE"' <<<"$save_pause_source" >/dev/null; then
  echo 'RetroArch save-pause main path must not force-stop the emulator' >&2
  exit 1
fi
if ! grep -F '01KZBYHCA4R9C8QK131HK0VWSA' <<<"$save_pause_source" >/dev/null; then
  echo 'RetroArch save-pause must document the deferred abrupt-death lifecycle gate' >&2
  exit 1
fi
if grep -Eq 'DEBUG_PORTAL_RELOAD_SH|launch_wario_entry' <<<"$save_pause_source"; then
  echo 'RetroArch save-pause must not reload or relaunch before paired retirement proof' >&2
  exit 1
fi
launch_flow_source_for_quiescence="$(sed -n '/^launch_wario_entry() {/,/^}/p' "$ACCEPTANCE")"
grep -F 'GATE_CURRENT_LAUNCH_QUIESCED=false' \
  <<<"$launch_flow_source_for_quiescence" >/dev/null || {
  echo 'A newly published local launch must reset the quiesced marker' >&2
  exit 1
}

stage_report_source="$(sed -n '/^report_stage_failure() {/,/^}/p' "$ACCEPTANCE")"
grep -F "printf 'RetroArch acceptance failed: stage=%s line=%s\\n'" \
  <<<"$stage_report_source" >/dev/null
if grep -Eq 'BASH_COMMAND|capability|authority|payload|authorization' \
    <<<"$stage_report_source"; then
  echo 'RetroArch stage diagnostics must expose only stage and line' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'report_stage_failure "$status" "$LINENO"' "$ACCEPTANCE" >/dev/null
grep -F 'set -Eeuo pipefail' "$ACCEPTANCE" >/dev/null
cleanup_source="$(sed -n '/^cleanup() {/,/^}/p' "$ACCEPTANCE")"
for classification in \
  'active-launch=%s' 'active_classification="absent"' \
  'active_classification="recorded"' 'active_classification="replacement"' \
  'fork-pid=%s' 'pid_classification="recorded"' \
  'pid_classification="replacement"'; do
  grep -F "$classification" <<<"$cleanup_source" >/dev/null || {
    echo "RetroArch cleanup is missing safe classification: $classification" >&2
    exit 1
  }
done
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'confirmed_pid="$(package_pid "$FORK_PACKAGE")"' \
  <<<"$cleanup_source" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'for ((quiesce_attempt = 1; quiesce_attempt <= 20; quiesce_attempt++)); do' \
  <<<"$cleanup_source" >/dev/null
grep -F 'sleep 0.25' <<<"$cleanup_source" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
revalidate_pid_line="$(grep -nF 'confirmed_pid="$(package_pid "$FORK_PACKAGE")"' \
  <<<"$cleanup_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
force_stop_line="$(grep -nF 'shell am force-stop "$FORK_PACKAGE"' \
  <<<"$cleanup_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
bounded_wait_line="$(grep -nF 'for ((quiesce_attempt = 1; quiesce_attempt <= 20; quiesce_attempt++)); do' \
  <<<"$cleanup_source" | cut -d: -f1)"
[[ -n "$revalidate_pid_line" && -n "$force_stop_line" && -n "$bounded_wait_line" \
  && "$revalidate_pid_line" -lt "$force_stop_line" \
  && "$force_stop_line" -lt "$bounded_wait_line" ]] || {
  echo 'RetroArch cleanup must revalidate the recorded PID, force-stop, then wait boundedly' >&2
  exit 1
}
if grep -Eq 'BASH_COMMAND|authorization: Bearer|capability=' <<<"$cleanup_source"; then
  echo 'RetroArch cleanup diagnostics must not expose commands or authority' >&2
  exit 1
fi
for cleanup_contract in \
  "controls_for_launch \"\$GATE_CURRENT_LAUNCH\"" \
  "invoke_control \"\$GATE_CURRENT_LAUNCH\" '@korri:retroarch/quit'" \
  'tracked_launch_stale=true' \
  "[[ \"\$tracked_launch_stale\" == true ]]" \
  "[[ \"\$GATE_CURRENT_LAUNCH_QUIESCED\" == true ]]"; do
  grep -F "$cleanup_contract" <<<"$cleanup_source" >/dev/null || {
    echo "RetroArch cleanup is missing tracked local-launch policy: $cleanup_contract" >&2
    exit 1
  }
done
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'EXPECTED_MODEL="$2"' "$ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'EXPECTED_HARDWARE_SERIAL="$3"' "$ACCEPTANCE" >/dev/null
grep -F 'getprop ro.product.model' "$ACCEPTANCE" >/dev/null
grep -F 'getprop ro.serialno' "$ACCEPTANCE" >/dev/null
grep -F 'exec-out screencap -p' "$ACCEPTANCE" >/dev/null
grep -F 'GATE_LAUNCH_IDS' "$ACCEPTANCE" >/dev/null
grep -F 'assert_no_artemis_game_activity' "$ACCEPTANCE" >/dev/null
grep -F 'assert_korri_process_unchanged' "$ACCEPTANCE" >/dev/null
grep -F 'focus_wario_in_installed_library' "$ACCEPTANCE" >/dev/null
grep -F 'traverse_library_to_final_viewport' "$ACCEPTANCE" >/dev/null
grep -F 'DEBUG_PORTAL_FOCUS_GAME_SH=' "$ACCEPTANCE" >/dev/null
grep -F 'portal_shot_focuses_wario' "$ACCEPTANCE" >/dev/null
grep -F 'render_focused_wario_crop_evidence' "$ACCEPTANCE" >/dev/null
grep -F 'verified_element_center' "$ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F '"${ADB[@]}" shell input tap "$tap_x" "$tap_y"' "$ACCEPTANCE" >/dev/null
[[ "$(grep -Fc 'shell input tap' "$ACCEPTANCE")" -eq 4 ]]
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F -- '"$SERIAL" "$KORRI_PACKAGE" --detail-play' "$ACCEPTANCE" >/dev/null
grep -F 'detail-play.focus.json' "$ACCEPTANCE" >/dev/null
grep -F 'detail-play.png' "$ACCEPTANCE" >/dev/null
grep -F -- '--launch-location' "$ACCEPTANCE" >/dev/null
grep -F 'local-location.focus.json' "$ACCEPTANCE" >/dev/null
grep -F 'local-location.png' "$ACCEPTANCE" >/dev/null
grep -F "local local_location_id='[\"local\",null,\"01K4J6K8Y00000000000000002\"]'" "$ACCEPTANCE" >/dev/null
grep -F 'Physical controller confirm remains mandatory' "$ACCEPTANCE" >/dev/null
launch_flow_source="$(sed -n '/^launch_wario_entry() {/,/^}/p' "$ACCEPTANCE")"
# shellcheck disable=SC2016 # Literal source-contract needles.
card_tap_line="$(grep -nF 'shell input tap "$tap_x" "$tap_y"' <<<"$launch_flow_source" | sed -n '1s/:.*//p')"
# shellcheck disable=SC2016 # Literal source-contract needle.
detail_focus_line="$(grep -nF -- '"$SERIAL" "$KORRI_PACKAGE" --detail-play' <<<"$launch_flow_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
play_tap_line="$(grep -nF 'shell input tap "$tap_x" "$tap_y"' <<<"$launch_flow_source" | sed -n '2s/:.*//p')"
location_focus_line="$(grep -nF -- '--launch-location' <<<"$launch_flow_source" | head -1 | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
location_tap_line="$(grep -nF 'shell input tap "$tap_x" "$tap_y"' <<<"$launch_flow_source" | sed -n '3s/:.*//p')"
[[ -n "$card_tap_line" && -n "$detail_focus_line" && -n "$play_tap_line" \
  && -n "$location_focus_line" && -n "$location_tap_line" \
  && "$card_tap_line" -lt "$detail_focus_line" \
  && "$detail_focus_line" -lt "$play_tap_line" \
  && "$play_tap_line" -lt "$location_focus_line" \
  && "$location_focus_line" -lt "$location_tap_line" ]] || {
  echo 'RetroArch acceptance must verify detail and explicit local choice before launch' >&2
  exit 1
}
# shellcheck disable=SC2016 # Literal source-contract needles.
for failure_evidence in \
  'local-location.launch-failed.png' \
  'local-location.launch-failed.txt' \
  'exact local launch row remained visible after the one pointer activation' \
  'evidence is in $PORTAL_EVIDENCE_DIR'; do
  grep -F "$failure_evidence" <<<"$launch_flow_source" >/dev/null || {
    echo "RetroArch local launch failure evidence is missing: $failure_evidence" >&2
    exit 1
  }
done
[[ "$(grep -Fc -- '--launch-location' <<<"$launch_flow_source")" -eq 2 ]]
# shellcheck disable=SC2016 # Literal source-contract range.
location_failure_source="$(sed -n \
  '/exec-out screencap -p >"$location_failure_image"/,/# This is the first process observed/p' \
  <<<"$launch_flow_source")"
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'timeout 5 "$DEBUG_PORTAL_FOCUS_GAME_SH"' \
  <<<"$location_failure_source" >/dev/null
if grep -Eq 'shell input tap|rpc |app\.local-games\.launch' <<<"$location_failure_source"; then
  echo 'RetroArch local launch diagnostics must not activate or invoke a launch RPC' >&2
  exit 1
fi
if grep -Eq '\.click\(' "$ACCEPTANCE"; then
  echo 'RetroArch UI activation must not use DevTools click' >&2
  exit 1
fi
# A fresh marker precedes the one installed pointer activation. The resulting
# dedicated publication event discovers local identity without an RPC cycle.
# shellcheck disable=SC2016 # Literal source-contract needles.
publication_marker_line="$(grep -nF 'publication_marker="$(new_logcat_marker' \
  <<<"$launch_flow_source" | cut -d: -f1)"
record_pid_line="$(grep -nF 'record_gate_pid "$pid"' <<<"$launch_flow_source" | cut -d: -f1)"
record_launch_line="$(grep -nF 'record_gate_launch "$GATE_CURRENT_LAUNCH"' <<<"$launch_flow_source" | cut -d: -f1)"
controls_line="$(grep -nF 'observed_controls="$(controls_for_launch' <<<"$launch_flow_source" | cut -d: -f1)"
publication_recheck_line="$(grep -nF 'if ! publication_launch="$(parse_local_publication' \
  <<<"$launch_flow_source" | cut -d: -f1)"
resume_rpc_line="$(grep -nF 'observed_resume="$(rpc' <<<"$launch_flow_source" | cut -d: -f1)"
[[ -n "$publication_marker_line" && -n "$record_pid_line" && -n "$record_launch_line" \
  && -n "$controls_line" && -n "$publication_recheck_line" && -n "$resume_rpc_line" \
  && "$publication_marker_line" -lt "$location_tap_line" \
  && "$location_tap_line" -lt "$record_pid_line" \
  && "$record_pid_line" -lt "$record_launch_line" \
  && "$record_launch_line" -lt "$controls_line" \
  && "$controls_line" -lt "$publication_recheck_line" \
  && "$publication_recheck_line" -lt "$resume_rpc_line" ]] || {
  echo 'RetroArch acceptance must mark, activate, record PID/publication, prove controls, recheck publication, then resume' >&2
  exit 1
}
for publication_contract in \
  'KorriLocalLifecycle:I' \
  'local-publication.log' \
  'publication_count" -eq 1' \
  'parse_local_publication "$publication_lines"' \
  'record_gate_launch "$GATE_CURRENT_LAUNCH"'; do
  grep -F "$publication_contract" <<<"$launch_flow_source" >/dev/null || {
    echo "RetroArch publication discovery is missing: $publication_contract" >&2
    exit 1
  }
done
publication_parser_source="$(sed -n '/^parse_local_publication() {/,/^}/p' "$ACCEPTANCE")"
grep -F 'launchId=([0-9a-f]{32}) event=published gameId=01K4J6K8Y00000000000000002 package=com\.korri\.retroarch launcher=retroarch' \
  <<<"$publication_parser_source" >/dev/null
for controls_contract in \
  'local-controls.json' \
  'controls_ready=false' \
  '@korri:retroarch/open-menu' \
  '@korri:retroarch/quit' \
  'retroarchTelemetry.contentBasename == "wl4.gba"' \
  'retroarchTelemetry.crc32 == "d6141609"'; do
  grep -F "$controls_contract" <<<"$launch_flow_source" >/dev/null || {
    echo "RetroArch bounded controls readiness is missing: $controls_contract" >&2
    exit 1
  }
done
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'observed_resume="$(rpc '\''{"_tag":"app.local-games.launch","payload":{"gameId":"01K4J6K8Y00000000000000002"}}'\'')"' \
  <<<"$launch_flow_source" >/dev/null
grep -F 'assert_exact_wario_resume "$observed_resume" "$GATE_CURRENT_LAUNCH"' \
  <<<"$launch_flow_source" >/dev/null
if grep -E 'rpc .*app\.session\.status' <<<"$launch_flow_source" >/dev/null; then
  echo 'local launch discovery must never read federated app.session.status' >&2
  exit 1
fi
resume_assertion_source="$(sed -n '/^assert_exact_wario_resume() {/,/^}/p' "$ACCEPTANCE")"
for resume_contract in \
  '.outcome.payload.disposition == "resume"' \
  '.outcome.payload.launchId == $launchId' \
  'test("^[0-9a-f]{32}$")' \
  '.outcome.payload.context.gameId == "01K4J6K8Y00000000000000002"' \
  '.outcome.payload.context.contentCrc32 == "d6141609"' \
  '"packageName":"com.korri.retroarch"' \
  '"id":"@korri:mgba/mgba"' \
  '.outcome.payload.extras.ROM == "/storage/emulated/0/korri/roms/wl4.gba"' \
  '.outcome.payload.extras.LIBRETRO == "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so"'; do
  grep -F "$resume_contract" <<<"$resume_assertion_source" >/dev/null || {
    echo "RetroArch exact resume contract is missing: $resume_contract" >&2
    exit 1
  }
done
grep -F 'focus-crop-4x.png' "$ACCEPTANCE" >/dev/null
grep -F 'focusOutlinePaddedCropRatio' "$ACCEPTANCE" >/dev/null
grep -F -- '-colorspace sRGB' "$ACCEPTANCE" >/dev/null
grep -F 'ratio + 0 >= 0.01' "$ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract pattern includes `$12`.
if grep -Eq 'deskew|tolower\(\$12\) == "wario"|brightness.*-ge' "$ACCEPTANCE"; then
  echo 'RetroArch focus evidence must not depend on full-frame OCR title coordinates' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F -- '"$SERIAL" "$KORRI_PACKAGE" --library' "$ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F -- '"$SERIAL" "$KORRI_PACKAGE" --verify-library' "$ACCEPTANCE" >/dev/null
grep -F -- "--game 'local-game:01K4J6K8Y00000000000000002' 'Wario Land 4'" "$ACCEPTANCE" >/dev/null
# A DevTools-focused element must be activated only through its validated
# installed-pointer bounds. Physical A remains in the human overlay gate.
if grep -F 'KEYCODE_BUTTON_A' "$ACCEPTANCE" >/dev/null; then
  echo 'RetroArch acceptance must not assume controller A after DevTools focus' >&2
  exit 1
fi
if grep -F 'KEYCODE_BUTTON_MODE' "$ACCEPTANCE" >/dev/null; then
  echo 'RetroArch acceptance must never synthesize Guide with KEYCODE_BUTTON_MODE' >&2
  exit 1
fi
if grep -Eq '^(press_guide|invoke_overlay_row)\(\)' "$ACCEPTANCE"; then
  echo 'RetroArch acceptance must use fail-closed human overlay checkpoints' >&2
  exit 1
fi
grep -F 'human_checkpoint open-retroarch-menu' "$ACCEPTANCE" >/dev/null
grep -F 'human_checkpoint move-retroarch-menu' "$ACCEPTANCE" >/dev/null
grep -F 'human_checkpoint resume-retroarch-menu' "$ACCEPTANCE" >/dev/null
if grep -F 'human_checkpoint close-retroarch-menu' "$ACCEPTANCE" >/dev/null \
    || grep -F 'Press physical B exactly once to close the native RetroArch menu.' \
      "$ACCEPTANCE" >/dev/null; then
  echo 'RetroArch acceptance must resume from Quick Menu rather than claiming B closes it' >&2
  exit 1
fi
native_resume_checkpoint_contract="$(cat <<'EOF'
human_checkpoint resume-retroarch-menu $'On the physical device only:\n  1. Press physical Up exactly once to return selection to Resume.\n  2. Visually verify Resume is selected.\n  3. Press physical A exactly once to activate Resume and return to gameplay.'
EOF
)"
grep -Fx "$native_resume_checkpoint_contract" "$ACCEPTANCE" >/dev/null
grep -F 'human_checkpoint resume-from-overlay' "$ACCEPTANCE" >/dev/null
grep -F 'human_checkpoint quit-retroarch' "$ACCEPTANCE" >/dev/null
grep -F 'Visually verify the actual Shift gameplay sheet is visible' "$ACCEPTANCE" >/dev/null
human_checkpoint_source="$(sed -n '/^human_checkpoint() {/,/^}/p' "$ACCEPTANCE")"
grep -F "printf 'Human checkpoint %s:" <<<"$human_checkpoint_source" >/dev/null
grep -F 'IFS= read -r confirmation' <<<"$human_checkpoint_source" >/dev/null
grep -F 'stdin reached EOF before confirmation' <<<"$human_checkpoint_source" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'expected_confirmation="${RUN_NONCE}/${checkpoint_id}"' \
  <<<"$human_checkpoint_source" >/dev/null
if grep -Eq '/dev/tty|read .*-[tT]|timeout' <<<"$human_checkpoint_source"; then
  echo 'RetroArch human checkpoints must use blocking managed stdin without timeout or TTY assumptions' >&2
  exit 1
fi
(
  eval "$human_checkpoint_source"
  RUN_NONCE=0123456789abcdef0123456789abcdef
  expected="$RUN_NONCE/static-checkpoint"
  printf '%s\n' "$expected" | human_checkpoint static-checkpoint instructions \
    >/dev/null 2>&1
  if printf '%s\n' "$expected-extra" | human_checkpoint static-checkpoint instructions \
      >/dev/null 2>&1; then
    echo 'RetroArch human checkpoint accepted a non-exact confirmation' >&2
    exit 1
  fi
  if human_checkpoint static-checkpoint instructions </dev/null >/dev/null 2>&1; then
    echo 'RetroArch human checkpoint passed when managed stdin reached EOF' >&2
    exit 1
  fi
)
grep -F 'assert_overlay_window_absent' "$ACCEPTANCE" >/dev/null
grep -F 'Korri gameplay overlay' "$ACCEPTANCE" >/dev/null
grep -F 'authenticated_retroarch_status' "$ACCEPTANCE" >/dev/null
grep -F 'discover_live_korri_authority' "$ACCEPTANCE" >/dev/null
grep -F 'DEBUG_PORTAL_RELOAD_SH=' "$ACCEPTANCE" >/dev/null
grep -F -- "--expect-game 01K4J6K8Y00000000000000002 'Wario Land 4'" "$ACCEPTANCE" >/dev/null
grep -F -- '--expect-portal' "$ACCEPTANCE" >/dev/null
grep -F 'assert_pristine_gate_state' "$ACCEPTANCE" >/dev/null
grep -F 'assert_session_idle' "$ACCEPTANCE" >/dev/null
grep -F 'revalidate_gate_state_after_mutation' "$ACCEPTANCE" >/dev/null
grep -F 'RUN_NONCE=' "$ACCEPTANCE" >/dev/null
grep -F 'BACKUP_OWNER_REMOTE=' "$ACCEPTANCE" >/dev/null
grep -F 'BACKUP_CREATED=false' "$ACCEPTANCE" >/dev/null
grep -F 'remove_owned_backup' "$ACCEPTANCE" >/dev/null
grep -F 'refusing pre-existing backup directory' "$ACCEPTANCE" >/dev/null
grep -F 'new_logcat_marker' "$ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'logcat_since "$AUTO_LOAD_LOG_MARKER"' "$ACCEPTANCE" >/dev/null
grep -F '"_tag":"system.health"' "$ACCEPTANCE" >/dev/null
grep -F 'existing_korri_pid=' "$ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal forbidden source pattern.
if grep -F 'am start --display 0 -n "$KORRI_ACTIVITY"' "$ACCEPTANCE" >/dev/null; then
  echo 'RetroArch acceptance must not stack a bare Korri Shell component Activity' >&2
  exit 1
fi
bring_shell_source="$(sed -n '/^bring_existing_shell_task_forward() {/,/^}/p' "$ACCEPTANCE")"
# shellcheck disable=SC2016 # Literal source-contract needles.
for launcher_contract in \
  '-a android.intent.action.MAIN' \
  '-c android.intent.category.LAUNCHER' \
  '-f 0x10200000' \
  '-n "$KORRI_ACTIVITY"' \
  'before_count="$(activity_dump_shell_instance_count' \
  'after_count="$(activity_dump_shell_instance_count' \
  '[[ "$after_count" == 1 ]]' \
  'assert_korri_process_unchanged'; do
  grep -F -- "$launcher_contract" <<<"$bring_shell_source" >/dev/null || {
    echo "launcher-equivalent Korri return is missing: $launcher_contract" >&2
    exit 1
  }
done
[[ "$(grep -Fc 'bring_existing_shell_task_forward' "$ACCEPTANCE")" -ge 3 ]]
grep -F 'assert_accessibility_service_enabled' "$ACCEPTANCE" >/dev/null
grep -F 'assert_shell_foreground' "$ACCEPTANCE" >/dev/null
shell_foreground_source="$(sed -n '/^assert_shell_foreground() {/,/^}/p' "$ACCEPTANCE")"
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'activity_dump_has_resumed_component "$activities" "$KORRI_ACTIVITY"' \
  <<<"$shell_foreground_source" >/dev/null
awake_focus_source="$(sed -n '/^assert_device_awake_and_shell_focused() {/,/^}/p' "$ACCEPTANCE")"
grep -F 'dumpsys power' <<<"$awake_focus_source" >/dev/null
grep -F 'mWakefulness=Awake' <<<"$awake_focus_source" >/dev/null
grep -F 'dumpsys window displays' <<<"$awake_focus_source" >/dev/null
grep -F 'mCurrentFocus=' <<<"$awake_focus_source" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F '[[ "$focused_component" == "$KORRI_ACTIVITY" ]]' <<<"$awake_focus_source" >/dev/null
if grep -Eq 'KEYCODE_WAKEUP|dismiss-keyguard|statusbar collapse' <<<"$awake_focus_source"; then
  echo 'RetroArch pristine preflight must observe wake/focus state without changing it' >&2
  exit 1
fi
pristine_source="$(sed -n '/^assert_pristine_gate_state() {/,/^}/p' "$ACCEPTANCE")"
grep -F 'assert_device_awake_and_shell_focused' <<<"$pristine_source" >/dev/null
grep -F 'assert_single_shell_task_activity' <<<"$pristine_source" >/dev/null
grep -F 'assert_menu_status 1' "$ACCEPTANCE" >/dev/null
grep -F 'assert_selection_advanced' "$ACCEPTANCE" >/dev/null
grep -F 'assert_menu_status 0' "$ACCEPTANCE" >/dev/null
selection_poll_source="$(sed -n '/^assert_selection_advanced() {/,/^}/p' "$ACCEPTANCE")"
grep -F 'for attempt in $(seq 1 20); do' <<<"$selection_poll_source" >/dev/null
grep -F 'sleep 0.1' <<<"$selection_poll_source" >/dev/null
if grep -F 'echo "$status"' <<<"$selection_poll_source" >/dev/null; then
  echo 'RetroArch selection polling must not expose authenticated status payloads' >&2
  exit 1
fi
run_selection_fixture() (
  local before="$1"
  shift
  local -a responses=("$@")
  local counter
  local index
  counter="$(mktemp)"
  trap 'rm -f "$counter"' EXIT
  printf '0' >"$counter"
  GATE_CURRENT_LAUNCH=fixture-launch
  authenticated_retroarch_status() {
    index="$(cat "$counter")"
    if [[ "$index" -ge "${#responses[@]}" ]]; then
      index=$((${#responses[@]} - 1))
    fi
    printf '%s' "$((index + 1))" >"$counter"
    printf '%s\n' "${responses[$index]}"
  }
  sleep() { :; }
  eval "$selection_poll_source"
  assert_selection_advanced "$before"
)
assert_selection_fixture_fails() {
  local label="$1"
  shift
  if run_selection_fixture "$@" >/dev/null 2>&1; then
    echo "RetroArch selection fixture unexpectedly passed: $label" >&2
    exit 1
  fi
}
run_selection_fixture 1 \
  '{"menuAlive":true,"menuSelection":1}' \
  '{"menuAlive":true,"menuSelection":1}' \
  '{"menuAlive":true,"menuSelection":2}' >/dev/null
run_selection_fixture 3 \
  '{"menuAlive":true,"menuSelection":4}' >/dev/null
assert_selection_fixture_fails overshoot 1 \
  '{"menuAlive":true,"menuSelection":3}' \
  '{"menuAlive":true,"menuSelection":2}'
assert_selection_fixture_fails regression 2 \
  '{"menuAlive":true,"menuSelection":1}' \
  '{"menuAlive":true,"menuSelection":3}'
assert_selection_fixture_fails menu-closed 1 \
  '{"menuAlive":false,"menuSelection":1}' \
  '{"menuAlive":true,"menuSelection":2}'
assert_selection_fixture_fails timeout 1 \
  '{"menuAlive":true,"menuSelection":1}'
invalid_selection_output=""
if invalid_selection_output="$(run_selection_fixture 1 \
    '{"menuAlive":true,"menuSelection":"private-selection-value"}' \
    '{"menuAlive":true,"menuSelection":2}' 2>&1)"; then
  echo 'RetroArch non-numeric selection fixture unexpectedly passed' >&2
  exit 1
fi
grep -F 'before=1 after=invalid' <<<"$invalid_selection_output" >/dev/null
if grep -F 'private-selection-value' <<<"$invalid_selection_output" >/dev/null; then
  echo 'RetroArch invalid selection diagnostics exposed the raw telemetry value' >&2
  exit 1
fi
overlay_menu_source="$(sed -n '/^STAGE="overlay-menu"/,/^STAGE="save-pause"/p' "$ACCEPTANCE")"
if grep -Eq 'shell input( -d [^ ]+)? keyevent (KEYCODE_DPAD_DOWN|KEYCODE_BACK)' \
    <<<"$overlay_menu_source"; then
  echo 'RetroArch positive native-menu parity must not synthesize DPAD_DOWN or BACK' >&2
  exit 1
fi
grep -F 'synthetic SELECT input is intentionally a non-parity negative probe' \
  <<<"$overlay_menu_source" >/dev/null
grep -F 'keyevent KEYCODE_BUTTON_SELECT' <<<"$overlay_menu_source" >/dev/null
initial_closed_line="$(grep -nF 'assert_menu_status 0' \
  <<<"$overlay_menu_source" | sed -n '1p' | cut -d: -f1)"
open_checkpoint_line="$(grep -nF 'human_checkpoint open-retroarch-menu' \
  <<<"$overlay_menu_source" | cut -d: -f1)"
open_absent_line="$(grep -nF 'assert_overlay_window_absent' \
  <<<"$overlay_menu_source" | head -1 | cut -d: -f1)"
open_alive_line="$(grep -nF 'assert_menu_status 1' \
  <<<"$overlay_menu_source" | cut -d: -f1)"
before_move_line="$(grep -nF 'capture_rgui_evidence retroarch-rgui-before-move' \
  <<<"$overlay_menu_source" | cut -d: -f1)"
move_checkpoint_line="$(grep -nF 'human_checkpoint move-retroarch-menu' \
  <<<"$overlay_menu_source" | cut -d: -f1)"
selection_assert_line="$(grep -nF 'assert_selection_advanced "$menu_selection_before"' \
  <<<"$overlay_menu_source" | cut -d: -f1)"
after_move_line="$(grep -nF 'capture_rgui_evidence retroarch-rgui-after-move' \
  <<<"$overlay_menu_source" | cut -d: -f1)"
native_resume_checkpoint_line="$(grep -nF 'human_checkpoint resume-retroarch-menu' \
  <<<"$overlay_menu_source" | cut -d: -f1)"
native_resume_status_line="$(grep -nF 'assert_menu_status 0' \
  <<<"$overlay_menu_source" | sed -n '2p' | cut -d: -f1)"
select_probe_line="$(grep -nF 'keyevent KEYCODE_BUTTON_SELECT' \
  <<<"$overlay_menu_source" | cut -d: -f1)"
select_closed_line="$(grep -nF 'assert_menu_status 0' \
  <<<"$overlay_menu_source" | sed -n '3p' | cut -d: -f1)"
resume_checkpoint_line="$(grep -nF 'human_checkpoint resume-from-overlay' \
  <<<"$overlay_menu_source" | cut -d: -f1)"
resume_absent_line="$(grep -nF 'assert_overlay_window_absent' \
  <<<"$overlay_menu_source" | tail -1 | cut -d: -f1)"
resume_closed_line="$(grep -nF 'assert_menu_status 0' \
  <<<"$overlay_menu_source" | tail -1 | cut -d: -f1)"
for ordered_line in \
  initial_closed_line open_checkpoint_line open_absent_line open_alive_line \
  before_move_line move_checkpoint_line selection_assert_line after_move_line \
  native_resume_checkpoint_line native_resume_status_line select_probe_line select_closed_line \
  resume_checkpoint_line resume_absent_line resume_closed_line; do
  [[ -n "${!ordered_line}" ]] || {
    echo "RetroArch overlay-menu ordering marker is missing: $ordered_line" >&2
    exit 1
  }
done
[[ "$initial_closed_line" -lt "$open_checkpoint_line" \
  && "$open_checkpoint_line" -lt "$open_absent_line" \
  && "$open_absent_line" -lt "$open_alive_line" \
  && "$open_alive_line" -lt "$before_move_line" \
  && "$before_move_line" -lt "$move_checkpoint_line" \
  && "$move_checkpoint_line" -lt "$selection_assert_line" \
  && "$selection_assert_line" -lt "$after_move_line" \
  && "$after_move_line" -lt "$native_resume_checkpoint_line" \
  && "$native_resume_checkpoint_line" -lt "$native_resume_status_line" \
  && "$native_resume_status_line" -lt "$select_probe_line" \
  && "$select_probe_line" -lt "$select_closed_line" \
  && "$select_closed_line" -lt "$resume_checkpoint_line" \
  && "$resume_checkpoint_line" -lt "$resume_absent_line" \
  && "$resume_absent_line" -lt "$resume_closed_line" ]] || {
  echo 'RetroArch physical menu checkpoints must precede exact authenticated effect assertions' >&2
  exit 1
}
for physical_instruction in \
  'Press the physical Guide button.' \
  'Press physical Down exactly once to focus Open RetroArch menu.' \
  'Press physical A exactly once to invoke Open RetroArch menu.' \
  'Press physical Down exactly once in the native RetroArch menu.' \
  'Visually verify the native RetroArch menu selection moved down exactly one row.' \
  'Press physical Up exactly once to return selection to Resume.' \
  'Visually verify Resume is selected.' \
  'Press physical A exactly once to activate Resume and return to gameplay.' \
  'Press physical A exactly once on Resume.' \
  'Press physical Down exactly twice to focus Quit game.' \
  'Press physical A exactly once to invoke Quit game.'; do
  grep -F "$physical_instruction" "$ACCEPTANCE" >/dev/null || {
    echo "RetroArch human checkpoint is missing physical instruction: $physical_instruction" >&2
    exit 1
  }
done
quit_stage_source="$(sed -n '/^STAGE="quit-stale"/,/^STAGE="restoration"/p' "$ACCEPTANCE")"
quit_checkpoint_line="$(grep -nF 'human_checkpoint quit-retroarch' \
  <<<"$quit_stage_source" | cut -d: -f1)"
quit_stopped_line="$(grep -nF 'wait_stopped' <<<"$quit_stage_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
quit_stale_line="$(grep -nF 'wait_old_launch_stale "$quit_launch_id"' \
  <<<"$quit_stage_source" | cut -d: -f1)"
[[ -n "$quit_checkpoint_line" && -n "$quit_stopped_line" && -n "$quit_stale_line" \
  && "$quit_checkpoint_line" -lt "$quit_stopped_line" \
  && "$quit_stopped_line" -lt "$quit_stale_line" ]] || {
  echo 'RetroArch physical Quit checkpoint must precede process and stale-control gates' >&2
  exit 1
}
menu_status_source="$(sed -n '/^assert_menu_status() {/,/^}/p' "$ACCEPTANCE")"
grep -F '0) expected_json=false ;;' <<<"$menu_status_source" >/dev/null
grep -F '1) expected_json=true ;;' <<<"$menu_status_source" >/dev/null
(
  eval "$menu_status_source"
  # shellcheck disable=SC2034 # Read by the evaluated source contract.
  GATE_CURRENT_LAUNCH=test-launch
  menu_alive=false
  # shellcheck disable=SC2329 # Called by the evaluated source contract.
  authenticated_retroarch_status() {
    printf '{"menuAlive":%s}\n' "$menu_alive"
  }
  assert_menu_status 0
  menu_alive=true
  assert_menu_status 1
  invalid_error=''
  if invalid_error="$(assert_menu_status false 2>&1)"; then
    echo 'RetroArch menu status assertion accepted non-numeric boolean input' >&2
    exit 1
  fi
  [[ "$invalid_error" == \
    'RetroArch menu status expectation must be exactly 0 or 1: false' ]]
  missing_error=''
  if missing_error="$(assert_menu_status 2>&1)"; then
    echo 'RetroArch menu status assertion accepted missing input' >&2
    exit 1
  fi
  [[ "$missing_error" == \
    'RetroArch menu status expectation must be exactly 0 or 1: ' ]]
)
grep -F 'KEYCODE_BUTTON_SELECT' "$ACCEPTANCE" >/dev/null
grep -F 'network_cmd_port' "$ACCEPTANCE" >/dev/null
grep -F 'UDP_COMPLETION_MARKER=' "$ACCEPTANCE" >/dev/null
grep -F 'assert_adb_probe_ready' "$ACCEPTANCE" >/dev/null
grep -F 'remote_nc_rc=0 remote_nc_output=' "$ACCEPTANCE" >/dev/null
grep -F 'remote_nc_rc=124 remote_nc_output=' "$ACCEPTANCE" >/dev/null
grep -F 'UDP probe transport failed before its remote completion marker' "$ACCEPTANCE" >/dev/null
grep -F 'unauthenticated UDP probe must report empty output and remote rc 0 or 124' "$ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'assert_udp_rejection_log "$UDP_REJECTION_LOG_MARKER"' "$ACCEPTANCE" >/dev/null
grep -F '[NetCMD] Rejected malformed Korri command.' "$ACCEPTANCE" >/dev/null
grep -F '\[NetCMD\] Korri authenticated (request accepted|reply)' "$ACCEPTANCE" >/dev/null
udp_stage_source="$(sed -n '/^STAGE="udp-negative"/,/^STAGE="overlay-menu"/p' "$ACCEPTANCE")"
# shellcheck disable=SC2016 # Literal source-contract needle.
udp_marker_line="$(grep -nF 'UDP_REJECTION_LOG_MARKER="$(new_logcat_marker udp-negative)"' \
  <<<"$udp_stage_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
udp_probe_line="$(grep -nF 'if ! udp_remote_result="$(udp_unauthenticated "$control_port")"; then' \
  <<<"$udp_stage_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
udp_result_line="$(grep -nF 'assert_udp_no_response "$UDP_COMPLETION_MARKER" "$udp_remote_result"' \
  <<<"$udp_stage_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
udp_log_line="$(grep -nF 'assert_udp_rejection_log "$UDP_REJECTION_LOG_MARKER"' \
  <<<"$udp_stage_source" | cut -d: -f1)"
[[ -n "$udp_marker_line" && -n "$udp_probe_line" && -n "$udp_result_line" \
  && -n "$udp_log_line" && "$udp_probe_line" -eq $((udp_marker_line + 1)) \
  && "$udp_probe_line" -lt "$udp_result_line" \
  && "$udp_result_line" -lt "$udp_log_line" ]] || {
  echo 'UDP negative gate must mark immediately before probe, then verify no response and exact rejection logs' >&2
  exit 1
}
grep -F 'enabled_accessibility_services' "$ACCEPTANCE" >/dev/null
# Runtime choice is now derived from plugin system declarations, not launch opinions.
grep -F 'systems: ["gba"]' "$ROOT/plugins/mgba/plugin.ts" >/dev/null
grep -F 'app: "@korri:retroarch/retroarch"' "$ROOT/plugins/mgba/plugin.ts" >/dev/null
grep -F 'id: "@korri:retroarch/retroarch"' "$ROOT/plugins/retroarch/plugin.ts" >/dev/null
python3 "$ROOT/services/korrid/test-minimum-config-fixtures.py"
library_focus_source="$(sed -n '/^focus_wario_in_installed_library() {/,/^}/p' "$ACCEPTANCE")"
# shellcheck disable=SC2016 # Literal source-contract needles.
nav_focus_line="$(grep -nF -- '"$SERIAL" "$KORRI_PACKAGE" --library' <<<"$library_focus_source" | head -1 | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
open_library_line="$(grep -nF 'shell input tap "$tap_x" "$tap_y"' <<<"$library_focus_source" | head -1 | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
verify_library_line="$(grep -nF -- '"$SERIAL" "$KORRI_PACKAGE" --verify-library' <<<"$library_focus_source" | head -1 | cut -d: -f1)"
traversal_line="$(grep -nF 'traverse_library_to_final_viewport' <<<"$library_focus_source" | head -1 | cut -d: -f1)"
strict_focus_line="$(grep -nF -- "--game 'local-game:01K4J6K8Y00000000000000002'" <<<"$library_focus_source" | head -1 | cut -d: -f1)"
[[ -n "$nav_focus_line" && -n "$open_library_line" && -n "$verify_library_line" \
  && -n "$traversal_line" && -n "$strict_focus_line" \
  && "$nav_focus_line" -lt "$open_library_line" \
  && "$open_library_line" -lt "$verify_library_line" \
  && "$verify_library_line" -lt "$traversal_line" \
  && "$traversal_line" -lt "$strict_focus_line" ]] || {
  echo 'RetroArch acceptance must focus Library, activate it, verify its view, traverse, then focus Wario' >&2
  exit 1
}
if grep -F 'KEYCODE_DPAD_RIGHT' <<<"$library_focus_source" >/dev/null; then
  echo 'RetroArch acceptance must not assume retained Home focus before opening Library' >&2
  exit 1
fi
[[ "$(grep -Fc 'shell input tap' <<<"$library_focus_source")" -eq 1 ]]
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'for _ in $(seq 1 20); do' <<<"$library_focus_source" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'timeout 1 "$DEBUG_PORTAL_FOCUS_GAME_SH"' <<<"$library_focus_source" >/dev/null
grep -F 'sleep 0.25' <<<"$library_focus_source" >/dev/null
grep -F 'library_verified=true' <<<"$library_focus_source" >/dev/null
grep -F 'library-view.last-diagnostic.txt' <<<"$library_focus_source" >/dev/null
grep -F "jq -e '.view == \"library\" and .verified == true'" \
  <<<"$library_focus_source" >/dev/null
# The bounded post-tap wait may only repeat the read-only verifier. It must not
# introduce another activation or route-changing RPC.
# shellcheck disable=SC2016 # Literal source-contract needle.
[[ "$(grep -Fc -- '"$SERIAL" "$KORRI_PACKAGE" --verify-library' <<<"$library_focus_source")" -eq 1 ]]
if grep -F 'rpc ' <<<"$library_focus_source" >/dev/null; then
  echo 'RetroArch Library transition polling must not invoke RPC' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F 'verified_library_system_edge_avoiding_point "$navigation_json"' \
  <<<"$library_focus_source" >/dev/null
grep -F 'wait_for_stable_library_tile "$navigation_observation"' \
  <<<"$library_focus_source" >/dev/null
grep -F 'library-view.post-tap.png' <<<"$library_focus_source" >/dev/null
grep -F 'exec-out screencap -p >"$library_failure_image"' \
  <<<"$library_focus_source" >/dev/null
grep -F 'Physical A activation is retained by the human unified-overlay gate' \
  <<<"$library_focus_source" >/dev/null
stable_library_source="$(sed -n '/^wait_for_stable_library_tile() {/,/^}/p' "$ACCEPTANCE")"
grep -F 'local max_attempts=10' <<<"$stable_library_source" >/dev/null
grep -F 'sleep 0.15' <<<"$stable_library_source" >/dev/null
grep -F -- '"$SERIAL" "$KORRI_PACKAGE" --library' \
  <<<"$stable_library_source" >/dev/null
grep -F 'library_tile_bounds_stable "$previous" "$current"' \
  <<<"$stable_library_source" >/dev/null
if grep -Eq 'input (tap|swipe)|rpc ' <<<"$stable_library_source"; then
  echo 'Library bounds stabilization must only repeat the trusted focus observation' >&2
  exit 1
fi
traversal_source="$(sed -n '/^traverse_library_to_final_viewport() {/,/^}/p' "$ACCEPTANCE")"
traversal_max="$(sed -n 's/^[[:space:]]*local max_steps=\([0-9][0-9]*\)$/\1/p' <<<"$traversal_source")"
[[ "$traversal_max" =~ ^[0-9]+$ && "$traversal_max" -ge 1 && "$traversal_max" -le 64 ]] || {
  echo 'RetroArch Library traversal must have an explicit bound no larger than 64' >&2
  exit 1
}
grep -F 'KEYCODE_DPAD_DOWN' <<<"$traversal_source" >/dev/null
grep -F 'sleep 0.15' <<<"$traversal_source" >/dev/null
if grep -Eq 'DEBUG_PORTAL_FOCUS_GAME_SH|input (tap|swipe)' <<<"$traversal_source"; then
  echo 'RetroArch Library traversal must use only bounded controller focus movement' >&2
  exit 1
fi
if grep -Eq 'assert_rgui_menu_visible|assert_rgui_selection_moves|compare -metric' "$ACCEPTANCE"; then
  echo 'RetroArch native menu acceptance must use authenticated status telemetry' >&2
  exit 1
fi
if grep -Eq 'input swipe|input tap[[:space:]]+[0-9]' "$ACCEPTANCE"; then
  echo 'RetroArch acceptance must not use swipes or hard-coded tap coordinates' >&2
  exit 1
fi
if grep -F 'launch_spec=' "$ACCEPTANCE" >/dev/null; then
  echo 'RetroArch acceptance must not speculatively launch through RPC before portal activation' >&2
  exit 1
fi
discovery_line="$(grep -nF 'discover_live_korri_authority ||' "$ACCEPTANCE" | head -1 | cut -d: -f1)"
pristine_line="$(grep -nF 'assert_pristine_gate_state' "$ACCEPTANCE" | tail -1 | cut -d: -f1)"
idle_line="$(grep -n '^assert_session_idle$' "$ACCEPTANCE" | head -1 | cut -d: -f1)"
mutation_line="$(grep -nF 'provision_checkpoint_files' "$ACCEPTANCE" | tail -1 | cut -d: -f1)"
[[ -n "$discovery_line" && -n "$pristine_line" && -n "$idle_line" && -n "$mutation_line" \
  && "$discovery_line" -lt "$mutation_line" \
  && "$pristine_line" -lt "$mutation_line" \
  && "$idle_line" -lt "$mutation_line" ]] || {
  echo 'RetroArch acceptance must prove live RPC, idle session, activities, processes, and foreground before mutation' >&2
  exit 1
}
if grep -F '.retroarch-route-check-backup-$$' "$ACCEPTANCE" >/dev/null; then
  echo 'RetroArch acceptance backup ownership must never be PID-derived' >&2
  exit 1
fi
if grep -Eq 'udp_unauthenticated.*\|\| true|rm -f /sdcard/korri-acceptance.png|pull /sdcard/korri-acceptance.png' "$ACCEPTANCE"; then
  echo 'RetroArch acceptance must preserve UDP errors and use local screenshot capture' >&2
  exit 1
fi
if grep -Eq 'pm (clear|grant|install|uninstall)| adb install' "$ACCEPTANCE"; then
  echo 'RetroArch acceptance must not mutate installed packages' >&2
  exit 1
fi
if sed '/^[[:space:]]*#/d' "$ACCEPTANCE" \
  | grep -Eq '(force-stop|am[[:space:]]+kill|kill[[:space:]]+[^-]).*(KORRI_PACKAGE|existing_korri_pid|com\.simonwjackson\.korri)'; then
  echo 'RetroArch acceptance must never stop or kill Korri after the user-owned grant' >&2
  exit 1
fi
discovery_source="$(sed -n '/^discover_live_korri_authority() {/,/^}/p' "$ACCEPTANCE")"
if grep -Eq 'logcat|KorridServer|KorriPortal|listening on 127' <<<"$discovery_source"; then
  echo 'RetroArch authority discovery must not depend on historical logcat' >&2
  exit 1
fi
grep -F 'KORRI_ANDROID_DEBUG_AUTHORITY_JSON' "$ACCEPTANCE" >/dev/null
# shellcheck disable=SC2016 # Literal source-contract needle.
grep -F -- '"$DEBUG_CAPABILITY_SH" "$SERIAL" "$KORRI_PACKAGE" --json' "$ACCEPTANCE" >/dev/null
if grep -Eq 'settings (put|delete) secure (enabled_accessibility_services|accessibility_enabled)' "$ACCEPTANCE"; then
  echo 'RetroArch acceptance must not modify Android accessibility settings' >&2
  exit 1
fi
quit_source="$(sed -n '/^STAGE="quit-stale"/,/^STAGE="restoration"/p' "$ACCEPTANCE")"
wait_stopped_line="$(grep -nF 'wait_stopped' <<<"$quit_source" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal source-contract needle.
stale_controls_line="$(grep -nF 'wait_old_launch_stale "$quit_launch_id"' <<<"$quit_source" | cut -d: -f1)"
remote_idle_line="$(grep -nF 'assert_session_idle' <<<"$quit_source" | cut -d: -f1)"
quiesced_line="$(grep -nF 'GATE_CURRENT_LAUNCH_QUIESCED=true' <<<"$quit_source" | cut -d: -f1)"
[[ -n "$wait_stopped_line" && -n "$stale_controls_line" && -n "$remote_idle_line" \
  && -n "$quiesced_line" && "$wait_stopped_line" -lt "$stale_controls_line" \
  && "$stale_controls_line" -lt "$remote_idle_line" \
  && "$remote_idle_line" -lt "$quiesced_line" ]] || {
  echo 'local Quit proof must use process stop and stale controls before remote idle precondition' >&2
  exit 1
}
if grep -F 'quit_session=' <<<"$quit_source" >/dev/null; then
  echo 'local Quit proof must not treat remote session status as local evidence' >&2
  exit 1
fi
stale_wait_source="$(sed -n '/^wait_old_launch_stale() {/,/^}/p' "$ACCEPTANCE")"
if [[ "$(grep -Fc '.outcome.payload.reason == "StaleSession"' <<<"$stale_wait_source")" -ne 2 ]] \
  || grep -F 'Unavailable' <<<"$stale_wait_source" >/dev/null; then
  echo 'local Quit proof must poll controls and invoke until both are exactly StaleSession' >&2
  exit 1
fi

TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

PUBLICATION_PARSER="$TMP/parse-local-publication.sh"
printf '%s\n' "$publication_parser_source" >"$PUBLICATION_PARSER"
# shellcheck source=/dev/null
source "$PUBLICATION_PARSER"
valid_publication='launchId=0123456789abcdef0123456789abcdef event=published gameId=01K4J6K8Y00000000000000002 package=com.korri.retroarch launcher=retroarch'
[[ "$(parse_local_publication "$valid_publication")" == \
  '0123456789abcdef0123456789abcdef' ]]
for invalid_publication in \
  '' \
  "$valid_publication"$'\n'"$valid_publication" \
  'launchId=ABCDEF6789abcdef0123456789abcdef event=published gameId=01K4J6K8Y00000000000000002 package=com.korri.retroarch launcher=retroarch' \
  'launchId=0123456789abcdef0123456789abcdef event=published gameId=other package=com.korri.retroarch launcher=retroarch' \
  'launchId=0123456789abcdef0123456789abcdef event=published gameId=01K4J6K8Y00000000000000002 package=com.other.retroarch launcher=retroarch'; do
  if parse_local_publication "$invalid_publication" >/dev/null; then
    echo "local publication parser accepted zero, duplicate, or malformed evidence" >&2
    exit 1
  fi
done

RESUME_ASSERTION="$TMP/assert-exact-wario-resume.sh"
printf '%s\n' "$resume_assertion_source" >"$RESUME_ASSERTION"
# shellcheck source=/dev/null
source "$RESUME_ASSERTION"
valid_resume='{"outcome":{"_tag":"Ok","payload":{"disposition":"resume","launchId":"0123456789abcdef0123456789abcdef","launcherId":"retroarch","context":{"gameId":"01K4J6K8Y00000000000000002","title":"Wario Land 4","contentCrc32":"d6141609","contributors":[{"kind":"launcher","id":"@korri:retroarch/retroarch"},{"kind":"runtime","id":"@korri:mgba/mgba"}],"executor":{"id":"retroarch-control","available":true},"foreground":{"kind":"component","packageName":"com.korri.retroarch","className":"com.retroarch.browser.retroactivity.RetroActivityFuture"}},"component":{"packageName":"com.korri.retroarch","className":"com.retroarch.browser.retroactivity.RetroActivityFuture"},"extras":{"ROM":"/storage/emulated/0/korri/roms/wl4.gba","LIBRETRO":"/data/data/com.korri.retroarch/cores/mgba_libretro_android.so","CONFIGFILE":"/storage/emulated/0/korri/retroarch.cfg"},"integrity":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}}}'
expected_resume_launch_id='0123456789abcdef0123456789abcdef'
assert_exact_wario_resume "$valid_resume" "$expected_resume_launch_id"
for mutation in \
  '.outcome.payload.disposition = "fresh"' \
  '.outcome.payload.launchId = "replacement"' \
  '.outcome.payload.context.contentCrc32 = "00000000"' \
  '.outcome.payload.component.packageName = "com.retroarch.aarch64"' \
  '.outcome.payload.extras.LIBRETRO = "/replacement/core.so"'; do
  invalid_resume="$(jq -c "$mutation" <<<"$valid_resume")"
  if assert_exact_wario_resume "$invalid_resume" "$expected_resume_launch_id"; then
    echo "exact Wario resume assertion accepted mutation: $mutation" >&2
    exit 1
  fi
done

ACTIVITY_PARSER="$TMP/activity-dump-parser.sh"
sed -n '/^activity_dump_has_resumed_component() {/,/^}/p' "$ACCEPTANCE" >"$ACTIVITY_PARSER"
sed -n '/^activity_dump_shell_instance_count() {/,/^}/p' "$ACCEPTANCE" >>"$ACTIVITY_PARSER"
sed -n '/^activity_dump_has_live_component() {/,/^}/p' "$ACCEPTANCE" >>"$ACTIVITY_PARSER"
# shellcheck source=/dev/null
source "$ACTIVITY_PARSER"
SHELL_COMPONENT='com.simonwjackson.korri.debug/com.limelight.KorriShellActivity'
GAME_COMPONENT='com.simonwjackson.korri.debug/com.limelight.Game'
FORK_COMPONENT='com.korri.retroarch/com.retroarch.browser.retroactivity.RetroActivityFuture'
STOCK_COMPONENT='com.retroarch.aarch64/com.retroarch.browser.retroactivity.RetroActivityFuture'
for resumed_marker in topResumedActivity mResumedActivity ResumedActivity; do
  activity_dump_has_resumed_component \
    "    $resumed_marker: ActivityRecord{778899 u0 $SHELL_COMPONENT t109}" \
    "$SHELL_COMPONENT"
done
if activity_dump_has_resumed_component \
  "    ResumedActivity: ActivityRecord{778899 u0 $GAME_COMPONENT t109}" \
  "$SHELL_COMPONENT"; then
  echo 'resumed activity parser accepted a component other than the exact Korri Shell activity' >&2
  exit 1
fi
single_shell_dump="
  topResumedActivity=ActivityRecord{778899 u0 $SHELL_COMPONENT t109}
    * Hist  #0: ActivityRecord{778899 u0 $SHELL_COMPONENT t109}
  ResumedActivity: ActivityRecord{778899 u0 $SHELL_COMPONENT t109}
"
[[ "$(activity_dump_shell_instance_count "$single_shell_dump" "$SHELL_COMPONENT")" == 1 ]]
duplicate_shell_dump="
    * Hist  #1: ActivityRecord{aabbcc u0 $SHELL_COMPONENT t109}
    * Hist  #0: ActivityRecord{778899 u0 $SHELL_COMPONENT t109}
"
[[ "$(activity_dump_shell_instance_count "$duplicate_shell_dump" "$SHELL_COMPONENT")" == 2 ]]
tombstone_shell_dump="
    * Hist  #1: ActivityRecord{aabbcc u0 $SHELL_COMPONENT t-1 f}
"
[[ "$(activity_dump_shell_instance_count "$tombstone_shell_dump" "$SHELL_COMPONENT")" == 0 ]]
# Attached task ids (t0+) are live even when the record is paused/history.
activity_dump_has_live_component \
  "    mLastPausedActivity: ActivityRecord{ac82bb6 u0 $GAME_COMPONENT t69}" \
  "$GAME_COMPONENT"
activity_dump_has_live_component \
  "    * Hist #0: ActivityRecord{aabbcc u0 $FORK_COMPONENT t0}" \
  'com.korri.retroarch/'
activity_dump_has_live_component \
  "    * Hist #0: ActivityRecord{ddeeff u0 $STOCK_COMPONENT t42}" \
  'com.retroarch.aarch64/'
# A resumed/top Game record is live even if a malformed summary says t-1.
activity_dump_has_live_component \
  "    topResumedActivity=ActivityRecord{112233 u0 $GAME_COMPONENT t-1 f}" \
  "$GAME_COMPONENT"
# Destroyed/finishing t-1 bookkeeping is not an attached task.
for component in "$GAME_COMPONENT" 'com.korri.retroarch/' 'com.retroarch.aarch64/'; do
  case "$component" in
    "$GAME_COMPONENT") record="$GAME_COMPONENT" ;;
    'com.korri.retroarch/') record="$FORK_COMPONENT" ;;
    'com.retroarch.aarch64/') record="$STOCK_COMPONENT" ;;
  esac
  if activity_dump_has_live_component \
    "    mLastPausedActivity: ActivityRecord{445566 u0 $record t-1 f}}" \
    "$component"; then
    echo "activity parser treated destroyed t-1 tombstone as live: $component" >&2
    exit 1
  fi
done

UDP_ASSERTION="$TMP/assert-udp-no-response.sh"
sed -n '/^assert_udp_no_response() {/,/^}/p' "$ACCEPTANCE" >"$UDP_ASSERTION"
# shellcheck source=/dev/null
source "$UDP_ASSERTION"
marker='korri-udp-probe-complete-review'
assert_udp_no_response "$marker" "$marker remote_nc_rc=0 remote_nc_output="
assert_udp_no_response "$marker" "$marker remote_nc_rc=124 remote_nc_output="
for rejected in \
  '' \
  'adb transport timeout' \
  "$marker remote_nc_rc=1 remote_nc_output=" \
  "$marker remote_nc_rc=125 remote_nc_output=" \
  "$marker remote_nc_rc=0 remote_nc_output=unexpected" \
  "$marker remote_nc_rc=124 remote_nc_output=unexpected"; do
  if assert_udp_no_response "$marker" "$rejected" >/dev/null 2>&1; then
    echo "UDP negative accepted an inexact remote result: $rejected" >&2
    exit 1
  fi
done

UDP_LOG_ASSERTION="$TMP/assert-udp-rejection-log.sh"
sed -n '/^assert_udp_rejection_log() {/,/^}/p' "$ACCEPTANCE" >"$UDP_LOG_ASSERTION"
# shellcheck source=/dev/null
source "$UDP_LOG_ASSERTION"
# Invoked indirectly by the sourced assertion helper.
# shellcheck disable=SC2329
logcat_since() {
  printf '%s\n' "$MOCK_UDP_LOGS"
}
# Invoked indirectly by the sourced assertion helper.
# shellcheck disable=SC2329
sleep() { :; }
MOCK_UDP_LOGS='[NetCMD] Rejected malformed Korri command.'
assert_udp_rejection_log 'korri-retroarch-acceptance-udp-negative-review'
for rejected_logs in \
  '' \
  $'[NetCMD] Rejected malformed Korri command.\n[NetCMD] Rejected malformed Korri command.' \
  $'[NetCMD] Rejected malformed Korri command.\n[NetCMD] Korri authenticated request accepted command=1.' \
  $'[NetCMD] Rejected malformed Korri command.\n[NetCMD] Korri authenticated reply attempted command=1 length=62.' \
  $'[NetCMD] Rejected malformed Korri command.\n[NetCMD] Korri authenticated reply sent command=1 length=62.'; do
  MOCK_UDP_LOGS="$rejected_logs"
  if assert_udp_rejection_log 'korri-retroarch-acceptance-udp-negative-review' \
      >/dev/null 2>&1; then
    echo 'UDP rejection evidence accepted duplicate, request, or reply logs' >&2
    exit 1
  fi
done
unset -f sleep logcat_since

LIBRARY_STABILITY="$TMP/library-stability.sh"
for stability_function in \
  exact_library_tile_observation \
  library_tile_bounds_stable \
  wait_for_stable_library_tile; do
  sed -n "/^${stability_function}() {/,/^}/p" "$ACCEPTANCE" \
    >>"$LIBRARY_STABILITY"
done
# shellcheck source=/dev/null
source "$LIBRARY_STABILITY"
MOCK_LIBRARY_HELPER="$TMP/mock-library-helper.sh"
cat >"$MOCK_LIBRARY_HELPER" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
index="$(cat "$MOCK_LIBRARY_STATE")"
index=$((index + 1))
printf '%s\n' "$index" >"$MOCK_LIBRARY_STATE"
cat "$MOCK_LIBRARY_FIXTURES/$index.json"
SH
chmod +x "$MOCK_LIBRARY_HELPER"
# These inputs are read by the shipping functions sourced from LIBRARY_STABILITY.
# shellcheck disable=SC2034
DEBUG_PORTAL_FOCUS_GAME_SH="$MOCK_LIBRARY_HELPER"
# shellcheck disable=SC2034
SERIAL=fixture-serial
# shellcheck disable=SC2034
KORRI_PACKAGE=fixture-package
library_fixture() {
  local path="$1"
  local left="$2"
  local top="$3"
  local width="$4"
  local height="$5"
  jq -cn \
    --argjson left "$left" --argjson top "$top" \
    --argjson width "$width" --argjson height "$height" \
    '{view:"home",part:"shift.cine-library-tile",title:"Library",
      focused:true,rectFinitePositive:true,fullyOnScreen:true,
      bounds:{left:$left,top:$top,width:$width,height:$height},
      viewport:{width:640,height:480}}' >"$path"
}
# Keep source delay evidence above while making fixture execution immediate.
sleep() { :; }

MOVING_FIXTURES="$TMP/moving-library-fixtures"
mkdir "$MOVING_FIXTURES"
library_fixture "$TMP/moving-library-initial.json" 400 350 52 78
library_fixture "$MOVING_FIXTURES/1.json" 403 351 53 79
library_fixture "$MOVING_FIXTURES/2.json" 405 352 54 80
library_fixture "$MOVING_FIXTURES/3.json" 405.8 352.6 54.4 80.7
MOCK_LIBRARY_STATE="$TMP/moving-library-state"
MOCK_LIBRARY_FIXTURES="$MOVING_FIXTURES"
export MOCK_LIBRARY_STATE MOCK_LIBRARY_FIXTURES
printf '0\n' >"$MOCK_LIBRARY_STATE"
stable_library_observation="$(wait_for_stable_library_tile \
  "$(cat "$TMP/moving-library-initial.json")")"
jq -e --slurpfile expected "$MOVING_FIXTURES/3.json" \
  '. == $expected[0]' <<<"$stable_library_observation" >/dev/null
[[ "$(cat "$MOCK_LIBRARY_STATE")" == 3 ]]

MOVING_FOREVER_FIXTURES="$TMP/moving-forever-library-fixtures"
mkdir "$MOVING_FOREVER_FIXTURES"
library_fixture "$TMP/moving-forever-library-initial.json" 400 350 52 78
for fixture_index in $(seq 1 10); do
  if ((fixture_index % 2 == 0)); then
    library_fixture "$MOVING_FOREVER_FIXTURES/$fixture_index.json" 406 350 52 78
  else
    library_fixture "$MOVING_FOREVER_FIXTURES/$fixture_index.json" 403 350 52 78
  fi
done
MOCK_LIBRARY_STATE="$TMP/moving-forever-library-state"
MOCK_LIBRARY_FIXTURES="$MOVING_FOREVER_FIXTURES"
export MOCK_LIBRARY_STATE MOCK_LIBRARY_FIXTURES
printf '0\n' >"$MOCK_LIBRARY_STATE"
if wait_for_stable_library_tile \
    "$(cat "$TMP/moving-forever-library-initial.json")" >/dev/null 2>&1; then
  echo 'Library bounds stabilization accepted a never-stable fixture sequence' >&2
  exit 1
fi
[[ "$(cat "$MOCK_LIBRARY_STATE")" == 10 ]]
unset -f sleep library_fixture

FOCUS_RENDERER="$TMP/focus-renderer.sh"
sed -n '/^render_focused_wario_crop_evidence() {/,/^}/p' "$ACCEPTANCE" >"$FOCUS_RENDERER"
# shellcheck source=/dev/null
source "$FOCUS_RENDERER"
CENTER_EXTRACTOR="$TMP/verified-element-center.sh"
sed -n '/^verified_element_center() {/,/^}/p' "$ACCEPTANCE" >"$CENTER_EXTRACTOR"
# shellcheck source=/dev/null
source "$CENTER_EXTRACTOR"
LIBRARY_POINT_EXTRACTOR="$TMP/verified-library-system-edge-avoiding-point.sh"
sed -n '/^verified_library_system_edge_avoiding_point() {/,/^}/p' \
  "$ACCEPTANCE" >"$LIBRARY_POINT_EXTRACTOR"
# shellcheck source=/dev/null
source "$LIBRARY_POINT_EXTRACTOR"
cat >"$TMP/focused-wario.svg" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" width="640" height="480">
  <rect width="640" height="480" fill="#050505"/>
  <!-- Shift uses an external CSS outline: keep every cyan pixel outside the
       exact getBoundingClientRect fixture below. -->
  <rect x="544" y="226" width="90" height="128" rx="12"
        fill="none" stroke="#32c7e6" stroke-width="4"/>
  <g transform="translate(548,230)">
    <rect width="82" height="120" rx="8" fill="#17343d"/>
    <text x="41" y="112" text-anchor="middle" font-family="DejaVu Sans" font-size="9" fill="white">Wario Land 4</text>
  </g>
</svg>
SVG
magick "$TMP/focused-wario.svg" "$TMP/focused-wario-full.png"
cat >"$TMP/focused-wario.json" <<'JSON'
{"gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":548,"top":230,"width":82,"height":120},"viewport":{"width":640,"height":480}}
JSON
[[ "$(verified_element_center "$TMP/focused-wario.json")" == '589 290' ]]
cat >"$TMP/focused-play.json" <<'JSON'
{"gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","label":"Play","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":342.5,"top":287.25,"width":130,"height":52},"viewport":{"width":640,"height":480}}
JSON
[[ "$(verified_element_center "$TMP/focused-play.json")" == '407 313' ]]
cat >"$TMP/focused-library.json" <<'JSON'
{"view":"home","part":"shift.cine-library-tile","title":"Library","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":407.25,"top":352.5,"width":56,"height":80},"viewport":{"width":640,"height":480}}
JSON
[[ "$(verified_element_center "$TMP/focused-library.json")" == '435 392' ]]
cat >"$TMP/measured-rg405m-library.json" <<'JSON'
{"view":"home","part":"shift.cine-library-tile","title":"Library","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":407.2203063964844,"top":352.7406311035156,"width":54.59063720703125,"height":81.89999389648438},"viewport":{"width":640,"height":480}}
JSON
read -r library_x library_y < <(
  verified_library_system_edge_avoiding_point "$TMP/measured-rg405m-library.json"
)
[[ "$library_x $library_y" == '434 373' ]]
jq -e --argjson x "$library_x" --argjson y "$library_y" '
  .bounds as $bounds | .viewport as $viewport
  | ($bounds.top + ($bounds.height / 2) | floor) as $center_y
  | $x >= $bounds.left and $x < ($bounds.left + $bounds.width)
  and $y >= $bounds.top and $y < ($bounds.top + $bounds.height)
  and $x >= 0 and $x < $viewport.width
  and $y >= 0 and $y < $viewport.height
  and $y < $center_y
' "$TMP/measured-rg405m-library.json" >/dev/null
[[ "$library_x" =~ ^[0-9]+$ && "$library_y" =~ ^[0-9]+$ ]]
for invalid_library in \
  '{"view":"home","part":"shift.cine-settings-tile","title":"Library","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":407.2203063964844,"top":352.7406311035156,"width":54.59063720703125,"height":81.89999389648438},"viewport":{"width":640,"height":480}}' \
  '{"view":"home","part":"shift.cine-library-tile","title":"Library","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":407.2203063964844,"top":352.7406311035156,"width":54.59063720703125,"height":81.89999389648438},"viewport":{"width":640,"height":400}}'; do
  printf '%s\n' "$invalid_library" >"$TMP/invalid-library-point.json"
  if verified_library_system_edge_avoiding_point \
      "$TMP/invalid-library-point.json" >/dev/null 2>&1; then
    echo "unsafe or inexact Library bounds produced an edge-avoiding point: $invalid_library" >&2
    exit 1
  fi
done
for invalid in \
  '{"gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":-1,"top":230,"width":82,"height":120},"viewport":{"width":640,"height":480}}' \
  '{"gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":600,"top":230,"width":82,"height":120},"viewport":{"width":640,"height":480}}' \
  '{"gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":548,"top":470,"width":82,"height":120},"viewport":{"width":640,"height":480}}' \
  '{"gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":548,"top":230,"width":0,"height":120},"viewport":{"width":640,"height":480}}' \
  '{"gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","focused":true,"rectFinitePositive":true,"fullyOnScreen":true,"bounds":{"left":1e999,"top":230,"width":82,"height":120},"viewport":{"width":640,"height":480}}'; do
  printf '%s\n' "$invalid" >"$TMP/invalid-center.json"
  if verified_element_center "$TMP/invalid-center.json" >/dev/null 2>&1; then
    echo "unsafe focused-element bounds produced a tap center: $invalid" >&2
    exit 1
  fi
done
# The title is intentionally too small for reliable full-frame OCR. Exact DOM
# bounds must drive the crop and scale before OCR rather than false-failing.
tesseract "$TMP/focused-wario-full.png" stdout --psm 6 >"$TMP/full-frame.txt" 2>/dev/null
if grep -Eqi 'wario[[:space:]]+land[[:space:]]+4' "$TMP/full-frame.txt"; then
  echo 'deterministic focused-Wario fixture unexpectedly became full-frame OCR-readable' >&2
  exit 1
fi
render_focused_wario_crop_evidence \
  "$TMP/focused-wario-full.png" "$TMP/focused-wario.json" \
  "$TMP/focused-wario-crop.png" "$TMP/focused-wario-crop-4x.png" \
  "$TMP/focused-wario-crop.txt" "$TMP/focused-wario-observation.json"
jq -e '
  .crop == {x:538,y:220,width:102,height:140}
  and .focusedElement == {x:548,y:230,width:82,height:120}
  and .focusOutlinePaddedCropRatio >= 0.01
  and .activeElementVerified == true
  and .ocrTitle == "Wario Land 4"
' "$TMP/focused-wario-observation.json" >/dev/null
grep -Eqi 'wario[[:space:]]+land[[:space:]]+4' "$TMP/focused-wario-crop.txt"
[[ -s "$TMP/focused-wario-crop.png" && -s "$TMP/focused-wario-crop-4x.png" \
  && -s "$TMP/focused-wario-crop.png.focus-element.png" ]]
# The fixture's focus outline is wholly external to getBoundingClientRect. The
# exact element artifact must contain no qualifying cyan pixels, proving that
# the passing evidence came from the bounds-derived padded crop.
element_outline_ratio="$(magick "$TMP/focused-wario-crop.png.focus-element.png" \
  -alpha off -colorspace sRGB \
  -fx 'g > 0.65 && b > 0.75 && r < 0.4 ? 1 : 0' \
  -format '%[fx:mean]' info:)"
awk -v ratio="$element_outline_ratio" 'BEGIN { exit !(ratio + 0 < 0.001) }'

printf 'RetroArch acceptance config-safety contract passed\n'
