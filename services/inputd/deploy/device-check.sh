#!/usr/bin/env bash
set -Eeuo pipefail

# Explicit-target, reversible gate for the first Linux InputPlumber rollout.
# Remote argv is always single-quoted by ssh_transport. No caller value is
# sent as unquoted remote shell text.

SSH_BIN="${KORRI_DEVICE_GATE_SSH:-ssh}"
POLL_ATTEMPTS="${KORRI_DEVICE_GATE_POLL_ATTEMPTS:-40}"
POLL_DELAY="${KORRI_DEVICE_GATE_POLL_DELAY:-0.25}"
REMOTE_COMMAND_TIMEOUT="${KORRI_DEVICE_GATE_REMOTE_TIMEOUT:-570}"
LOCK_WAIT_TIMEOUT="${KORRI_DEVICE_GATE_LOCK_WAIT_TIMEOUT:-575}"
LOCAL_SSH_TIMEOUT="${KORRI_DEVICE_GATE_SSH_TIMEOUT:-1170}"
HITL_READ_TIMEOUT="${KORRI_DEVICE_GATE_HITL_READ_TIMEOUT:-300}"
HITL_OVERALL_TIMEOUT="${KORRI_DEVICE_GATE_HITL_OVERALL_TIMEOUT:-2400}"
ATTEMPT_TIMEOUT="${KORRI_DEVICE_GATE_ATTEMPT_TIMEOUT:-4800}"
UNHEALTHY_OBSERVE_SECONDS=60
GATE_LOCK='/run/lock/korri-device-gate.lock'
ATTEMPT_MARKER='/var/lib/korri-device-gate/attempt'
ATTEMPT_UNIT='korri-device-gate-attempt.service'
NORMALIZED_NAME='Microsoft X-Box 360 pad'
SUPPORTED_PRODUCTION_PROFILE='korri-60-xbox_one_gamepad.yaml'
OLD_USER_UNITS=(korrid.service sunshine.service x11-headless.service)
PREDICATE_SYSTEM_UNITS=(korrid.service sunshine.service x11-headless.service korri-compositor.service)
SUNSHINE_UINPUT_GROUP='korri-sunshine-uinput'
COMPOSITOR_CONTROL_DIRECTORY='/run/korri-compositor'
COMPOSITOR_CONTROL_SOCKET='/run/korri-compositor/sway-ipc.sock'
COMPOSITOR_WAYLAND_ALIAS='korri-wayland'
COMPOSITOR_OUTPUT='HEADLESS-1'
EXPECTED_SUNSHINE_FORMAT='1'
EXPECTED_SUNSHINE_BASE_VERSION='2025.924.154138'
EXPECTED_SUNSHINE_BASE_SOURCE_HASH='sha256-QrPfZqd9pgufohUjxlTpO6V0v7B41UrXHZaESsFjZ48='
EXPECTED_SUNSHINE_BASE_DERIVATION='/nix/store/5hmg1ff3wjkzcmspssckdlydk0d5bbjz-sunshine-2025.924.154138.drv'
EXPECTED_SUNSHINE_LIBAVCODEC_VERSION='62.11.100'
EXPECTED_SUNSHINE_FFMPEG_COMMIT='61c50407fd429a5e2ec616e2e846c3fe3743879a'
EXPECTED_SUNSHINE_FFMPEG_SOURCE_HASH='sha256-LKQUfHb9/Z4uvPx4vrtAOPL95Un9/C26lvCbQZ51avk='
EXPECTED_SUNSHINE_NVENC_API='12.0'
EXPECTED_SUNSHINE_PATCH_SET_SHA256='e330647511163d07c252901908e7c29c435b36459246201162be58bb2eb007d5'
KORRID_CONTROL_GROUP='korri-control'
KORRID_CONTROL_PEER_USER='korri-inputd'
KORRID_CONTROL_SOCKET='/run/korrid-control/control.sock'
KORRID_CONTROL_SOCKET_UNIT='korrid-control.socket'
# This path comes from HostSessionControl's existing private-state producer.
KORRID_HOST_SESSION_ROOT='/var/lib/korrid/host-session'
BUNDLE_SELECTOR_ROOT='/nix/var/nix/gcroots/korri-bundle'
EXPECTED_KEYS='304,305,307,308,310,311,314,315,316,317,318,704,705,706,707'
EXPECTED_ABS='0,1,2,3,4,5,16,17'
LEDGER_PROOF_HELPER=''


expected_sunshine_patch_manifest() {
  cat <<'EOF'
patch=0001-add-runtime-settings-protocol-surface.patch sha256=8a9522e39de85cb4ea7c0558a806780ae39d588555c7a84c600a56b9fdbe3bd4
patch=0002-wire-runtime-settings-control-plane.patch sha256=dd9b7283dd2cbcb2476571bfcf61702b00dba428422d309e31e7b4c839db41be
patch=0003-apply-runtime-bitrate-and-fps-changes.patch sha256=d7d89d4a8b4b06d2c473f4c2156a17ecfe369f805132e90a2d05197e69e7e01d
patch=0004-add-proof-gated-runtime-resolution-apply-path.patch sha256=599d3db14ea57e9712148e83fd7f0404dba96c5c40506c3209c5dbaa7778646e
patch=0005-add-seamless-vaapi-runtime-bitrate-path.patch sha256=a14ca9d556728ca1a4fcb14ae338a6275c9b28c52598a82a4e4f424956154d53
patch=0010-extend-runtime-resolution-fresh-idr-window.patch sha256=86252208da87bff0b61623f7da86e50d9f35c19963910e5e30703b72b86a42eb
patch=0012-persist-runtime-config-and-reinit-capture-after-resolution.patch sha256=2ac28eb76da2d02aa97812e9708094480cc1b7c4b897cf123772c24f16c493c6
patch=0013-request-async-capture-reinit-after-runtime-resolution.patch sha256=0831530081f9551173ff1a74a5ca2771942e9c519ec476c27548a1d3cbea3fa2
patch=0014-skip-runtime-vaapi-destructor-flush.patch sha256=59eedaf576f99223bd807205c45b12b1ac5f9850225614530b4ab925e3204e50
patch=0015-add-korri-input-seat-event-mirror.patch sha256=c0eab65a69c17b2f3f5b6c9c69fabaaf51519da26a81f9e57dbd714ab5ca92db
patch=0016-add-seamless-nvenc-runtime-path.patch sha256=686decb81379741e01e0b9b0e9105bbe23765a1bf728565767604383983a7074
patch=0017-use-wayland-ram-capture-for-cuda.patch sha256=a87aefc6eb5f71a4d413d751eefb87743745a2fab126dded5b66b23b949f66b2
patch=0018-vectorize-wayland-bgr888-with-swscale.patch sha256=753971f16e33598215caa455074f3bbca23e43b0cf2a2b8a97779f356486203f
patch=0019-use-pinned-memory-for-cuda-capture.patch sha256=83fd586d210668b06753fdd8bb6312967ba8805ce7a2fdc992c5cbdc49c79c88
patch=0020-add-korrid-certificate-control.patch sha256=5ab5b2b5a464c4839f18aa1fc0f304b42b5b8c941fbc4ad5605b34eeaff525e0
EOF
}

fail() {
  printf 'device gate: %s\n' "$*" >&2
  exit 1
}

valid_generation_path() {
  [[ "$1" =~ ^/nix/store/[0-9a-df-np-sv-z]{32}-nixos-system-[A-Za-z0-9+._?=-]+$ ]]
}

remote_wait_unit() {
  local unit="$1" expected_status="${2:-}" active status attempt
  for ((attempt = 1; attempt <= POLL_ATTEMPTS; attempt++)); do
    active="$(systemctl show "$unit" -p ActiveState --value 2>/dev/null || true)"
    status="$(systemctl show "$unit" -p StatusText --value 2>/dev/null || true)"
    if [[ "$active" == active && ( -z "$expected_status" || "$status" == "$expected_status" ) ]]; then
      return 0
    fi
    sleep "$POLL_DELAY"
  done
  printf 'timed out waiting for %s ActiveState=active StatusText=%s\n' "$unit" "${expected_status:-<any>}" >&2
  return 1
}

remote_generation() { realpath -e /run/current-system; }
remote_user_systemctl() {
  local runtime_user="$1" uid
  shift
  uid="$(id -u "$runtime_user")" || fail 'runtime user is unavailable'
  [[ "$uid" =~ ^[1-9][0-9]*$ ]] || fail 'runtime user UID is invalid'
  sudo -n -u "$runtime_user" env \
    XDG_RUNTIME_DIR="/run/user/$uid" \
    DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" \
    systemctl --user "$@"
}

remote_system_unit_active() {
  local unit="$1" state
  state="$(systemctl show "$unit" -p ActiveState --value)" || return 1
  case "$state" in
    active) printf 'true\n' ;;
    inactive|failed) printf 'false\n' ;;
    *) printf 'unexpected system unit ActiveState for %s: %s\n' "$unit" "${state:-<empty>}" >&2; return 1 ;;
  esac
}

remote_user_unit_active() {
  local runtime_user="$1" unit="$2" state
  state="$(remote_user_systemctl "$runtime_user" show "$unit" -p ActiveState --value)" || return 1
  case "$state" in
    active) printf 'true\n' ;;
    inactive) printf 'false\n' ;;
    *) printf 'unexpected user unit ActiveState for %s: %s\n' "$unit" "${state:-<empty>}" >&2; return 1 ;;
  esac
}

remote_user_unit_enabled() {
  local runtime_user="$1" unit="$2" snapshot load state
  snapshot="$(remote_user_systemctl "$runtime_user" show "$unit" -p LoadState -p UnitFileState)" \
    || return 1
  load="$(awk -F= '$1 == "LoadState" { print substr($0, index($0, "=") + 1) }' <<<"$snapshot")"
  state="$(awk -F= '$1 == "UnitFileState" { print substr($0, index($0, "=") + 1) }' <<<"$snapshot")"
  [[ "$(grep -c '^LoadState=' <<<"$snapshot")" -eq 1 \
    && "$(grep -c '^UnitFileState=' <<<"$snapshot")" -eq 1 ]] \
    || { printf 'incomplete user unit enablement state for %s\n' "$unit" >&2; return 1; }
  case "$load:$state" in
    loaded:enabled) printf 'true\n' ;;
    loaded:disabled|loaded:static|loaded:masked|loaded:masked-runtime|masked:masked|masked:masked-runtime|not-found:)
      printf 'false\n'
      ;;
    *)
      printf 'unexpected user unit enablement state for %s: LoadState=%s UnitFileState=%s\n' \
        "$unit" "${load:-<empty>}" "${state:-<empty>}" >&2
      return 1
      ;;
  esac
}

remote_control_socket_present() {
  [[ -S "$KORRID_CONTROL_SOCKET" ]]
}

remote_private_session_state_absent() {
  local mode entries
  [[ ! -L "$KORRID_HOST_SESSION_ROOT" ]] || return 1
  [[ -e "$KORRID_HOST_SESSION_ROOT" ]] || return 0
  [[ -d "$KORRID_HOST_SESSION_ROOT" ]] || return 1
  mode="$(stat -Lc '%a' -- "$KORRID_HOST_SESSION_ROOT" 2>/dev/null)" || return 1
  [[ "$mode" == 700 ]] || return 1
  entries="$(find "$KORRID_HOST_SESSION_ROOT" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null)" \
    || return 1
  [[ -z "$entries" ]]
}

# One-off clean cut from the obsolete launch-id atom. Call this only after
# every launch authority and its activation socket are stopped and systemd
# proves that no game is active.
remote_remove_obsolete_launch_id_atom() {
  local mode entries atom
  [[ ! -L "$KORRID_HOST_SESSION_ROOT" ]] || return 1
  [[ -e "$KORRID_HOST_SESSION_ROOT" ]] || return 0
  [[ -d "$KORRID_HOST_SESSION_ROOT" ]] || return 1
  mode="$(stat -Lc '%a' -- "$KORRID_HOST_SESSION_ROOT" 2>/dev/null)" || return 1
  [[ "$mode" == 700 ]] || return 1
  entries="$(find "$KORRID_HOST_SESSION_ROOT" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null)" \
    || return 1
  [[ -n "$entries" ]] || return 0
  [[ "$entries" == launch-id ]] || return 1
  atom="$KORRID_HOST_SESSION_ROOT/launch-id"
  [[ ! -L "$atom" && -f "$atom" ]] || return 1
  [[ "$(stat -Lc '%a' -- "$atom" 2>/dev/null)" == 600 ]] || return 1
  [[ "$(cat -- "$atom" 2>/dev/null)" =~ ^[0-9a-f]{32}$ ]] || return 1
  rm -- "$atom" || return 1
  sync -f "$KORRID_HOST_SESSION_ROOT" || return 1
}

remote_refuse_active_game() {
  local runtime_user="$1" response phase code live_units proof_source=local-state
  remote_launch_authority_quiesced "$runtime_user" \
    || fail 'korrid launch authority is not quiesced; refusing private-state cut'
  if remote_control_socket_present; then
    if response="$(sudo -n -u "$KORRID_CONTROL_PEER_USER" curl --fail --silent --connect-timeout 1 --max-time 2 \
      --unix-socket "$KORRID_CONTROL_SOCKET" http://localhost/rpc \
      -H 'content-type: application/json' -d '{"_tag":"app.session.status","payload":{}}' 2>/dev/null)"; then
      phase="$(jq -r 'if ._tag == "app.session.status" and .outcome._tag == "Ok" then (.outcome.payload.active.phase // "none") else "none" end' <<<"$response" 2>/dev/null)" \
        || fail 'exact local game status is invalid; refusing service mutation'
      case "${phase,,}" in
        running|stopping) fail "exact local game status is ${phase}; refusing service mutation" ;;
        none)
          code="$(jq -r 'if ._tag == "app.session.status" and .outcome._tag == "Err" then .outcome.payload.code else "" end' <<<"$response" 2>/dev/null || true)"
          if [[ "$(jq -r 'if ._tag == "app.session.status" and .outcome._tag == "Ok" and (.outcome.payload.active // null) == null then "safe" else "" end' <<<"$response" 2>/dev/null || true)" != safe \
            && "$code" != NoActiveSession && "$code" != SessionCompleted ]]; then
            fail 'exact local game status cannot prove that no game is active; refusing service mutation'
          fi
          proof_source=rpc
          ;;
        *) fail 'exact local game status has an unknown phase; refusing service mutation' ;;
      esac
    fi
  fi
  live_units="$(systemctl list-units --type=service --state=activating,active,reloading,deactivating \
    --no-legend --plain 'korri-game-*.service' 2>/dev/null)" \
    || fail 'Korri game unit state is unavailable; refusing service mutation'
  [[ -z "$live_units" ]] || fail 'a Korri game unit is live; refusing service mutation'
  live_units="$(remote_user_systemctl "$runtime_user" list-units --type=service \
    --state=activating,active,reloading,deactivating --no-legend --plain 'korri-game-*.service' 2>/dev/null)" \
    || fail 'user Korri game unit state is unavailable; refusing service mutation'
  [[ -z "$live_units" ]] || fail 'a user Korri game unit is live; refusing service mutation'
  remote_remove_obsolete_launch_id_atom \
    || fail 'obsolete private launch state cannot be removed safely; refusing service mutation'
  remote_private_session_state_absent \
    || fail 'private launch state is not empty after inactivity proof; refusing service mutation'
  printf 'active-game-check=clear source=%s\n' "$proof_source"
}

remote_bundle_selector_service_loaded() {
  local load
  load="$(systemctl show korri-bundle-selector.service -p LoadState --value 2>/dev/null)" \
    || fail 'bundle selector service state is unavailable'
  case "$load" in
    loaded) return 0 ;;
    not-found) return 1 ;;
    *) fail "bundle selector service has an unexpected load state: ${load:-<empty>}" ;;
  esac
}

clear_bundle_selector_root() {
  local root="$1" expected_uid="$2" expected_gid="$3"
  local metadata entries name selector target
  [[ ! -L "$root" ]] || fail 'orphan bundle selector root is a symbolic link'
  if [[ ! -e "$root" ]]; then
    printf 'bundle-selector=absent\n'
    return 0
  fi
  [[ -d "$root" ]] || fail 'orphan bundle selector root is not a directory'
  metadata="$(stat -Lc '%u:%g:%a' -- "$root" 2>/dev/null)" \
    || fail 'orphan bundle selector root metadata is unavailable'
  [[ "$metadata" == "$expected_uid:$expected_gid:700" \
    || "$metadata" == "$expected_uid:$expected_gid:711" ]] \
    || fail 'orphan bundle selector root ownership or mode is invalid'
  entries="$(find "$root" -mindepth 1 -maxdepth 1 -printf '%f\n' 2>/dev/null)" \
    || fail 'orphan bundle selector entries are unavailable'
  while IFS= read -r name; do
    [[ -z "$name" ]] && continue
    case "$name" in
      active|previous) ;;
      *) fail "orphan bundle selector contains an unexpected entry: $name" ;;
    esac
  done <<<"$entries"
  for name in active previous; do
    selector="$root/$name"
    [[ -e "$selector" || -L "$selector" ]] || continue
    [[ -L "$selector" ]] \
      || fail "orphan bundle selector entry is not a symbolic link: $name"
    target="$(readlink -- "$selector")" \
      || fail "orphan bundle selector target is unavailable: $name"
    [[ "$target" =~ ^/nix/store/[0-9a-df-np-sv-z]{32}-korri-bundle-[A-Za-z0-9+._?=-]+$ ]] \
      || fail "orphan bundle selector target is invalid: $name"
    rm -- "$selector" || return $?
  done
  rmdir -- "$root" || return $?
  sync -f "${root%/*}" || return $?
  printf 'bundle-selector=cleared-orphan\n'
}

remote_clear_orphan_bundle_selector() {
  local unit
  remote_bundle_selector_service_loaded && return 0
  for unit in korri-inputd.service korrid.service x11-headless.service korri-compositor.service sunshine.service; do
    ! systemctl is-active --quiet "$unit" \
      || fail "orphan bundle selector cleanup found an active candidate service: $unit"
  done
  clear_bundle_selector_root "$BUNDLE_SELECTOR_ROOT" 0 0
}

remote_generation_has_bundle_selector() {
  local generation="$1"
  [[ -f "$generation/etc/systemd/system/korri-bundle-selector.service" ]]
}

remote_generation_bundle() {
  local generation="$1" unit
  local -a bundles
  unit="$generation/etc/systemd/system/korri-bundle-selector.service"
  remote_generation_has_bundle_selector "$generation" \
    || fail 'generation bundle selector unit is absent'
  mapfile -t bundles < <(
    grep -oE '/nix/store/[0-9a-df-np-sv-z]{32}-korri-bundle-[A-Za-z0-9+._?=-]+' "$unit" \
      | LC_ALL=C sort -u
  )
  [[ "${#bundles[@]}" -eq 1 ]] || fail 'generation does not declare exactly one Korri bundle'
  printf '%s\n' "${bundles[0]}"
}

remote_switch_generation_bundle() {
  local generation="$1" bundle selector systemctl_bin
  bundle="$(remote_generation_bundle "$generation")" \
    || fail 'generation bundle could not be resolved'
  selector="$(readlink -f -- "$generation/sw/bin/korri-bundle-select" 2>/dev/null)" \
    || fail 'generation bundle selector executable is unavailable'
  systemctl_bin="$(readlink -f -- "$generation/sw/bin/systemctl" 2>/dev/null)" \
    || fail 'generation systemctl executable is unavailable'
  [[ "$selector" == /nix/store/*/bin/korri-bundle-select && -x "$selector" ]] \
    || fail 'generation bundle selector executable is not immutable'
  [[ "$systemctl_bin" == /nix/store/*/bin/systemctl && -x "$systemctl_bin" ]] \
    || fail 'generation systemctl executable is not immutable'
  sudo -n "$selector" switch "$bundle" "$systemctl_bin"
  [[ "$(readlink -f -- "$BUNDLE_SELECTOR_ROOT/active" 2>/dev/null)" == "$bundle" ]] \
    || fail 'active Korri bundle does not match the requested generation'
}

remote_current_launch_authority() {
  local runtime_user="$1" system_present=false user_active
  if systemctl is-active --quiet korrid.service \
    || systemctl is-active --quiet "$KORRID_CONTROL_SOCKET_UNIT"; then
    system_present=true
  fi
  user_active="$(remote_user_unit_active "$runtime_user" korrid.service)" \
    || fail 'old user korrid active state is unavailable before quiesce'
  if [[ "$system_present" == true && "$user_active" == true ]]; then
    fail 'system and user korrid launch authorities are active together'
  elif [[ "$system_present" == true ]]; then
    printf 'system\n'
  elif [[ "$user_active" == true ]]; then
    printf 'user\n'
  else
    printf 'none\n'
  fi
}

remote_quiesce_launch_authority() {
  local runtime_user="$1" authority state
  authority="$(remote_current_launch_authority "$runtime_user")" || return $?
  # Stop socket activation before the system service. Otherwise the status
  # probe itself can restart korrid between the inactivity proof and the cut.
  systemctl stop "$KORRID_CONTROL_SOCKET_UNIT" >/dev/null 2>&1 || true
  ! systemctl is-active --quiet "$KORRID_CONTROL_SOCKET_UNIT" \
    || fail 'system korrid activation socket remained active'
  systemctl stop korrid.service >/dev/null 2>&1 || true
  ! systemctl is-active --quiet korrid.service \
    || fail 'system korrid launch authority remained active'
  remote_user_systemctl "$runtime_user" stop korrid.service >/dev/null 2>&1 || true
  state="$(remote_user_unit_active "$runtime_user" korrid.service)" \
    || fail 'old user korrid active state is unavailable after quiesce'
  [[ "$state" == false ]] || fail 'old user korrid launch authority remained active'
  printf 'launch-authority=%s quiesced\n' "$authority"
}

remote_launch_authority_quiesced() {
  local runtime_user="$1" state unit
  for unit in "$KORRID_CONTROL_SOCKET_UNIT" korrid.service; do
    state="$(remote_system_unit_active "$unit")" || return 1
    [[ "$state" == false ]] || return 1
  done
  state="$(remote_user_unit_active "$runtime_user" korrid.service)" || return 1
  [[ "$state" == false ]]
}

remote_restore_launch_authority() {
  local runtime_user="$1" socket_was_active="$2" system_was_active="$3" user_was_active="$4"
  if [[ "$socket_was_active" == true ]]; then
    systemctl start "$KORRID_CONTROL_SOCKET_UNIT"
  else
    systemctl stop "$KORRID_CONTROL_SOCKET_UNIT" >/dev/null 2>&1 || true
  fi
  if [[ "$system_was_active" == true ]]; then
    systemctl start korrid.service
  else
    systemctl stop korrid.service >/dev/null 2>&1 || true
  fi
  remote_restore_old_user_unit_activity "$runtime_user" korrid.service "$user_was_active"
}

# Preparation refusal is not a generation rollback: rollback may hit the same
# invalid selector. Keep restoration scoped through every pre-activation step,
# without replacing the caller's trap or restarting old authority after handoff.
remote_quiesce_for_private_state_cut() {
  (
    local runtime_user="$1" preparation="${2:?}" rollback_has_bundle_selector="${3:-false}"
    local socket_was_active system_was_active user_was_active sunshine_was_active x11_was_active
    socket_was_active="$(remote_system_unit_active "$KORRID_CONTROL_SOCKET_UNIT")" \
      || fail 'system korrid socket activity is unavailable before private-state cut'
    system_was_active="$(remote_system_unit_active korrid.service)" \
      || fail 'system korrid activity is unavailable before private-state cut'
    user_was_active="$(remote_user_unit_active "$runtime_user" korrid.service)" \
      || fail 'old user korrid active state is unavailable before private-state cut'
    sunshine_was_active="$(remote_user_unit_active "$runtime_user" sunshine.service)" \
      || fail 'old user Sunshine activity is unavailable before private-state cut'
    x11_was_active="$(remote_user_unit_active "$runtime_user" x11-headless.service)" \
      || fail 'old user X11 activity is unavailable before private-state cut'
    trap 'remote_restore_old_user_unit_activity "$runtime_user" x11-headless.service "$x11_was_active"
      remote_restore_old_user_unit_activity "$runtime_user" sunshine.service "$sunshine_was_active"
      remote_restore_launch_authority "$runtime_user" "$socket_was_active" "$system_was_active" "$user_was_active"' EXIT
    remote_quiesce_launch_authority "$runtime_user" || return $?
    remote_refuse_active_game "$runtime_user" || return $?
    case "$preparation" in
      candidate)
        remote_clear_orphan_bundle_selector || return $?
        remote_quiesce_old_user_units "$runtime_user" || return $?
        remote_set_pairing_state_modes "$runtime_user" 700 600 >/dev/null || return $? ;;
      rollback)
        remote_stop_candidate_services || return $?
        remote_restore_raw_joystick_udev || return $?
        if [[ "$rollback_has_bundle_selector" == false ]]; then
          systemctl stop korri-bundle-selector.service >/dev/null 2>&1 || true
          clear_bundle_selector_root "$BUNDLE_SELECTOR_ROOT" 0 0 || return $?
        fi ;;
      health-failure) ;;
      *) fail "unknown private-state preparation: $preparation" ;;
    esac
    # The caller now owns activation (including partial activation failure),
    # profile mutation, or intentional health-failure injection.
    trap - EXIT
  )
}

remote_quiesce_old_user_units() {
  local runtime_user="$1" unit state
  for unit in "${OLD_USER_UNITS[@]}"; do
    remote_user_systemctl "$runtime_user" stop "$unit" >/dev/null 2>&1 || true
    state="$(remote_user_unit_active "$runtime_user" "$unit")" \
      || fail "old user unit active state query failed after stop: $unit"
    [[ "$state" == false ]] || fail "old user unit remained active: $unit"
  done
}

remote_disable_old_user_units() {
  local runtime_user="$1" unit state
  remote_user_systemctl "$runtime_user" daemon-reload
  for unit in "${OLD_USER_UNITS[@]}"; do
    remote_user_systemctl "$runtime_user" disable "$unit" >/dev/null 2>&1 || true
    state="$(remote_user_unit_active "$runtime_user" "$unit")" \
      || fail "old user unit active state query failed after candidate activation: $unit"
    [[ "$state" == false ]] || fail "old user unit became active during candidate activation: $unit"
    state="$(remote_user_unit_enabled "$runtime_user" "$unit")" \
      || fail "old user unit enablement query failed after disable: $unit"
    [[ "$state" == false ]] || fail "old user unit remained enabled: $unit"
  done
}

remote_restore_old_user_unit_enablement() {
  local runtime_user="$1" unit="$2" was_enabled="$3" state
  if [[ "$was_enabled" == true ]]; then
    remote_user_systemctl "$runtime_user" enable "$unit" >/dev/null
  else
    remote_user_systemctl "$runtime_user" disable "$unit" >/dev/null 2>&1 || true
  fi
  state="$(remote_user_unit_enabled "$runtime_user" "$unit")" \
    || fail "old user unit enablement query failed during restore: $unit"
  [[ "$state" == "$was_enabled" ]] || fail "old user unit enabled state was not restored: $unit"
}

remote_restore_old_user_unit_activity() {
  local runtime_user="$1" unit="$2" was_active="$3" state
  if [[ "$was_active" == true ]]; then
    remote_user_systemctl "$runtime_user" start "$unit"
  else
    remote_user_systemctl "$runtime_user" stop "$unit" >/dev/null 2>&1 || true
  fi
  state="$(remote_user_unit_active "$runtime_user" "$unit")" \
    || fail "old user unit active state query failed during restore: $unit"
  [[ "$state" == "$was_active" ]] || fail "old user unit active state was not restored: $unit"
}

remote_restore_old_user_units() {
  local runtime_user="$1"
  local korrid_active="$2" korrid_enabled="$3" sunshine_active="$4" sunshine_enabled="$5"
  local x11_active="$6" x11_enabled="$7"
  remote_user_systemctl "$runtime_user" daemon-reload
  remote_restore_old_user_unit_enablement "$runtime_user" x11-headless.service "$x11_enabled"
  remote_restore_old_user_unit_enablement "$runtime_user" sunshine.service "$sunshine_enabled"
  remote_restore_old_user_unit_enablement "$runtime_user" korrid.service "$korrid_enabled"
  remote_restore_old_user_unit_activity "$runtime_user" x11-headless.service "$x11_active"
  remote_restore_old_user_unit_activity "$runtime_user" sunshine.service "$sunshine_active"
  remote_restore_old_user_unit_activity "$runtime_user" korrid.service "$korrid_active"
}

# Print the exact set bits in a Linux sysfs capability bitmap. Sysfs prints
# machine words from highest to lowest. A short zero word still occupies one
# complete native-word offset, so whitespace must never be discarded.
remote_bitmap_codes() {
  local path="$1" word_bits="${2:-}" word_hex_digits word_index=0 word nibble value bit nibble_index code
  local -a words
  [[ -n "$word_bits" ]] || word_bits="$(getconf LONG_BIT)"
  [[ "$word_bits" == 32 || "$word_bits" == 64 ]] || return 1
  word_hex_digits=$((word_bits / 4))
  read -r -a words <"$path" || return 1
  [[ "${#words[@]}" -gt 0 ]] || return 1
  for ((word_index = 0; word_index < ${#words[@]}; word_index++)); do
    word="${words[${#words[@]} - 1 - word_index]}"
    [[ "$word" =~ ^[0-9a-fA-F]{1,$word_hex_digits}$ ]] || return 1
    for ((nibble_index = 0; nibble_index < ${#word}; nibble_index++)); do
      nibble="${word:${#word}-1-nibble_index:1}"
      case "$nibble" in
        [0-9]) value=$((10#$nibble)) ;;
        [a-f]) value=$((10 + $(printf '%d' "'$nibble") - 97)) ;;
        [A-F]) value=$((10 + $(printf '%d' "'$nibble") - 65)) ;;
        *) return 1 ;;
      esac
      for bit in 0 1 2 3; do
        if ((value & (1 << bit))); then
          code=$((word_index * word_bits + nibble_index * 4 + bit))
          printf '%s\n' "$code"
        fi
      done
    done
  done
}

remote_normalized_fingerprint() {
  local event node name phys uniq bustype vendor product version sysfs dev_sys dev_stat
  local keys axes ff ff_codes exe ip_version props fingerprint count=0
  shopt -s nullglob
  for event in /sys/class/input/event*; do
    [[ -r "$event/device/name" ]] || continue
    name="$(<"$event/device/name")"
    [[ "$name" == "$NORMALIZED_NAME" ]] || continue
    node="/dev/input/${event##*/}"
    phys="$(cat "$event/device/phys" 2>/dev/null || true)"
    uniq="$(cat "$event/device/uniq" 2>/dev/null || true)"
    bustype="$(cat "$event/device/id/bustype" 2>/dev/null || true)"
    vendor="$(cat "$event/device/id/vendor" 2>/dev/null || true)"
    product="$(cat "$event/device/id/product" 2>/dev/null || true)"
    version="$(cat "$event/device/id/version" 2>/dev/null || true)"
    sysfs="$(realpath -e -- "$event" 2>/dev/null || true)"
    [[ "$bustype:$vendor:$product:$version" == '0003:045e:028e:0001' ]] || continue
    [[ -z "$phys" && -z "$uniq" && "$sysfs" == /sys/devices/virtual/input/input*/event* ]] || continue
    props="$(udevadm info --query=property --name="$node" 2>/dev/null || true)"
    grep -Fx 'ID_INPUT_JOYSTICK=1' <<<"$props" >/dev/null || continue
    keys="$(remote_bitmap_codes "$event/device/capabilities/key" | paste -sd, -)"
    axes="$(remote_bitmap_codes "$event/device/capabilities/abs" | paste -sd, -)"
    ff_codes="$(remote_bitmap_codes "$event/device/capabilities/ff")" || continue
    ff="${ff_codes%%$'\n'*}"
    [[ "$keys" == "$EXPECTED_KEYS" && "$axes" == "$EXPECTED_ABS" && -n "$ff" ]] || continue
    dev_sys="$(<"$event/dev")"
    dev_stat="$(stat -Lc '%t:%T' "$node" 2>/dev/null || true)"
    [[ "$dev_sys" == "$((16#${dev_stat%:*})):$((16#${dev_stat#*:}))" ]] || continue
    exe="$(readlink -f /proc/"$(systemctl show inputplumber.service -p MainPID --value)"/exe 2>/dev/null || true)"
    [[ "$exe" =~ ^/nix/store/[0-9a-df-np-sv-z]{32}-inputplumber-korri-0\.75\.2/bin/inputplumber$ && -x "$exe" ]] || continue
    ip_version="$("$exe" --version 2>/dev/null || true)"
    [[ "$ip_version" =~ (^|[[:space:]])0\.75\.2($|[[:space:]]) ]] || continue
    fingerprint="node=$node sysfs=$sysfs dev=$dev_sys inode=$(stat -Lc '%d:%i' "$node") inputplumber=$exe version=0.75.2 keys=$keys abs=$axes ff=yes"
    printf '%s\n' "$fingerprint"
    count=$((count + 1))
  done
  [[ "$count" -eq 1 ]]
}

remote_controller_candidates() {
  local event node name identity properties sysfs dev_sys dev_stat
  shopt -s nullglob
  for event in /sys/class/input/event*; do
    node="/dev/input/${event##*/}"
    [[ -r "$event/device/name" && -e "$node" ]] || continue
    name="$(<"$event/device/name")"
    [[ "$name" != "$NORMALIZED_NAME" && "$name" != 'Korri U7 Synthetic Controller' ]] || continue
    properties="$(udevadm info --query=property --name="$node" 2>/dev/null || true)"
    grep -Fx 'ID_INPUT_JOYSTICK=1' <<<"$properties" >/dev/null || continue
    sysfs="$(realpath -e -- "$event" 2>/dev/null || true)"
    [[ "$sysfs" == /sys/devices/* && "$sysfs" != /sys/devices/virtual/* ]] || continue
    dev_sys="$(<"$event/dev")"
    dev_stat="$(stat -Lc '%t:%T' "$node" 2>/dev/null || true)"
    [[ "$dev_stat" =~ ^[0-9a-fA-F]+:[0-9a-fA-F]+$ ]] || continue
    [[ "$dev_sys" == "$((16#${dev_stat%:*})):$((16#${dev_stat#*:}))" ]] || continue
    identity="$(cat "$event/device/id/bustype" 2>/dev/null || true):$(cat "$event/device/id/vendor" 2>/dev/null || true):$(cat "$event/device/id/product" 2>/dev/null || true):$(cat "$event/device/id/version" 2>/dev/null || true)"
    [[ "${identity,,}" =~ ^[0-9a-f]{4}:[0-9a-f]{4}:[0-9a-f]{4}:[0-9a-f]{4}$ ]] || continue
    printf 'controller-candidate identity=%s name=%q sysfs=%s event=%s\n' \
      "${identity,,}" "$name" "$sysfs" "${event##*/}"
  done
}

remote_dbus_unique_owner() {
  local name="$1" reply
  [[ "$name" =~ ^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)+$ ]] || return 1
  reply="$(busctl --system call org.freedesktop.DBus /org/freedesktop/DBus \
    org.freedesktop.DBus GetNameOwner s "$name" 2>/dev/null)" || return 1
  [[ "$reply" =~ ^s\ \"(:[0-9]+\.[0-9]+)\"$ ]] || return 1
  printf '%s\n' "${BASH_REMATCH[1]}"
}

remote_profile_selects_event() {
  local event_name="$1" profile="$2" exe expected_path object profile_property sources_property
  exe="$(readlink -f /proc/"$(systemctl show inputplumber.service -p MainPID --value)"/exe 2>/dev/null || true)"
  [[ "$exe" =~ ^/nix/store/[0-9a-df-np-sv-z]{32}-inputplumber-korri-0\.75\.2/bin/inputplumber$ ]] || return 1
  expected_path="${exe%/bin/inputplumber}/share/inputplumber/profiles/$profile"
  [[ -f "$expected_path" ]] || return 1
  while IFS= read -r object; do
    [[ "$object" == /org/shadowblip/InputPlumber/CompositeDevice* ]] || continue
    profile_property="$(busctl --system get-property org.shadowblip.InputPlumber "$object" org.shadowblip.Input.CompositeDevice ProfilePath 2>/dev/null || true)"
    [[ "$profile_property" == "s \"$expected_path\"" ]] || continue
    sources_property="$(busctl --system get-property org.shadowblip.InputPlumber "$object" org.shadowblip.Input.CompositeDevice SourceDevicePaths 2>/dev/null || true)"
    grep -F "\"/dev/input/$event_name\"" <<<"$sources_property" >/dev/null && return 0
  done < <(busctl --system tree org.shadowblip.InputPlumber --list --no-pager 2>/dev/null || true)
  return 1
}

remote_resolve_physical_controller_node() {
  local original="$1" isolated="$2" source node
  if [[ -e "$original" && -e "$isolated" ]]; then
    return 1
  elif [[ -e "$original" ]]; then
    source=direct
    node="$original"
  elif [[ -e "$isolated" ]]; then
    source=isolated
    node="$isolated"
  else
    return 1
  fi
  [[ -c "$node" && ! -L "$node" ]] || return 1
  printf '%s|%s\n' "$source" "$node"
}

remote_physical_controller_evidence() {
  local expected_identity="$1" profile="$2" require_profile="${3:-true}"
  local event original_node isolated_node resolved source node name identity properties sysfs dev_sys dev_stat count=0
  [[ "$profile" == "$SUPPORTED_PRODUCTION_PROFILE" ]] || return 1
  shopt -s nullglob
  for event in /sys/class/input/event*; do
    [[ -r "$event/device/name" ]] || continue
    name="$(<"$event/device/name")"
    [[ "$name" != "$NORMALIZED_NAME" && "$name" != 'Korri U7 Synthetic Controller' ]] || continue
    identity="$(cat "$event/device/id/bustype" 2>/dev/null || true):$(cat "$event/device/id/vendor" 2>/dev/null || true):$(cat "$event/device/id/product" 2>/dev/null || true):$(cat "$event/device/id/version" 2>/dev/null || true)"
    [[ "${identity,,}" == "$expected_identity" ]] || continue
    sysfs="$(realpath -e -- "$event" 2>/dev/null || true)"
    [[ "$sysfs" == /sys/devices/* && "$sysfs" != /sys/devices/virtual/* ]] || continue
    properties="$(udevadm info --query=property --path="$sysfs" 2>/dev/null || true)"
    grep -Fx 'ID_INPUT_JOYSTICK=1' <<<"$properties" >/dev/null || continue
    original_node="/dev/input/${event##*/}"
    isolated_node="/dev/inputplumber/sources/${event##*/}"
    resolved="$(remote_resolve_physical_controller_node "$original_node" "$isolated_node")" || continue
    source="${resolved%%|*}"
    node="${resolved#*|}"
    dev_sys="$(<"$event/dev")"
    dev_stat="$(stat -Lc '%t:%T' "$node" 2>/dev/null || true)"
    [[ "$dev_stat" =~ ^[0-9a-fA-F]+:[0-9a-fA-F]+$ ]] || continue
    [[ "$dev_sys" == "$((16#${dev_stat%:*})):$((16#${dev_stat#*:}))" ]] || continue
    if [[ "$require_profile" == true ]]; then
      remote_profile_selects_event "${event##*/}" "$profile" || continue
    fi
    printf 'identity=%s event=%s sysfs=%s profile=%s source=%s\n' \
      "$expected_identity" "${event##*/}" "$sysfs" \
      "$([[ "$require_profile" == true ]] && printf '%s' "$profile" || printf pending-candidate)" "$source"
    count=$((count + 1))
  done
  [[ "$count" -eq 1 ]]
}

remote_temporary_artifacts_dirty() {
  local event name
  local -a private_paths
  shopt -s nullglob
  for event in /sys/class/input/event*; do
    [[ -r "$event/device/name" ]] || continue
    name="$(<"$event/device/name")"
    [[ "$name" == 'Korri U7 Synthetic Controller' ]] && return 0
  done
  private_paths=(/run/korri-u7-device-gate.* /tmp/korri-u7-device-gate.*)
  ((${#private_paths[@]} == 0)) || return 0
  return 1
}

remote_unit_value() {
  local scope="$1" runtime_user="$2" unit="$3" property="$4"
  if [[ "$scope" == user ]]; then
    remote_user_systemctl "$runtime_user" show "$unit" -p "$property" --value
  else
    systemctl show "$unit" -p "$property" --value
  fi
}

remote_korrid_api_port() {
  local environment token address='' port count=0
  local -a tokens
  environment="$(systemctl show korrid.service -p Environment --value 2>/dev/null)" || return 1
  read -r -a tokens <<<"$environment"
  for token in "${tokens[@]}"; do
    case "$token" in
      KORRID_ADDRESS=*)
        address="${token#KORRID_ADDRESS=}"
        count=$((count + 1))
        ;;
    esac
  done
  [[ "$count" -eq 1 ]] || return 1
  [[ "$address" =~ ^(0\.0\.0\.0|127\.0\.0\.1):([1-9][0-9]{0,4})$ ]] || return 1
  port="${BASH_REMATCH[2]}"
  ((10#$port <= 65535)) || return 1
  printf '%s\n' "$port"
}

remote_catalog_health() {
  local port
  port="$(remote_korrid_api_port)" || { printf unavailable; return 0; }
  curl --fail --silent --connect-timeout 1 --max-time 2 "http://127.0.0.1:$port/rpc" \
    -H 'content-type: application/json' -d '{"_tag":"app.catalog.snapshot","payload":{}}' \
    | jq -r 'if .outcome._tag == "Ok" then "Ok" else "unhealthy" end' 2>/dev/null || printf unavailable
}

remote_pairing_state_modes() {
  local runtime_user="$1" uid home home_real config_tree config_dir state_file
  local config_real state_real config_stat state_stat config_mode state_mode
  uid="$(id -u "$runtime_user")" || return 1
  home="$(getent passwd "$runtime_user" | cut -d: -f6)" || return 1
  [[ -n "$home" ]] || return 1
  home_real="$(realpath -e -- "$home" 2>/dev/null)" || return 1
  config_tree="$home_real/.config"
  [[ "$(realpath -e -- "$home/.config" 2>/dev/null)" == "$config_tree" ]] || return 1
  config_dir="$home/.config/sunshine"
  state_file="$config_dir/sunshine_state.json"
  config_stat="$(stat -c '%F:%u:%a' -- "$config_dir" 2>/dev/null)" || return 1
  state_stat="$(stat -c '%F:%u:%a' -- "$state_file" 2>/dev/null)" || return 1
  [[ "$config_stat" == "directory:$uid:"* && "$state_stat" == regular*"file:$uid:"* ]] || return 1
  config_mode="${config_stat##*:}"
  state_mode="${state_stat##*:}"
  [[ "$config_mode" =~ ^[0-7]{3,4}$ && "$state_mode" =~ ^[0-7]{3,4}$ ]] || return 1
  config_real="$(realpath -e -- "$config_dir" 2>/dev/null)" || return 1
  state_real="$(realpath -e -- "$state_file" 2>/dev/null)" || return 1
  [[ "$config_real" == "$config_tree/"* && "$state_real" == "$config_real/sunshine_state.json" ]] || return 1
  printf '%s:%s\n' "$config_mode" "$state_mode"
}

remote_pairing_state_present() {
  local modes config_mode state_mode
  modes="$(remote_pairing_state_modes "$1")" || return 1
  config_mode="${modes%%:*}"
  state_mode="${modes#*:}"
  (( (8#$config_mode & 8#077) == 0 && (8#$state_mode & 8#077) == 0 ))
}

remote_sunshine_private_state_digest() {
  local runtime_user="$1" uid home home_real self_real helper digest
  uid="$(id -u "$runtime_user")" || return 1
  home="$(getent passwd "$runtime_user" | cut -d: -f6)" || return 1
  home_real="$(realpath -e -- "$home" 2>/dev/null)" || return 1
  self_real="$(readlink -f -- "$0" 2>/dev/null)" || return 1
  if [[ "$self_real" != /nix/store/*     && "${KORRI_DEVICE_GATE_TEST_PRIVATE_DIGEST:-}" =~ ^[0-9a-f]{64}$ ]]; then
    printf '%s\n' "$KORRI_DEVICE_GATE_TEST_PRIVATE_DIGEST"
    return 0
  fi
  helper="${self_real%/*}/korri-sunshine-state-digest"
  [[ "$helper" == /nix/store/*/bin/korri-sunshine-state-digest && -x "$helper" ]] || return 1
  digest="$("$helper" "$home_real" "$uid" 2>/dev/null)" || return 1
  [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s\n' "$digest"
}

remote_set_pairing_state_modes() {
  local runtime_user="$1" config_mode="$2" state_mode="$3" before after home
  [[ "$config_mode" =~ ^[0-7]{3,4}$ && "$state_mode" =~ ^[0-7]{3,4}$ ]] || return 1
  before="$(remote_pairing_state_modes "$runtime_user")" || return 1
  home="$(getent passwd "$runtime_user" | cut -d: -f6)" || return 1
  chmod "$config_mode" -- "$home/.config/sunshine" || return $?
  chmod "$state_mode" -- "$home/.config/sunshine/sunshine_state.json" || return $?
  after="$(remote_pairing_state_modes "$runtime_user")" || return 1
  [[ "$after" == "$config_mode:$state_mode" ]] || return 1
  printf 'pairing-modes=%s->%s\n' "$before" "$after"
}

remote_group_gid() {
  local name="$1" gid
  gid="$(getent group "$name" | cut -d: -f3)" || return 1
  [[ "$gid" =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "$gid"
}

remote_pid_has_supplementary_gid() {
  local pid="$1" expected_gid="$2" groups
  groups="$(awk 'BEGIN { found=0 } /^Groups:/ { found=1; for (i=2; i<=NF; i++) print $i } END { exit found ? 0 : 1 }' "/proc/$pid/status" 2>/dev/null)" \
    || return 2
  grep -Fx "$expected_gid" <<<"$groups" >/dev/null
}

remote_pid_reject_supplementary_gid() {
  local unit="$1" pid="$2" gid="$3" label="$4" status
  if remote_pid_has_supplementary_gid "$pid" "$gid"; then
    fail "$label supplementary group leaked to $unit"
  else
    status=$?
    [[ "$status" -eq 1 ]] || fail "could not inspect supplementary groups for $unit"
  fi
}

REMOTE_SERVICE_PID=''
REMOTE_SERVICE_GID=''
remote_service_credentials() {
  local unit="$1" declared_user declared_group pid uid gid expected_uid expected_gid fragment
  declared_user="$(systemctl show "$unit" -p User --value)"
  declared_group="$(systemctl show "$unit" -p Group --value)"
  pid="$(systemctl show "$unit" -p MainPID --value)"
  fragment="$(realpath -e -- "$(systemctl show "$unit" -p FragmentPath --value)" 2>/dev/null || true)"
  [[ -n "$declared_user" && "$pid" =~ ^[1-9][0-9]*$ && "$fragment" == /nix/store/* ]] \
    || fail "candidate system service lacks declarative service-specific credentials: $unit"
  expected_uid="$(id -u "$declared_user")" || fail "candidate system service user is unavailable: $unit"
  if [[ -n "$declared_group" ]]; then
    expected_gid="$(getent group "$declared_group" | cut -d: -f3)"
  else
    expected_gid="$(getent passwd "$declared_user" | cut -d: -f4)"
  fi
  uid="$(awk '/^Uid:/ {print $2}' "/proc/$pid/status" 2>/dev/null || true)"
  gid="$(awk '/^Gid:/ {print $2}' "/proc/$pid/status" 2>/dev/null || true)"
  [[ "$uid" == "$expected_uid" && -n "$expected_gid" && "$gid" == "$expected_gid" ]] \
    || fail "candidate system service process credentials do not match its unit: $unit"
  REMOTE_SERVICE_PID="$pid"
  REMOTE_SERVICE_GID="$gid"
  printf 'service-credentials unit=%s user=%s primary-gid=%s\n' "$unit" "$declared_user" "$expected_gid"
}

remote_process_group_policy() {
  local unit="$1" pid="$2" primary_gid="$3" input_gid="$4" uinput_gid="$5" control_gid="$6" sunshine_gid="$7"
  [[ "$primary_gid" != "$input_gid" ]] || fail "forbidden input primary group leaked to $unit"
  remote_pid_reject_supplementary_gid "$unit" "$pid" "$input_gid" 'forbidden input'
  if [[ -n "$uinput_gid" ]]; then
    [[ "$primary_gid" != "$uinput_gid" ]] || fail "forbidden uinput primary group leaked to $unit"
    remote_pid_reject_supplementary_gid "$unit" "$pid" "$uinput_gid" 'forbidden uinput'
  fi
  if [[ "$unit" == korri-inputd.service ]]; then
    [[ "$primary_gid" == "$control_gid" ]] || fail 'inputd does not use the control primary group'
  else
    [[ "$primary_gid" != "$control_gid" ]] || fail "control primary group leaked to $unit"
    remote_pid_reject_supplementary_gid "$unit" "$pid" "$control_gid" 'forbidden control'
  fi
  if [[ "$unit" == sunshine.service ]]; then
    [[ "$primary_gid" != "$sunshine_gid" ]] || fail 'Sunshine dedicated uinput group must be supplementary'
    remote_pid_has_supplementary_gid "$pid" "$sunshine_gid" \
      || fail 'system Sunshine lacks its dedicated uinput group'
  else
    [[ "$primary_gid" != "$sunshine_gid" ]] || fail "dedicated Sunshine uinput primary group leaked to $unit"
    remote_pid_reject_supplementary_gid "$unit" "$pid" "$sunshine_gid" 'dedicated Sunshine uinput'
  fi
}

REMOTE_SUNSHINE_PACKAGE_ROOT=''
remote_resolve_sunshine_executable() {
  local running="$1" declared="$2" package_root wrapper_link expected
  REMOTE_SUNSHINE_PACKAGE_ROOT=''
  [[ "$running" == /nix/store/*/* && "$running" == "$(readlink -f -- "$running" 2>/dev/null || true)" \
    && -f "$running" && -x "$running" && ! -L "$running" ]] || return 1
  [[ "$declared" == /nix/store/*/bin/sunshine && -f "$declared" && -x "$declared" && ! -L "$declared" ]] \
    || return 1
  package_root="${declared%/bin/sunshine}"
  [[ "$package_root" == "$(readlink -f -- "$package_root" 2>/dev/null || true)" ]] || return 1
  wrapper_link="$package_root/bin/.sunshine-wrapped"
  [[ -L "$wrapper_link" ]] || return 1
  grep -F "\"$wrapper_link\"" "$declared" >/dev/null || return 1
  expected="$(readlink -f -- "$wrapper_link" 2>/dev/null || true)"
  [[ "$expected" == "$package_root"/bin/sunshine-* && "$running" == "$expected" \
    && -f "$expected" && -x "$expected" && ! -L "$expected" ]] || return 1
  REMOTE_SUNSHINE_PACKAGE_ROOT="$package_root"
}

remote_sunshine_package_provenance() {
  local pid running declared execstart package_root provenance patch_set computed_patch_set
  local field expected_value
  local -a sunshine_execs=()
  pid="$(systemctl show sunshine.service -p MainPID --value 2>/dev/null || true)"
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || fail 'Sunshine MainPID is unavailable'
  running="$(readlink -f -- "/proc/$pid/exe" 2>/dev/null || true)"

  execstart="$(systemctl show sunshine.service -p ExecStart --value 2>/dev/null || true)"
  mapfile -t sunshine_execs < <(grep -oE '/nix/store/[^ ;{}"]+/bin/sunshine' <<<"$execstart" | sort -u)
  [[ "${#sunshine_execs[@]}" -eq 1 ]] \
    || fail 'Sunshine unit does not declare one exact store executable'
  declared="${sunshine_execs[0]}"
  remote_resolve_sunshine_executable "$running" "$declared" \
    || fail 'running Sunshine executable differs from the candidate unit'
  package_root="$REMOTE_SUNSHINE_PACKAGE_ROOT"

  [[ "${package_root##*/}" == *sunshine-korri* ]] \
    || fail 'candidate Sunshine package is not sunshine-korri'
  provenance="$package_root/share/korri/sunshine-korri/provenance"
  [[ -f "$provenance" && ! -L "$provenance" \
    && "$(stat -c '%u:%g:%a' -- "$provenance" 2>/dev/null)" == 0:0:444 ]] \
    || fail 'sunshine-korri provenance is absent or mutable'

  while IFS='|' read -r field expected_value; do
    [[ "$(grep -c "^${field}=" "$provenance")" -eq 1 \
      && "$(grep "^${field}=" "$provenance")" == "${field}=${expected_value}" ]] \
      || fail "sunshine-korri provenance has an invalid ${field} field"
  done <<EOF
format|$EXPECTED_SUNSHINE_FORMAT
package|sunshine-korri
base_sunshine_version|$EXPECTED_SUNSHINE_BASE_VERSION
approved_base_sunshine_source_hash|$EXPECTED_SUNSHINE_BASE_SOURCE_HASH
base_sunshine_derivation|$EXPECTED_SUNSHINE_BASE_DERIVATION
approved_base_sunshine_derivation|$EXPECTED_SUNSHINE_BASE_DERIVATION
reviewed_libavcodec_version|$EXPECTED_SUNSHINE_LIBAVCODEC_VERSION
reviewed_ffmpeg_commit|$EXPECTED_SUNSHINE_FFMPEG_COMMIT
reviewed_ffmpeg_source_hash|$EXPECTED_SUNSHINE_FFMPEG_SOURCE_HASH
reviewed_nvenc_api|$EXPECTED_SUNSHINE_NVENC_API
executable|bin/sunshine
patch_set_sha256|$EXPECTED_SUNSHINE_PATCH_SET_SHA256
EOF

  cmp -s <(grep '^patch=' "$provenance") <(expected_sunshine_patch_manifest) \
    || fail 'sunshine-korri provenance does not match the approved ordered patch manifest'
  patch_set="$(sed -n 's/^patch_set_sha256=//p' "$provenance")"
  computed_patch_set="$(grep '^patch=' "$provenance" \
    | sed -E 's/^patch=([^ ]+) sha256=([0-9a-f]{64})$/\1 \2/' \
    | sha256sum | cut -d' ' -f1)"
  [[ "$computed_patch_set" == "$EXPECTED_SUNSHINE_PATCH_SET_SHA256" \
    && "$computed_patch_set" == "$patch_set" ]] \
    || fail 'sunshine-korri provenance ordered patch digest is invalid'

  printf 'sunshine-executable=%s patch-set-sha256=%s patches=15 base-version=%s libavcodec=%s\n' \
    "$running" "$patch_set" "$EXPECTED_SUNSHINE_BASE_VERSION" "$EXPECTED_SUNSHINE_LIBAVCODEC_VERSION"
}

remote_candidate_credentials() {
  local runtime_user="$1" uid manager_pid manager_primary_gid unit name pid primary_gid game_units
  local input_gid uinput_gid control_gid sunshine_gid runtime_home sunshine_private environment inaccessible
  uid="$(id -u "$runtime_user")" || fail 'runtime user is unavailable'
  runtime_home="$(getent passwd "$runtime_user" | cut -d: -f6)" || fail 'runtime home is unavailable'
  sunshine_private="$runtime_home/.config/sunshine"
  environment="$(systemctl show korrid.service -p Environment --value 2>/dev/null || true)"
  [[ " $environment " == *" KORRID_SUNSHINE_PRIVATE_STATE_ROOT=$sunshine_private "* ]] \
    || fail 'korrid lacks the exact Sunshine private-state game isolation path'
  input_gid="$(remote_group_gid input)" || fail 'input group is unavailable'
  # A removed legacy uinput group has no credentials to leak. If it remains,
  # prove that no candidate or game process retains its numeric authority.
  uinput_gid="$(remote_group_gid uinput 2>/dev/null || true)"
  control_gid="$(remote_group_gid "$KORRID_CONTROL_GROUP")" || fail 'control group is unavailable'
  sunshine_gid="$(remote_group_gid "$SUNSHINE_UINPUT_GROUP")" || fail 'Sunshine uinput group is unavailable'
  manager_pid="$(systemctl show "user@$uid.service" -p MainPID --value 2>/dev/null || true)"
  [[ "$manager_pid" =~ ^[1-9][0-9]*$ && -r "/proc/$manager_pid/status" ]] \
    || fail 'fresh runtime user manager is unavailable'
  manager_primary_gid="$(awk '/^Gid:/ {print $2}' "/proc/$manager_pid/status" 2>/dev/null)" \
    || fail 'fresh runtime user manager credentials are unavailable'
  [[ "$manager_primary_gid" =~ ^[0-9]+$ ]] \
    || fail 'fresh runtime user manager credentials are invalid'
  for name in input uinput "$KORRID_CONTROL_GROUP" "$SUNSHINE_UINPUT_GROUP"; do
    ! id -nG "$runtime_user" | tr ' ' '\n' | grep -Fx "$name" >/dev/null \
      || fail "runtime user retains forbidden group: $name"
  done
  remote_process_group_policy 'runtime user manager' "$manager_pid" "$manager_primary_gid" \
    "$input_gid" "$uinput_gid" "$control_gid" "$sunshine_gid"
  for unit in korrid.service korri-compositor.service sunshine.service korri-inputd.service; do
    remote_service_credentials "$unit"
    remote_process_group_policy "$unit" "$REMOTE_SERVICE_PID" "$REMOTE_SERVICE_GID" \
      "$input_gid" "$uinput_gid" "$control_gid" "$sunshine_gid"
  done
  game_units="$(systemctl list-units --type=service --state=activating,active,reloading,deactivating \
    --no-legend --plain 'korri-game-*.service')" || fail 'Korri game unit credentials are unavailable'
  while read -r unit _; do
    [[ -n "$unit" ]] || continue
    pid="$(systemctl show "$unit" -p MainPID --value)" || fail "game unit PID is unavailable: $unit"
    primary_gid="$(awk '/^Gid:/ {print $2}' "/proc/$pid/status" 2>/dev/null)" \
      || fail "game unit process credentials are unavailable: $unit"
    [[ "$pid" =~ ^[1-9][0-9]*$ && "$primary_gid" =~ ^[0-9]+$ ]] \
      || fail "game unit process credentials are invalid: $unit"
    inaccessible="$(systemctl show "$unit" -p InaccessiblePaths --value 2>/dev/null || true)"
    [[ " $inaccessible " == *" $sunshine_private "* ]] \
      || fail "Sunshine private state is visible to game unit: $unit"
    [[ " $inaccessible " == *" $COMPOSITOR_CONTROL_DIRECTORY "* ]] \
      || fail "compositor control is visible to game unit: $unit"
    remote_process_group_policy "$unit" "$pid" "$primary_gid" \
      "$input_gid" "$uinput_gid" "$control_gid" "$sunshine_gid"
  done <<<"$game_units"
  printf 'candidate-credentials=pass runtime-broad-groups=none service-groups=least-privilege sunshine-uinput=exclusive\n'
}

remote_namespace_test() {
  local pid="$1" include_pid_namespace="$2" nsenter
  shift 2
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || fail 'namespace inspection PID is invalid'
  [[ "$include_pid_namespace" == yes || "$include_pid_namespace" == no ]] \
    || fail 'namespace inspection mode is invalid'
  nsenter="$(readlink -f -- "$(command -v nsenter)" 2>/dev/null)" \
    || fail 'namespace inspection helper is unavailable'
  [[ "$nsenter" == /nix/store/*/bin/nsenter && -x "$nsenter" ]] \
    || fail 'namespace inspection helper is not immutable'
  if [[ "$include_pid_namespace" == yes ]]; then
    "$nsenter" --mount="/proc/$pid/ns/mnt" --pid="/proc/$pid/ns/pid" -- "$@"
  else
    "$nsenter" --mount="/proc/$pid/ns/mnt" -- "$@"
  fi
}

remote_namespace_first_extra_x11_socket() {
  local pid="$1" find_helper
  find_helper="$(readlink -f -- "$(command -v find)" 2>/dev/null)" \
    || fail 'Xwayland socket enumeration helper is unavailable'
  [[ "$find_helper" == /nix/store/*/bin/find && -x "$find_helper" ]] \
    || fail 'Xwayland socket enumeration helper is not immutable'
  remote_namespace_test "$pid" no "$find_helper" /tmp/.X11-unix \
    -mindepth 1 -maxdepth 1 -type s ! -name X0 -print -quit
}

remote_namespace_first_wayland_socket() {
  local pid="$1" runtime="$2" find_helper
  find_helper="$(readlink -f -- "$(command -v find)" 2>/dev/null)" \
    || fail 'Wayland socket enumeration helper is unavailable'
  [[ "$find_helper" == /nix/store/*/bin/find && -x "$find_helper" ]] \
    || fail 'Wayland socket enumeration helper is not immutable'
  remote_namespace_test "$pid" no "$find_helper" "$runtime" \
    -maxdepth 1 -type s -name 'wayland-[0-9]*' -print -quit
}

remote_assert_pid_namespace_hides_compositor() {
  local subject="$1" pid="$2" sway_pid="$3"
  if remote_namespace_test "$pid" yes test -e "/proc/$sway_pid"; then
    fail "$subject PID namespace exposes the compositor process"
  fi
  if remote_namespace_test "$pid" yes \
    test -e "/proc/$sway_pid/root$COMPOSITOR_CONTROL_SOCKET"; then
    fail "$subject can reach compositor control through procfs"
  fi
}

remote_assert_game_display_namespace() {
  local pid="$1" runtime="$2" extra_x11_socket wayland_socket
  remote_namespace_test "$pid" no test -S /tmp/.X11-unix/X0 \
    || fail 'Xwayland display is not visible inside the live game unit'
  extra_x11_socket="$(remote_namespace_first_extra_x11_socket "$pid")" \
    || fail 'Xwayland socket enumeration failed inside the live game unit'
  [[ -z "$extra_x11_socket" ]] || fail 'live game unit can access an unapproved X11 socket'
  if remote_namespace_test "$pid" no test -e "$runtime/$COMPOSITOR_WAYLAND_ALIAS"; then
    fail 'stable Wayland socket is visible inside the live game unit'
  fi
  wayland_socket="$(remote_namespace_first_wayland_socket "$pid" "$runtime")" \
    || fail 'Wayland socket enumeration failed inside the live game unit'
  [[ -z "$wayland_socket" ]] || fail 'numeric Wayland socket is visible inside the live game unit'
  if remote_namespace_test "$pid" no test -e "$COMPOSITOR_CONTROL_SOCKET"; then
    fail 'compositor control is visible inside the live game unit'
  fi
}

remote_parse_compositor_mode() {
  local mode="$1" width height refresh_hz
  if [[ "$mode" =~ ^([1-9][0-9]*)x([1-9][0-9]*)@([1-9][0-9]*)Hz$ ]]; then
    width="${BASH_REMATCH[1]}"
    height="${BASH_REMATCH[2]}"
    refresh_hz="${BASH_REMATCH[3]}"
  else
    printf 'invalid compositor mode: %s\n' "$mode" >&2
    return 1
  fi
  printf '%s %s %s %s\n' "$width" "$height" "$((10#$refresh_hz * 1000))" "$mode"
}

remote_high_refresh_performance_gate() {
  local expected_refresh="$1" root="${2:-}"
  local profile="$root/sys/firmware/acpi/platform_profile"
  local min_perf="$root/sys/devices/system/cpu/intel_pstate/min_perf_pct"
  local max_perf="$root/sys/devices/system/cpu/intel_pstate/max_perf_pct"
  (( expected_refresh >= 120000 )) || return 0
  systemctl is-active --quiet korri-streaming-performance-profile.service \
    || fail 'high-refresh streaming performance profile is inactive'
  [[ -f "$profile" && "$(cat "$profile")" == performance ]] \
    || fail 'high-refresh ACPI platform profile is not performance'
  [[ -f "$min_perf" && "$(cat "$min_perf")" == 40 ]] \
    || fail 'high-refresh minimum Intel pstate bound is not 40'
  [[ -f "$max_perf" && "$(cat "$max_perf")" == 60 ]] \
    || fail 'high-refresh maximum Intel pstate bound is not 60'
}

remote_compositor_gate() {
  local runtime_user="$1" uid gid runtime stable target control_metadata directory_metadata
  local compositor_environment sunshine_environment sunshine_inaccessible korrid_environment
  local execstart running declared declared_link declared_real running_target package_root swaymsg outputs render_device render_real render_driver token dev_sys dev_stat jq_helper
  local sunshine_pid sunshine_private_pids sway_pid compositor_config compositor_config_metadata configured_mode
  local expected_width expected_height expected_refresh expected_mode renderer candidate_renderer renderer_count=0 performance_state=not-required
  local -a sway_execs=() wrapped_targets=() render_devices=() compositor_configs=() configured_modes=()
  uid="$(id -u "$runtime_user")" || fail 'runtime user is unavailable for compositor validation'
  gid="$(id -g "$runtime_user")" || fail 'runtime group is unavailable for compositor validation'
  runtime="/run/user/$uid"
  stable="$runtime/$COMPOSITOR_WAYLAND_ALIAS"
  [[ -d "$COMPOSITOR_CONTROL_DIRECTORY" && ! -L "$COMPOSITOR_CONTROL_DIRECTORY" ]] \
    || fail 'compositor control directory is absent or linked'
  directory_metadata="$(stat -Lc '%u:%g:%a' -- "$COMPOSITOR_CONTROL_DIRECTORY" 2>/dev/null)" \
    || fail 'compositor control directory metadata is unavailable'
  [[ "$directory_metadata" == "$uid:$gid:700" ]] \
    || fail 'compositor control directory ownership or mode is invalid'
  [[ -S "$COMPOSITOR_CONTROL_SOCKET" && ! -L "$COMPOSITOR_CONTROL_SOCKET" ]] \
    || fail 'compositor control socket is absent or linked'
  control_metadata="$(stat -Lc '%u:%g:%a' -- "$COMPOSITOR_CONTROL_SOCKET" 2>/dev/null)" \
    || fail 'compositor control socket metadata is unavailable'
  [[ "$control_metadata" == "$uid:$gid:700" ]] \
    || fail 'compositor control socket ownership or mode is invalid'
  [[ -L "$stable" ]] || fail 'stable Wayland display alias is absent'
  target="$(readlink -- "$stable")" || fail 'stable Wayland display alias target is unavailable'
  [[ "$target" =~ ^wayland-[0-9]+$ && -S "$runtime/$target" ]] \
    || fail 'stable Wayland display alias does not name a live numeric socket'
  [[ -S /tmp/.X11-unix/X0 ]] || fail 'Xwayland display :0 is absent'

  compositor_environment="$(systemctl show korri-compositor.service -p Environment --value 2>/dev/null)" \
    || fail 'compositor environment is unavailable'
  for expected in \
    'WLR_BACKENDS=headless' \
    "SWAYSOCK=$COMPOSITOR_CONTROL_SOCKET"; do
    [[ " $compositor_environment " == *" $expected "* ]] \
      || fail "compositor environment lacks $expected"
  done
  for candidate_renderer in gles2 pixman; do
    if [[ " $compositor_environment " == *" WLR_RENDERER=$candidate_renderer "* ]]; then
      renderer="$candidate_renderer"
      renderer_count=$((renderer_count + 1))
    fi
  done
  [[ "$renderer_count" -eq 1 ]] || fail 'compositor does not declare one approved renderer'
  [[ " $compositor_environment " != *' WAYLAND_DISPLAY='* ]] \
    || fail 'compositor service must not receive WAYLAND_DISPLAY'
  for token in $compositor_environment; do
    case "$token" in
      WLR_RENDER_DRM_DEVICE=*) render_devices+=("${token#WLR_RENDER_DRM_DEVICE=}") ;;
    esac
  done
  [[ "${#render_devices[@]}" -eq 1 ]] || fail 'compositor does not declare one DRM render device'
  render_device="${render_devices[0]}"
  [[ "$render_device" == /dev/dri/by-path/*-render && -L "$render_device" ]] \
    || fail 'compositor DRM render device is not a stable by-path link'
  render_real="$(realpath -e -- "$render_device" 2>/dev/null)" \
    || fail 'compositor DRM render device target is unavailable'
  [[ "$render_real" == /dev/dri/renderD* && -c "$render_device" ]] \
    || fail 'compositor DRM render device target is not a character device'
  dev_sys="$(<"/sys/class/drm/${render_real##*/}/dev")"
  dev_stat="$(stat -Lc '%t:%T' "$render_device" 2>/dev/null)" \
    || fail 'compositor DRM render device number is unavailable'
  [[ "$dev_stat" =~ ^[0-9a-fA-F]+:[0-9a-fA-F]+$ \
    && "$dev_sys" == "$((16#${dev_stat%:*})):$((16#${dev_stat#*:}))" ]] \
    || fail 'compositor DRM render device number does not match sysfs'
  render_driver="$(basename "$(readlink -f "/sys/class/drm/${render_real##*/}/device/driver" 2>/dev/null)")"
  [[ "$render_driver" == nvidia ]] || fail 'compositor DRM render device is not NVIDIA'
  sudo -n -u "$runtime_user" test -r "$render_device" \
    || fail 'runtime identity cannot read the compositor DRM render device'

  sunshine_environment="$(systemctl show sunshine.service -p Environment --value 2>/dev/null)" \
    || fail 'Sunshine environment is unavailable'
  [[ " $sunshine_environment " == *" WAYLAND_DISPLAY=$COMPOSITOR_WAYLAND_ALIAS "* \
    && " $sunshine_environment " == *" XDG_RUNTIME_DIR=$runtime "* \
    && " $sunshine_environment " == *' XDG_SESSION_TYPE=wayland '* ]] \
    || fail 'Sunshine lacks the exact Wayland capture environment'
  sunshine_inaccessible="$(systemctl show sunshine.service -p InaccessiblePaths --value 2>/dev/null)" \
    || fail 'Sunshine inaccessible paths are unavailable'
  [[ " $sunshine_inaccessible " == *" $COMPOSITOR_CONTROL_DIRECTORY "* ]] \
    || fail 'Sunshine can access compositor control'
  sunshine_private_pids="$(systemctl show sunshine.service -p PrivatePIDs --value 2>/dev/null)" \
    || fail 'Sunshine PID namespace state is unavailable'
  [[ "$sunshine_private_pids" == yes ]] || fail 'Sunshine does not have a private PID namespace'
  korrid_environment="$(systemctl show korrid.service -p Environment --value 2>/dev/null)" \
    || fail 'korrid environment is unavailable'
  [[ " $korrid_environment " == *" KORRID_COMPOSITOR_CONTROL_DIRECTORY=$COMPOSITOR_CONTROL_DIRECTORY "* ]] \
    || fail 'korrid lacks the compositor game-isolation path'

  execstart="$(systemctl show korri-compositor.service -p ExecStart --value 2>/dev/null)" \
    || fail 'compositor executable declaration is unavailable'
  mapfile -t sway_execs < <(grep -oE '/nix/store/[^ ;{}"]+/bin/sway' <<<"$execstart" | sort -u)
  [[ "${#sway_execs[@]}" -eq 1 ]] || fail 'compositor unit does not declare one exact Sway executable'
  mapfile -t compositor_configs < <(grep -oE -- '--config /nix/store/[^ ;{}"]+-korri-sway\.conf' <<<"$execstart" | awk '{ print $2 }' | sort -u)
  [[ "${#compositor_configs[@]}" -eq 1 ]] || fail 'compositor unit does not declare one exact Sway configuration'
  compositor_config="${compositor_configs[0]}"
  compositor_config_metadata="$(stat -Lc '%u:%g:%a' -- "$compositor_config" 2>/dev/null)" \
    || fail 'compositor configuration metadata is unavailable'
  [[ "$compositor_config" == /nix/store/*-korri-sway.conf \
    && -f "$compositor_config" && ! -L "$compositor_config" \
    && "$compositor_config_metadata" == 0:0:444 ]] \
    || fail 'compositor configuration is not immutable store content'
  mapfile -t configured_modes < <(grep -E "^output $COMPOSITOR_OUTPUT mode [1-9][0-9]*x[1-9][0-9]*@[1-9][0-9]*Hz$" "$compositor_config")
  [[ "${#configured_modes[@]}" -eq 1 ]] || fail 'compositor configuration does not declare one exact output mode'
  configured_mode="${configured_modes[0]##* mode }"
  read -r expected_width expected_height expected_refresh expected_mode \
    < <(remote_parse_compositor_mode "$configured_mode") \
    || fail 'compositor configuration mode is invalid'
  if (( expected_refresh >= 120000 )); then
    remote_high_refresh_performance_gate "$expected_refresh"
    performance_state=active
  fi
  declared="${sway_execs[0]}"
  package_root="${declared%/bin/sway}"
  swaymsg="$package_root/bin/swaymsg"
  [[ "$declared" == /nix/store/*/bin/sway && -x "$declared" && -x "$swaymsg" ]] \
    || fail 'compositor command package is invalid'
  declared_link="$package_root/bin/.sway-wrapped"
  [[ -f "$declared" && -x "$declared" && ! -L "$declared" \
    && -L "$declared_link" ]] \
    || fail 'declared compositor wrapper or same-package link is invalid'
  grep -F "\"$declared_link\"" "$declared" >/dev/null \
    || fail 'declared compositor wrapper does not execute its same-package link'
  declared_real="$(readlink -f -- "$declared_link" 2>/dev/null)" \
    || fail 'declared compositor wrapper target is unavailable'
  [[ "$declared_real" == /nix/store/*-sway/bin/sway \
    && -f "$declared_real" && -x "$declared_real" && ! -L "$declared_real" ]] \
    || fail 'declared compositor wrapper target is invalid'
  mapfile -t wrapped_targets < <(grep -oE '/nix/store/[^ ;{}"]+/bin/sway' "$declared_real" | sort -u)
  [[ "${#wrapped_targets[@]}" -eq 1 ]] \
    || fail 'same-package compositor wrapper does not declare one final Sway executable'
  running_target="${wrapped_targets[0]}"
  [[ "$running_target" == /nix/store/*-sway-unwrapped-*/bin/sway \
    && -f "$running_target" && -x "$running_target" && ! -L "$running_target" ]] \
    || fail 'final compositor executable target is invalid'
  sway_pid="$(systemctl show korri-compositor.service -p MainPID --value 2>/dev/null)" \
    || fail 'running compositor PID is unavailable'
  [[ "$sway_pid" =~ ^[1-9][0-9]*$ ]] || fail 'running compositor PID is invalid'
  running="$(readlink -f -- "/proc/$sway_pid/exe" 2>/dev/null)" \
    || fail 'running compositor executable is unavailable'
  [[ "$running" == "$running_target" ]] || fail 'running compositor executable differs from the candidate unit'
  sunshine_pid="$(systemctl show sunshine.service -p MainPID --value 2>/dev/null)" \
    || fail 'Sunshine PID is unavailable for compositor isolation validation'
  [[ "$sunshine_pid" =~ ^[1-9][0-9]*$ ]] || fail 'Sunshine PID is invalid for compositor isolation validation'
  remote_assert_pid_namespace_hides_compositor Sunshine "$sunshine_pid" "$sway_pid"
  outputs="$(sudo -n -u "$runtime_user" "$swaymsg" -s "$COMPOSITOR_CONTROL_SOCKET" -t get_outputs -r)" \
    || fail 'compositor output query failed'
  jq_helper="$(readlink -f -- "$(command -v jq)" 2>/dev/null)" \
    || fail 'compositor output validation helper is unavailable'
  [[ "$jq_helper" == /nix/store/*/bin/jq && -x "$jq_helper" ]] \
    || fail 'compositor output validation helper is not immutable'
  # shellcheck disable=SC2016 # The jq program reads the named --arg values.
  "$jq_helper" -e \
    --arg output "$COMPOSITOR_OUTPUT" \
    --argjson width "$expected_width" \
    --argjson height "$expected_height" \
    --argjson refresh "$expected_refresh" \
    '.[] | select(.name == $output and .active == true and .current_mode.width == $width and .current_mode.height == $height and .current_mode.refresh == $refresh)' \
    <<<"$outputs" >/dev/null || fail "compositor output is not active at $expected_mode"
  printf 'compositor-gate=pass renderer=%s output=%s mode=%s performance=%s wayland=stable xwayland=:0 sunshine-control=denied\n' \
    "$renderer" "$COMPOSITOR_OUTPUT" "$expected_mode" "$performance_state"
}

remote_compositor_game_gate() {
  local runtime_user="$1" uid game_units unit pid process_uid inaccessible runtime private_pids sway_pid game_environment bind_paths
  uid="$(id -u "$runtime_user")" || fail 'runtime user is unavailable for game compositor validation'
  runtime="/run/user/$uid"
  game_units="$(systemctl list-units --type=service --state=activating,active,reloading,deactivating \
    --no-legend --plain 'korri-game-*.service')" || fail 'Korri game units are unavailable for compositor validation'
  [[ "$(awk 'NF { count += 1 } END { print count + 0 }' <<<"$game_units")" -eq 1 ]] \
    || fail 'direct-action compositor validation requires exactly one live game unit'
  unit="$(awk 'NF { print $1 }' <<<"$game_units")"
  [[ "$unit" =~ ^korri-game-[0-9a-f]{32}\.service$ ]] || fail 'live game unit name is invalid'
  pid="$(systemctl show "$unit" -p MainPID --value 2>/dev/null)" \
    || fail 'live game unit PID is unavailable'
  [[ "$pid" =~ ^[1-9][0-9]*$ && -r "/proc/$pid/status" ]] || fail 'live game unit PID is invalid'
  process_uid="$(awk '/^Uid:/ {print $2}' "/proc/$pid/status")" \
    || fail 'live game unit UID is unavailable'
  [[ "$process_uid" == "$uid" ]] || fail 'live game unit does not use the runtime identity'
  inaccessible="$(systemctl show "$unit" -p InaccessiblePaths --value 2>/dev/null)" \
    || fail 'live game unit inaccessible paths are unavailable'
  [[ " $inaccessible " == *" $COMPOSITOR_CONTROL_DIRECTORY "* ]] \
    || fail 'live game unit lacks the compositor control denial path'
  [[ " $inaccessible " == *" $runtime "* ]] \
    || fail 'live game unit lacks the runtime-directory denial path'
  game_environment="$(systemctl show "$unit" -p Environment --value 2>/dev/null)" \
    || fail 'live game unit environment is unavailable'
  [[ " $game_environment " == *' DISPLAY=:0 '* && " $game_environment " == *' XDG_SESSION_TYPE=x11 '* ]] \
    || fail 'live game unit lacks the exact Xwayland environment'
  [[ " $game_environment " != *' WAYLAND_DISPLAY='* && " $game_environment " != *' SWAYSOCK='* \
    && " $game_environment " != *' XDG_RUNTIME_DIR='* ]] \
    || fail 'live game unit received native compositor access environment'
  bind_paths="$(systemctl show "$unit" -p BindReadOnlyPaths --value 2>/dev/null)" \
    || fail 'live game unit Xwayland bind state is unavailable'
  [[ " $bind_paths " == *" /tmp/.X11-unix/X0:/tmp/.X11-unix/X0:rbind "* ]] \
    || fail 'live game unit lacks the exact read-only Xwayland X0 socket bind'
  [[ " $bind_paths " != *" /tmp/.X11-unix:/tmp/.X11-unix:"* ]] \
    || fail 'live game unit binds the complete X11 socket directory'
  private_pids="$(systemctl show "$unit" -p PrivatePIDs --value 2>/dev/null)" \
    || fail 'live game unit PID namespace state is unavailable'
  [[ "$private_pids" == yes ]] || fail 'live game unit does not have a private PID namespace'
  remote_assert_game_display_namespace "$pid" "$runtime"
  sway_pid="$(systemctl show korri-compositor.service -p MainPID --value 2>/dev/null)" \
    || fail 'compositor PID is unavailable for game namespace validation'
  [[ "$sway_pid" =~ ^[1-9][0-9]*$ ]] || fail 'compositor PID is invalid for game namespace validation'
  remote_assert_pid_namespace_hides_compositor game "$pid" "$sway_pid"
  printf 'game-compositor-gate=pass xwayland=visible wayland=hidden control=hidden procfs=isolated unit=one\n'
}

remote_start_candidate_services() {
  local runtime_user="$1" unit
  for unit in korri-compositor.service korrid.service sunshine.service; do
    systemctl start "$unit"
    remote_wait_unit "$unit"
  done
  remote_candidate_credentials "$runtime_user"
}

remote_stop_candidate_services() {
  local unit
  for unit in sunshine.service korrid.service korri-compositor.service korri-inputd.service inputplumber.service; do
    systemctl stop "$unit" >/dev/null || return $?
    ! systemctl is-active --quiet "$unit" || fail "candidate system service remained active: $unit"
  done
}

remote_raw_joystick_events() {
  local event node name props
  shopt -s nullglob
  for event in /sys/class/input/event*; do
    node="/dev/input/${event##*/}"
    [[ -r "$event/device/name" ]] || continue
    name="$(<"$event/device/name")"
    [[ "$name" != "$NORMALIZED_NAME" ]] || continue
    props="$(udevadm info --query=property --name="$node" 2>/dev/null || true)"
    grep -Fx 'ID_INPUT_JOYSTICK=1' <<<"$props" >/dev/null || continue
    printf '%s\n' "$event"
  done
}

remote_restore_raw_joystick_udev() {
  local event
  [[ "$(systemctl show inputplumber.service -p ActiveState --value 2>/dev/null || true)" != active ]] \
    || fail 'InputPlumber is still active during raw joystick restore'
  [[ -z "$(find /run/udev/rules.d -maxdepth 1 -type f -name '*inputplumber-hide*' -print -quit 2>/dev/null)" ]] \
    || fail 'InputPlumber hide rules remain during raw joystick restore'
  sudo -n udevadm control --reload-rules \
    || fail 'raw joystick udev rule reload failed'
  while IFS= read -r event; do
    [[ -n "$event" ]] || continue
    sudo -n udevadm trigger --action=add --settle "$event" \
      || fail "raw joystick udev trigger failed: $event"
  done < <(remote_raw_joystick_events)
  sudo -n udevadm settle --timeout=30 \
    || fail 'raw joystick udev settle failed'
}

remote_stable_raw_topology_record() {
  local name="$1" phys="$2" uniq="$3" id="$4" props="$5" sysfs="$6" readable="$7"
  local key value serial='' path='' stable_sysfs
  while IFS='=' read -r key value; do
    case "$key" in
      ID_SERIAL)
        [[ -z "$serial" ]] || fail 'raw controller has duplicate ID_SERIAL properties'
        serial="$value"
        ;;
      ID_PATH)
        [[ -z "$path" ]] || fail 'raw controller has duplicate ID_PATH properties'
        path="$value"
        ;;
    esac
  done <<<"$props"
  [[ -n "$serial" ]] || fail 'raw controller ID_SERIAL is unavailable'
  [[ -n "$path" ]] || fail 'raw controller ID_PATH is unavailable'
  [[ "$sysfs" =~ ^(/sys/devices/.+)/input/input[0-9]+/event[0-9]+$ ]] \
    || fail 'raw controller sysfs path is not canonical'
  stable_sysfs="${BASH_REMATCH[1]}"
  printf '%q|%q|%q|%s|%q|%q|%q|%s\n' \
    "$name" "$phys" "$uniq" "$id" "$serial" "$path" "$stable_sysfs" "$readable"
}

remote_topology_digest() {
  local kind="$1" event node name phys uniq id props dev sysfs readable
  {
    shopt -s nullglob
    for event in /sys/class/input/event*; do
      node="/dev/input/${event##*/}"
      [[ -r "$event/device/name" ]] || continue
      name="$(<"$event/device/name")"
      props="$(udevadm info --query=property --name="$node" 2>/dev/null || true)"
      grep -Fx 'ID_INPUT_JOYSTICK=1' <<<"$props" >/dev/null || continue
      if [[ "$kind" == target ]]; then
        [[ "$name" == "$NORMALIZED_NAME" ]] || continue
      else
        [[ "$name" != "$NORMALIZED_NAME" ]] || continue
      fi
      phys="$(cat "$event/device/phys" 2>/dev/null || true)"
      uniq="$(cat "$event/device/uniq" 2>/dev/null || true)"
      id="$(cat "$event/device/id/bustype" 2>/dev/null || true):$(cat "$event/device/id/vendor" 2>/dev/null || true):$(cat "$event/device/id/product" 2>/dev/null || true):$(cat "$event/device/id/version" 2>/dev/null || true)"
      sysfs="$(realpath -e -- "$event" 2>/dev/null || true)"
      readable="$(test -r "$node" && printf yes || printf no)"
      if [[ "$kind" == raw ]]; then
        remote_stable_raw_topology_record "$name" "$phys" "$uniq" "$id" "$props" "$sysfs" "$readable"
      else
        dev="$(cat "$event/dev" 2>/dev/null || true)"
        printf '%q|%q|%q|%s|%s|%s|%s\n' "$name" "$phys" "$uniq" "$id" "$dev" "$sysfs" "$readable"
      fi
    done | sort
  } | sha256sum | cut -d' ' -f1
}

remote_canonical_acl() {
  local node="$1" acl line named=false
  local -a lines=()
  acl="$(getfacl -cpn "$node" 2>/dev/null)" || return 1
  mapfile -t lines <<<"$acl"
  for line in "${lines[@]}"; do
    [[ "$line" =~ ^(user|group):[^:]+: ]] && named=true
  done
  {
    printf '\n'
    for line in "${lines[@]}"; do
      [[ -n "$line" ]] || continue
      [[ "$named" == true || "$line" != mask::* ]] || continue
      printf '%s\n' "$line"
    done
  } | LC_ALL=C sort | tr '\n' ','
}

remote_acl_digest() {
  local runtime_user="$1" event node name props phys uniq id sysfs readable identity game_read
  {
    while IFS= read -r event; do
      [[ -n "$event" ]] || continue
      node="/dev/input/${event##*/}"
      name="$(<"$event/device/name")"
      props="$(udevadm info --query=property --name="$node" 2>/dev/null || true)"
      phys="$(cat "$event/device/phys" 2>/dev/null || true)"
      uniq="$(cat "$event/device/uniq" 2>/dev/null || true)"
      id="$(cat "$event/device/id/bustype" 2>/dev/null || true):$(cat "$event/device/id/vendor" 2>/dev/null || true):$(cat "$event/device/id/product" 2>/dev/null || true):$(cat "$event/device/id/version" 2>/dev/null || true)"
      sysfs="$(realpath -e -- "$event" 2>/dev/null || true)"
      readable="$(test -r "$node" && printf yes || printf no)"
      identity="$(remote_stable_raw_topology_record "$name" "$phys" "$uniq" "$id" "$props" "$sysfs" "$readable")"
      game_read="$(sudo -n -u "$runtime_user" test -r "$node" && printf yes || printf no)"
      printf '%s|%s|%s|' "$identity" "$(stat -Lc '%a:%u:%g' "$node" 2>/dev/null || true)" "$game_read"
      remote_canonical_acl "$node"
      printf '\n'
    done < <(remote_raw_joystick_events)
  } | sort | sha256sum | cut -d' ' -f1
}

remote_source_artifacts_digest() {
  {
    if [[ -d /dev/inputplumber/sources ]]; then
      find /dev/inputplumber/sources -mindepth 1 -maxdepth 2 -printf '%P|%y|%m|%u|%g|%D:%i\n' 2>/dev/null | sort
    else
      printf 'sources-directory=absent\n'
    fi
    printf 'temporary-dirty=%s\n' "$(remote_temporary_artifacts_dirty && printf yes || printf no)"
  } | sha256sum | cut -d' ' -f1
}

# Sanitized, exact predicates used for rollback and reconcile comparison.
remote_predicates() {
  local runtime_user="$1" unit stem active enabled
  printf 'generation.current=%s\n' "$(remote_generation)"
  printf 'generation.default=%s\n' "$(readlink -f /nix/var/nix/profiles/system 2>/dev/null || true)"
  for unit in "${OLD_USER_UNITS[@]}"; do
    stem="${unit%.service}"
    active="$(remote_user_unit_active "$runtime_user" "$unit")" \
      || fail "old user unit active state query failed: $unit"
    enabled="$(remote_user_unit_enabled "$runtime_user" "$unit")" \
      || fail "old user unit enablement query failed: $unit"
    printf 'old-user.%s.active=%s\n' "$stem" "$active"
    printf 'old-user.%s.enabled=%s\n' "$stem" "$enabled"
  done
  for unit in "${PREDICATE_SYSTEM_UNITS[@]}"; do
    stem="${unit%.service}"
    printf 'system.%s.active=%s\n' "$stem" "$(remote_unit_value system '' "$unit" ActiveState)"
    printf 'system.%s.enabled=%s\n' "$stem" "$(remote_unit_value system '' "$unit" UnitFileState)"
  done
  printf 'topology.target=%s\n' "$(remote_topology_digest target)"
  printf 'topology.raw=%s\n' "$(remote_topology_digest raw)"
  printf 'input.acl-readability=%s\n' "$(remote_acl_digest "$runtime_user")"
  printf 'input.sources-artifacts=%s\n' "$(remote_source_artifacts_digest)"
  printf 'inputplumber.active=%s\n' "$(remote_unit_value system '' inputplumber.service ActiveState)"
  printf 'inputplumber.enabled=%s\n' "$(remote_unit_value system '' inputplumber.service UnitFileState)"
  printf 'sunshine.pairing-state-modes=%s\n' "$(remote_pairing_state_modes "$runtime_user" 2>/dev/null || printf invalid)"
  printf 'sunshine.pairing-state-present=%s\n' "$(remote_pairing_state_present "$runtime_user" && printf true || printf false)"
  printf 'sunshine.private-state-digest=%s\n' "$(remote_sunshine_private_state_digest "$runtime_user" 2>/dev/null || printf invalid)"
  printf 'catalog.health=%s\n' "$(remote_catalog_health)"
}

remote_unit_snapshot() {
  local scope="$1" runtime_user="$2" unit="$3" load active sub enabled status
  load="$(remote_unit_value "$scope" "$runtime_user" "$unit" LoadState)" \
    || fail "$scope unit LoadState query failed: $unit"
  active="$(remote_unit_value "$scope" "$runtime_user" "$unit" ActiveState)" \
    || fail "$scope unit ActiveState query failed: $unit"
  sub="$(remote_unit_value "$scope" "$runtime_user" "$unit" SubState)" \
    || fail "$scope unit SubState query failed: $unit"
  enabled="$(remote_unit_value "$scope" "$runtime_user" "$unit" UnitFileState)" \
    || fail "$scope unit UnitFileState query failed: $unit"
  status="$(remote_unit_value "$scope" "$runtime_user" "$unit" StatusText)" \
    || fail "$scope unit StatusText query failed: $unit"
  printf '%s/%s LoadState=%s ActiveState=%s SubState=%s UnitFileState=%s StatusText=%s\n' \
    "$scope" "$unit" "$load" "$active" "$sub" "$enabled" "$status"
}

remote_inspect() {
  local runtime_user="$1" machine_id hostname
  machine_id="$(tr -d '\n' </etc/machine-id)"
  hostname="$(hostname)"
  printf 'identity machine-id=%s hostname=%s\n' "$machine_id" "$hostname"
  printf 'generation current=%s default=%s\n' "$(remote_generation)" "$(readlink -f /nix/var/nix/profiles/system 2>/dev/null || true)"
  printf '%s\n' 'units:'
  remote_unit_snapshot system '' inputplumber.service
  remote_unit_snapshot system '' korri-inputd.service
  remote_unit_snapshot system '' korrid.service
  remote_unit_snapshot system '' sunshine.service
  remote_unit_snapshot system '' x11-headless.service
  remote_unit_snapshot system '' korri-compositor.service
  remote_unit_snapshot user "$runtime_user" korrid.service
  remote_unit_snapshot user "$runtime_user" sunshine.service
  remote_unit_snapshot user "$runtime_user" x11-headless.service
  printf 'temporary-artifacts-dirty=%s catalog=%s\n' \
    "$(remote_temporary_artifacts_dirty && printf yes || printf no)" "$(remote_catalog_health)"
  printf '%s\n' 'physical-controller-candidates:'
  remote_controller_candidates
}

remote_preflight() {
  local candidate="$1" rollback="$2" expected_identity="${3:-}" profile="${4:-}" candidate_real rollback_real evidence
  valid_generation_path "$candidate" && valid_generation_path "$rollback" || return 1
  candidate_real="$(realpath -e -- "$candidate" 2>/dev/null || true)"
  rollback_real="$(realpath -e -- "$rollback" 2>/dev/null || true)"
  printf 'candidate=%s\n' "$candidate_real"
  printf 'candidate-switch=%s\n' "$([[ -x "$candidate_real/bin/switch-to-configuration" ]] && printf yes || printf no)"
  printf 'rollback=%s\n' "$rollback_real"
  printf 'rollback-switch=%s\n' "$([[ -x "$rollback_real/bin/switch-to-configuration" ]] && printf yes || printf no)"
  printf 'temporary-artifacts-dirty=%s\n' "$(remote_temporary_artifacts_dirty && printf yes || printf no)"
  if [[ -n "$expected_identity" && -n "$profile" ]]; then
    if evidence="$(remote_physical_controller_evidence "$expected_identity" "$profile" false 2>/dev/null)"; then
      printf 'expected-controller=yes\n'
      printf 'controller-evidence=%s\n' "$evidence"
    else
      printf 'expected-controller=no\n'
    fi
  fi
}

remote_unhealthy_system_units() {
  systemctl list-units --type=service --all --no-legend --plain 2>/dev/null \
    | awk '$3 == "failed" || ($3 == "activating" && $4 == "auto-restart") { print $1 }' \
    | LC_ALL=C sort -u
}

remote_service_restart_counts() {
  systemctl show --type=service --all -p Id -p NRestarts --no-pager 2>/dev/null \
    | awk -F= '
        $1 == "Id" { unit = $2 }
        $1 == "NRestarts" && unit != "" && $2 ~ /^[0-9]+$/ {
          print unit "=" $2
          unit = ""
        }
      '
}

remote_observed_unhealthy_system_units() {
  local first before second after unit count prior
  local -A prior_counts=()
  first="$(remote_unhealthy_system_units)" || return 1
  before="$(remote_service_restart_counts)" || return 1
  while IFS='=' read -r unit count; do
    [[ -z "$unit" ]] && continue
    [[ "$count" =~ ^[0-9]+$ ]] || return 1
    prior_counts["$unit"]="$count"
  done <<<"$before"
  sleep "$UNHEALTHY_OBSERVE_SECONDS"
  second="$(remote_unhealthy_system_units)" || return 1
  after="$(remote_service_restart_counts)" || return 1
  {
    [[ -z "$first" ]] || printf '%s\n' "$first"
    [[ -z "$second" ]] || printf '%s\n' "$second"
    while IFS='=' read -r unit count; do
      [[ -z "$unit" ]] && continue
      [[ "$count" =~ ^[0-9]+$ ]] || return 1
      prior="${prior_counts[$unit]:-0}"
      ((count <= prior)) || printf '%s\n' "$unit"
    done <<<"$after"
  } | awk 'NF' | LC_ALL=C sort -u
}

remote_activate_generation() {
  local generation="$1" action="$2" before after status unit summary
  before="$(remote_observed_unhealthy_system_units)" \
    || fail 'pre-activation system unit health is unavailable'
  if sudo -n "$generation/bin/switch-to-configuration" "$action"; then
    status=0
  else
    status=$?
  fi
  [[ "$status" == 0 || "$status" == 4 ]] \
    || fail "switch-to-configuration failed with status $status"
  [[ "$(remote_generation)" == "$generation" ]] \
    || fail 'activation did not make the requested generation current'
  after="$(remote_observed_unhealthy_system_units)" \
    || fail 'post-activation system unit health is unavailable'
  while IFS= read -r unit; do
    [[ -z "$unit" ]] || grep -Fqx -- "$unit" <<<"$before" \
      || fail "activation introduced unhealthy system unit: $unit"
  done <<<"$after"
  if [[ "$status" == 4 ]]; then
    summary="$(tr '\n' ',' <<<"$after" | sed 's/,$//')"
    printf 'activation=accepted status=4 remaining-unhealthy=%s\n' "${summary:-none}"
  fi
}

remote_activate_test() {
  local candidate="$1" runtime_user="$2"
  remote_quiesce_for_private_state_cut "$runtime_user" candidate || return $?
  remote_activate_generation "$candidate" test
  remote_switch_generation_bundle "$candidate"
  remote_disable_old_user_units "$runtime_user"
  remote_start_candidate_services "$runtime_user"
}

remote_restore() {
  local rollback="$1" persistent="$2" runtime_user="$3" rollback_has_bundle_selector=false
  shift 3
  [[ "$#" -eq 8 ]] || fail 'rollback requires all old user-unit states and pairing modes'
  if remote_generation_has_bundle_selector "$rollback"; then
    rollback_has_bundle_selector=true
  fi
  remote_quiesce_for_private_state_cut "$runtime_user" rollback "$rollback_has_bundle_selector" || return $?
  if [[ "$persistent" == true ]]; then
    sudo -n nix-env -p /nix/var/nix/profiles/system --set "$rollback"
    remote_activate_generation "$rollback" switch
  else
    remote_activate_generation "$rollback" test
  fi
  if [[ "$rollback_has_bundle_selector" == true ]]; then
    remote_switch_generation_bundle "$rollback"
  fi
  remote_set_pairing_state_modes "$runtime_user" "$7" "$8" >/dev/null
  remote_restore_old_user_units "$runtime_user" "$1" "$2" "$3" "$4" "$5" "$6"
  [[ "$(remote_generation)" == "$rollback" ]]
}

remote_acceptance_fingerprint() {
  local runtime_user="$1" expected_identity="$2" profile="$3" require_physical="$4"
  local normalized physical sunshine private_state
  normalized="$(remote_normalized_fingerprint)" || return 1
  sunshine="$(remote_sunshine_package_provenance)" || return 1
  private_state="$(remote_sunshine_private_state_digest "$runtime_user" 2>/dev/null)" || return 1
  [[ "$private_state" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf 'normalized=%s sunshine=%q private-state=%s' "$normalized" "$sunshine" "$private_state"
  if [[ "$require_physical" == true ]]; then
    physical="$(remote_physical_controller_evidence "$expected_identity" "$profile")" || return 1
    printf ' physical=%s' "$physical"
  fi
  printf '\n'
}

remote_automated_gates() {
  local runtime_user="$1" expected_identity="$2" profile="$3" require_physical="$4"
  local fingerprint current_fingerprint controller_evidence acceptance delegate delegate_controllers node event event_node unit active enabled readable_raw=0
  remote_wait_unit inputplumber.service
  remote_wait_unit korri-inputd.service Ready
  remote_wait_unit korrid.service
  remote_wait_unit korri-compositor.service
  remote_wait_unit sunshine.service
  for unit in "${OLD_USER_UNITS[@]}"; do
    active="$(remote_user_unit_active "$runtime_user" "$unit")" \
      || fail "old user unit active state query failed during candidate verification: $unit"
    enabled="$(remote_user_unit_enabled "$runtime_user" "$unit")" \
      || fail "old user unit enablement query failed during candidate verification: $unit"
    [[ "$active" == false ]] || fail "old user service is active beside its system replacement: $unit"
    [[ "$enabled" == false ]] || fail "old user service is enabled beside its system replacement: $unit"
  done
  remote_candidate_credentials "$runtime_user"
  remote_compositor_gate "$runtime_user"
  local sunshine_provenance
  sunshine_provenance="$(remote_sunshine_package_provenance)" \
    || fail 'running sunshine-korri provenance validation failed'
  remote_pairing_state_present "$runtime_user" || fail 'Sunshine pairing-state file is absent'
  local sunshine_private_state
  sunshine_private_state="$(remote_sunshine_private_state_digest "$runtime_user")" \
    || fail 'Sunshine private configuration tree is unsafe or incomplete'
  [[ "$sunshine_private_state" =~ ^[0-9a-f]{64}$ ]] \
    || fail 'Sunshine private configuration tree digest is invalid'
  fingerprint="$(remote_normalized_fingerprint)" || fail 'normalized target does not match the InputPlumber 0.75.2 xb360 fingerprint exactly'
  if [[ "$require_physical" == true ]]; then
    controller_evidence="$(remote_physical_controller_evidence "$expected_identity" "$profile")" \
      || fail 'expected physical controller is not live, supported, and selected with the production profile'
  fi
  node="${fingerprint#node=}"
  node="${node%% *}"
  sudo -n -u "$runtime_user" test -r "$node" || fail 'runtime user cannot read normalized target'
  shopt -s nullglob
  for event in /sys/class/input/event*; do
    event_node="/dev/input/${event##*/}"
    if [[ "$event_node" == "$node" ]]; then
      current_fingerprint="$(remote_normalized_fingerprint)" || current_fingerprint=''
      [[ "$current_fingerprint" != "$fingerprint" ]] || continue
    fi
    if udevadm info --query=property --name="$event_node" 2>/dev/null | grep -Fx 'ID_INPUT_JOYSTICK=1' >/dev/null \
      && sudo -n -u "$runtime_user" test -r "$event_node"; then
      readable_raw=$((readable_raw + 1))
    fi
  done
  [[ "$readable_raw" -eq 0 ]] || fail "runtime user can read $readable_raw raw controller node(s)"
  remote_dbus_unique_owner org.shadowblip.InputPlumber >/dev/null \
    || fail 'InputPlumber has no unique DBus owner'
  busctl --system introspect org.shadowblip.InputPlumber /org/shadowblip/InputPlumber/devices/target/dbus0 org.shadowblip.Input.DBusDevice --no-pager 2>/dev/null \
    | grep -F 'InputEvent' >/dev/null || fail 'InputPlumber DBus target interface is unavailable'
  [[ "$(stat -fc %T /sys/fs/cgroup)" == cgroup2fs ]] || fail 'cgroup v2 is unavailable'
  delegate="$(systemctl show korri-inputd.service -p Delegate --value 2>/dev/null || true)"
  delegate_controllers="$(systemctl show korri-inputd.service -p DelegateControllers --value 2>/dev/null || true)"
  [[ "$delegate" == yes ]] || fail 'inputd Delegate is not enabled'
  [[ " $delegate_controllers " == *' pids '* ]] || fail 'inputd DelegateControllers does not contain pids'
  [[ "$(remote_catalog_health)" == Ok ]] || fail 'korrid catalog is unhealthy'
  acceptance="$(remote_acceptance_fingerprint "$runtime_user" "$expected_identity" "$profile" "$require_physical")" \
    || fail 'acceptance fingerprint could not be captured'
  printf 'automated-gates=pass raw-readable=0 inputd-status=Ready system-korrid=active system-korri-compositor=active system-sunshine=active pairing-state=present credentials=service-specific sunshine-package=attested catalog=Ok delegate=yes controllers=pids\n'
  printf '%s\n' "$sunshine_provenance"
  printf 'sunshine-private-state=protected digest=%s\n' "$sunshine_private_state"
  printf 'normalized-fingerprint=%s\n' "$fingerprint"
  [[ "$require_physical" != true ]] || printf 'controller-evidence=%s\n' "$controller_evidence"
  printf 'acceptance-fingerprint=%s\n' "$acceptance"
}

remote_rollback_gates() {
  local runtime_user="$1" socket_was_active system_was_active user_was_active=false state
  socket_was_active="$(remote_system_unit_active "$KORRID_CONTROL_SOCKET_UNIT")" \
    || fail 'system korrid socket activity is unavailable before rollback gate'
  system_was_active="$(remote_system_unit_active korrid.service)" \
    || fail 'system korrid activity is unavailable before rollback gate'
  state="$(remote_user_unit_active "$runtime_user" korrid.service)" \
    || fail 'old user korrid active state is unavailable before rollback gate'
  [[ "$state" != true ]] || user_was_active=true
  trap 'remote_restore_launch_authority "$runtime_user" "$socket_was_active" "$system_was_active" "$user_was_active"' EXIT
  remote_quiesce_launch_authority "$runtime_user" || fail 'launch authority could not be quiesced for rollback gate'
  remote_refuse_active_game "$runtime_user"
  remote_temporary_artifacts_dirty && fail 'temporary U7 artifacts remain after rollback'
  remote_private_session_state_absent || fail 'private launch recovery state remains after rollback'
  remote_restore_launch_authority "$runtime_user" "$socket_was_active" "$system_was_active" "$user_was_active"
  trap - EXIT
  printf 'rollback-gates=pass\n'
}

remote_inject_health_failure() {
  local rollback="$1" persistent="$2" runtime_user="$3" status='' attempt
  shift 3
  remote_quiesce_for_private_state_cut "$runtime_user" health-failure || return $?
  sudo -n systemctl stop inputplumber.service
  for ((attempt = 1; attempt <= POLL_ATTEMPTS; attempt++)); do
    status="$(systemctl show korri-inputd.service -p StatusText --value 2>/dev/null || true)"
    [[ "$status" == Recovering || "$status" == Missing ]] && break
    sleep "$POLL_DELAY"
  done
  [[ "$status" == Recovering || "$status" == Missing ]] || return 1
  remote_restore "$rollback" "$persistent" "$runtime_user" "$@"
}

remote_persistent_switch() {
  local candidate="$1" runtime_user="$2"
  remote_quiesce_for_private_state_cut "$runtime_user" candidate || return $?
  sudo -n nix-env -p /nix/var/nix/profiles/system --set "$candidate"
  remote_activate_generation "$candidate" switch
  remote_switch_generation_bundle "$candidate"
  remote_disable_old_user_units "$runtime_user"
  remote_start_candidate_services "$runtime_user"
  [[ "$(remote_generation)" == "$candidate" ]]
}

remote_attempt_validate() {
  local nonce="$1" candidate="$2" helper
  helper="$candidate/sw/bin/korri-device-gate"
  [[ "$0" == "$helper" && -f "$ATTEMPT_MARKER" ]] || return 1
  [[ "$(stat -c '%u:%g:%a' "$ATTEMPT_MARKER" 2>/dev/null)" == 0:0:600 ]] || return 1
  grep -Fx "nonce=$nonce" "$ATTEMPT_MARKER" >/dev/null \
    && grep -Fx "candidate=$candidate" "$ATTEMPT_MARKER" >/dev/null \
    && grep -Fx "helper=$helper" "$ATTEMPT_MARKER" >/dev/null
}

remote_attempt_start_root() {
  local nonce="$1" candidate="$2" ttl="$3" helper marker_next
  helper="$candidate/sw/bin/korri-device-gate"
  [[ "$0" == "$helper" && "$nonce" =~ ^[0-9a-f]{64}$ ]] || return 1
  valid_generation_path "$candidate" || return 1
  [[ "$ttl" =~ ^[1-9][0-9]*$ ]] || return 1
  install -d -m0700 -o root -g root "${ATTEMPT_MARKER%/*}"
  [[ ! -e "$ATTEMPT_MARKER" ]] || fail 'another device-gate attempt marker already exists'
  systemctl is-active --quiet "$ATTEMPT_UNIT" 2>/dev/null \
    && fail 'another device-gate attempt is still active'
  marker_next="$ATTEMPT_MARKER.$$.next"
  umask 077
  {
    printf 'nonce=%s\n' "$nonce"
    printf 'candidate=%s\n' "$candidate"
    printf 'helper=%s\n' "$helper"
  } >"$marker_next"
  chown root:root "$marker_next"
  chmod 0600 "$marker_next"
  mv -T "$marker_next" "$ATTEMPT_MARKER"
  sync -f "$ATTEMPT_MARKER"
  sync -f "${ATTEMPT_MARKER%/*}"
  systemctl reset-failed "$ATTEMPT_UNIT" >/dev/null 2>&1 || true
  if ! systemd-run --quiet --collect --unit="$ATTEMPT_UNIT" \
    --property="RuntimeMaxSec=${ttl}s" --setenv="PATH=$candidate/sw/bin" \
    -- "$helper" --remote attempt-holder "$nonce" "$candidate" "$ttl"; then
    rm -f "$ATTEMPT_MARKER"
    sync -f "${ATTEMPT_MARKER%/*}"
    return 1
  fi
  systemctl is-active --quiet "$ATTEMPT_UNIT" || {
    rm -f "$ATTEMPT_MARKER"
    sync -f "${ATTEMPT_MARKER%/*}"
    return 1
  }
}

remote_attempt_finish_root() {
  local nonce="$1" candidate="$2"
  remote_attempt_validate "$nonce" "$candidate" || fail 'device-gate attempt marker does not match this private attempt'
  systemctl stop "$ATTEMPT_UNIT"
  rm -f "$ATTEMPT_MARKER"
  sync -f "${ATTEMPT_MARKER%/*}"
}

remote_attempt_reconcile_root() {
  local nonce="$1" candidate="$2" require_marker="${3:-false}"
  if [[ ! -e "$ATTEMPT_MARKER" ]]; then
    [[ "$require_marker" == false ]] || fail 'in-progress device-gate marker is missing'
    return 0
  fi
  remote_attempt_validate "$nonce" "$candidate" || fail 'stale device-gate marker does not match this private attempt'
  ! systemctl is-active --quiet "$ATTEMPT_UNIT" \
    || fail 'device-gate attempt is still live; reconcile refuses to race it'
  rm -f "$ATTEMPT_MARKER"
  sync -f "${ATTEMPT_MARKER%/*}"
}

remote_nvenc_stream_log_gate() {
  local log="$1" latest_start session_log first_h264
  grep -F "Found display [$COMPOSITOR_WAYLAND_ALIAS]" <<<"$log" >/dev/null \
    || { printf 'current Sunshine invocation did not connect to the stable Wayland display\n' >&2; return 1; }
  grep -F -- '-------- Start of Wayland monitor list --------' <<<"$log" >/dev/null \
    || { printf 'current Sunshine invocation did not enumerate Wayland outputs\n' >&2; return 1; }
  grep -F 'Selected monitor [' <<<"$log" >/dev/null \
    || { printf 'current Sunshine invocation did not select a Wayland monitor\n' >&2; return 1; }
  ! grep -F 'Streaming display:' <<<"$log" >/dev/null \
    || { printf 'current Sunshine invocation used X11 capture\n' >&2; return 1; }
  ! grep -F 'Unsupported screencopy SHM format:' <<<"$log" >/dev/null \
    || { printf 'current Sunshine invocation rejected a Wayland SHM format\n' >&2; return 1; }
  ! grep -F 'Frame capture failed' <<<"$log" >/dev/null \
    || { printf 'current Sunshine invocation reported a capture failure\n' >&2; return 1; }
  ! grep -F "Couldn't scale frame:" <<<"$log" >/dev/null \
    || { printf 'current Sunshine invocation reported a frame scaling failure\n' >&2; return 1; }
  latest_start="$(grep -F 'New streaming session started' <<<"$log" | tail -n 1)"
  [[ "$latest_start" == *'[active sessions: 1]'* ]] \
    || { printf 'latest NVENC stream is absent or concurrent\n' >&2; return 1; }
  session_log="$(awk '
    /New streaming session started/ { seen = 1; block = ""; next }
    seen { block = block $0 "\\n" }
    END { printf "%s", block }
  ' <<<"$log")"
  grep -F 'CLIENT CONNECTED' <<<"$session_log" >/dev/null \
    || { printf 'latest NVENC stream never connected\n' >&2; return 1; }
  first_h264="$(grep -F 'Creating encoder [h264_' <<<"$session_log" | head -n 1)"
  [[ "$first_h264" == *'Creating encoder [h264_nvenc]'* ]] \
    || { printf 'latest explicit NVENC stream did not first create h264_nvenc\n' >&2; return 1; }
  ! grep -F 'Creating encoder [h264_vaapi]' <<<"$session_log" >/dev/null \
    || { printf 'latest explicit NVENC stream fell back to h264_vaapi\n' >&2; return 1; }
}

remote_nvenc_stream_gate() {
  local exec_start environment snapshot active main_pid invocation log
  exec_start="$(systemctl show sunshine.service -p ExecStart --value)" || return 1
  if [[ "$exec_start" != *' encoder=nvenc'* ]]; then
    printf 'nvenc-stream-gate=not-required\n'
    return 0
  fi
  environment="$(systemctl show sunshine.service -p Environment --value)" || return 1
  grep -F 'SUNSHINE_STRICT_ENCODER=1' <<<"$environment" >/dev/null \
    || { printf 'explicit NVENC service is not strict\n' >&2; return 1; }
  snapshot="$(systemctl show sunshine.service -p ActiveState -p MainPID -p InvocationID)" || return 1
  active="$(awk -F= '$1 == "ActiveState" {print $2}' <<<"$snapshot")"
  main_pid="$(awk -F= '$1 == "MainPID" {print $2}' <<<"$snapshot")"
  invocation="$(awk -F= '$1 == "InvocationID" {print $2}' <<<"$snapshot")"
  [[ "$active" == active && "$main_pid" =~ ^[1-9][0-9]*$ && "$invocation" =~ ^[0-9a-f]{32}$ ]] \
    || { printf 'current explicit NVENC Sunshine invocation is not live\n' >&2; return 1; }
  log="$(journalctl -b --no-pager -o cat -n 2000 \
    _SYSTEMD_UNIT=sunshine.service _SYSTEMD_INVOCATION_ID="$invocation")" || return 1
  remote_nvenc_stream_log_gate "$log" || return 1
  printf 'nvenc-stream-gate=pass encoder=h264_nvenc strict=yes capture=wayland invocation=current\n'
}

remote_attempt_execute_root() {
  local nonce="$1" candidate="$2" action="$3"
  shift 3
  remote_attempt_validate "$nonce" "$candidate" \
    || fail 'device-gate attempt marker does not match this private attempt'
  systemctl is-active --quiet "$ATTEMPT_UNIT" \
    || fail 'device-gate attempt lease is not active'
  case "$action" in
    predicates) remote_predicates "${1:?}" ;;
    boot-id) tr -d '\n' </proc/sys/kernel/random/boot_id ;;
    current-generation) remote_generation ;;
    acceptance-fingerprint) remote_acceptance_fingerprint "${1:?}" "${2:-}" "${3:-}" "${4:?}" ;;
    automated-gates) remote_automated_gates "${1:?}" "${2:-}" "${3:-}" "${4:?}" ;;
    compositor-game-gate) remote_compositor_game_gate "${1:?}" ;;
    nvenc-stream-gate) remote_nvenc_stream_gate ;;
    rollback-gates) remote_rollback_gates "${1:?}" ;;
    activate-test) remote_activate_test "${1:?}" "${2:?}" ;;
    inject-health-failure) remote_inject_health_failure "${1:?}" "${2:?}" "${3:?}" "${@:4}" ;;
    restore) remote_restore "${1:?}" "${2:?}" "${3:?}" "${@:4}" ;;
    persistent-switch) remote_persistent_switch "${1:?}" "${2:?}" ;;
    *) fail "unknown attempt action: $action" ;;
  esac
}

if [[ "${1:-}" == --remote ]]; then
  action="${2:-}"
  shift 2
  if [[ "$action" == deadline ]]; then
    deadline="${1:?}"
    shift
    exec timeout --signal=TERM --kill-after=5s "${deadline}s" "$0" --remote "$@"
  fi
  if [[ "$action" == locked-root ]]; then
    deadline="${1:?}"
    lock_wait="${2:?}"
    shift 2
    exec sudo -n flock --wait "$lock_wait" --conflict-exit-code 75 "$GATE_LOCK" \
      timeout --signal=TERM --kill-after=5s "${deadline}s" "$0" --remote "$@"
  fi
  if [[ "$action" == attempt-command ]]; then
    deadline="${1:?}"
    lock_wait="${2:?}"
    shift 2
    exec sudo -n flock --wait "$lock_wait" --conflict-exit-code 75 "$GATE_LOCK" \
      timeout --signal=TERM --kill-after=5s "${deadline}s" "$0" --remote attempt-execute-root "$@"
  fi
  case "$action" in
    inspect) remote_inspect "${1:?}" ;;
    predicates) remote_predicates "${1:?}" ;;
    preflight) remote_preflight "${1:?}" "${2:?}" "${3:-}" "${4:-}" ;;
    boot-id) tr -d '\n' </proc/sys/kernel/random/boot_id ;;
    current-generation) remote_generation ;;
    nvenc-stream-gate) remote_nvenc_stream_gate ;;
    bitmap-codes) remote_bitmap_codes "${1:?}" "${2:-}" ;;
    attempt-start-root) remote_attempt_start_root "${1:?}" "${2:?}" "${3:?}" ;;
    attempt-finish-root) remote_attempt_finish_root "${1:?}" "${2:?}" ;;
    attempt-reconcile-root) remote_attempt_reconcile_root "${1:?}" "${2:?}" "${3:-false}" ;;
    attempt-execute-root) remote_attempt_execute_root "${1:?}" "${2:?}" "${3:?}" "${@:4}" ;;
    attempt-holder)
      remote_attempt_validate "${1:?}" "${2:?}" || exit 1
      exec sleep "${3:?}"
      ;;
    *) fail "unknown remote action: $action" ;;
  esac
  exit 0
fi

usage() {
  cat >&2 <<'EOF'
usage: device-check.sh --host HOST --expected-machine-id ID --expected-hostname NAME --candidate GENERATION --runtime-user USER [options]
Modes: inspect, reconcile, candidate-test, inject-health-failure, rollback,
       rollback-reboot-verify, persistent-switch, candidate-reboot-verify
Mutation modes require --candidate, --rollback-generation, --runtime-user,
--ledger, and --confirm. Persistent/reboot candidate modes also require
--expected-controller-id BUS:VENDOR:PRODUCT:VERSION and --production-profile.
HITL tokens are accepted only from /dev/tty.
EOF
}

HOST='' EXPECTED_MACHINE_ID='' EXPECTED_HOSTNAME='' MODE=inspect
CANDIDATE='' ROLLBACK='' RUNTIME_USER='' LEDGER='' CONFIRM=''
EXPECTED_CONTROLLER_ID='' PRODUCTION_PROFILE=''
while [[ $# -gt 0 ]]; do
  case "$1" in
    --host) HOST="${2:-}"; shift 2 ;;
    --expected-machine-id) EXPECTED_MACHINE_ID="${2:-}"; shift 2 ;;
    --expected-hostname) EXPECTED_HOSTNAME="${2:-}"; shift 2 ;;
    --mode) MODE="${2:-}"; shift 2 ;;
    --candidate) CANDIDATE="${2:-}"; shift 2 ;;
    --rollback-generation) ROLLBACK="${2:-}"; shift 2 ;;
    --runtime-user) RUNTIME_USER="${2:-}"; shift 2 ;;
    --ledger) LEDGER="${2:-}"; shift 2 ;;
    --confirm) CONFIRM="${2:-}"; shift 2 ;;
    --expected-controller-id) EXPECTED_CONTROLLER_ID="${2:-}"; shift 2 ;;
    --production-profile) PRODUCTION_PROFILE="${2:-}"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage; fail "unknown argument: $1" ;;
  esac
done

[[ -n "$HOST" ]] || { usage; fail 'an explicit --host is required'; }
[[ -n "$EXPECTED_MACHINE_ID" && -n "$EXPECTED_HOSTNAME" ]] || fail 'explicit --expected-machine-id and --expected-hostname are required'
[[ "$HOST" =~ ^[A-Za-z0-9][A-Za-z0-9.-]*$ ]] || fail 'invalid host target'
[[ "$EXPECTED_MACHINE_ID" =~ ^[0-9a-f]{32}$ ]] || fail 'expected machine-id must be 32 lowercase hexadecimal characters'
[[ "$EXPECTED_HOSTNAME" =~ ^[A-Za-z0-9][A-Za-z0-9.-]*$ ]] || fail 'invalid expected hostname'
for timeout_value in "$REMOTE_COMMAND_TIMEOUT" "$LOCK_WAIT_TIMEOUT" "$LOCAL_SSH_TIMEOUT"; do
  [[ "$timeout_value" =~ ^[1-9][0-9]*$ && "$timeout_value" -le 1200 ]] || fail 'gate timeouts must be 1 through 1200 seconds'
done
for timeout_value in "$HITL_READ_TIMEOUT" "$HITL_OVERALL_TIMEOUT" "$ATTEMPT_TIMEOUT"; do
  [[ "$timeout_value" =~ ^[1-9][0-9]*$ && "$timeout_value" -le 14400 ]] || fail 'HITL and attempt timeouts must be 1 through 14400 seconds'
done
((HITL_READ_TIMEOUT <= HITL_OVERALL_TIMEOUT)) || fail 'HITL read timeout must not exceed the overall HITL timeout'
((HITL_OVERALL_TIMEOUT < ATTEMPT_TIMEOUT)) || fail 'attempt timeout must exceed the overall HITL timeout'
((REMOTE_COMMAND_TIMEOUT < LOCAL_SSH_TIMEOUT)) || fail 'remote command timeout must be shorter than local SSH timeout'
((REMOTE_COMMAND_TIMEOUT + LOCK_WAIT_TIMEOUT < LOCAL_SSH_TIMEOUT)) \
  || fail 'local SSH timeout must exceed the remote deadline plus gate-lock wait'
case "$MODE" in
  inspect|reconcile|candidate-test|inject-health-failure|rollback|rollback-reboot-verify|persistent-switch|candidate-reboot-verify) ;;
  *) fail "unknown mode: $MODE" ;;
esac

valid_generation_path "$CANDIDATE" || fail 'candidate must be a strictly valid Nix store generation path in every mode'
[[ -z "$ROLLBACK" ]] || valid_generation_path "$ROLLBACK" || fail 'rollback must be a strictly valid Nix store generation path'
[[ -z "$EXPECTED_CONTROLLER_ID" || "$EXPECTED_CONTROLLER_ID" =~ ^[0-9a-f]{4}:[0-9a-f]{4}:[0-9a-f]{4}:[0-9a-f]{4}$ ]] \
  || fail 'expected controller identity must be exact lowercase BUS:VENDOR:PRODUCT:VERSION hexadecimal'
[[ -z "$PRODUCTION_PROFILE" || "$PRODUCTION_PROFILE" == "$SUPPORTED_PRODUCTION_PROFILE" ]] \
  || fail "unsupported production profile; this gate supports only $SUPPORTED_PRODUCTION_PROFILE"
if [[ "$MODE" == candidate-test || "$MODE" == persistent-switch || "$MODE" == candidate-reboot-verify ]]; then
  [[ -n "$EXPECTED_CONTROLLER_ID" && -n "$PRODUCTION_PROFILE" ]] \
    || fail 'candidate, persistent, and reboot modes require an explicit expected controller identity and production profile'
fi
[[ -n "$RUNTIME_USER" && "$RUNTIME_USER" =~ ^[a-z_][a-z0-9_-]*$ ]] \
  || fail 'every mode requires an explicit runtime user'
if [[ "$MODE" != inspect ]]; then
  valid_generation_path "$ROLLBACK" || fail 'mutation and reconcile modes require a strictly valid Nix store rollback generation path'
  [[ -n "$LEDGER" ]] || fail 'mutation and reconcile modes require a private --ledger directory outside the repository'
fi

ssh_options=(-o BatchMode=yes -o ConnectTimeout=5 -o ServerAliveInterval=5 -o ServerAliveCountMax=2)
remote_quote() {
  local value="$1"
  printf "'%s'" "${value//\'/\'\\\'\'}"
}
ssh_transport() {
  local command='' arg quoted
  for arg in "$@"; do
    quoted="$(remote_quote "$arg")"
    command+="${command:+ }$quoted"
  done
  timeout --signal=TERM --kill-after=5s "${LOCAL_SSH_TIMEOUT}s" \
    "$SSH_BIN" "${ssh_options[@]}" "$HOST" "$command"
}
ssh_exec_deadlined() {
  ssh_transport timeout --signal=TERM --kill-after=5s "${REMOTE_COMMAND_TIMEOUT}s" "$@"
}

REMOTE_HELPER="$CANDIDATE/sw/bin/korri-device-gate"
helper_stat="$(ssh_exec_deadlined stat -Lc '%u:%a' -- "$REMOTE_HELPER")" \
  || fail 'candidate does not expose the immutable device-gate helper'
helper_uid="${helper_stat%%:*}"
helper_mode="${helper_stat#*:}"
[[ "$helper_uid" == 0 && "$helper_mode" =~ ^[0-7]{3,4}$ ]] \
  || fail 'candidate device-gate helper is not root-owned executable store content'
(( (8#$helper_mode & 8#111) != 0 && (8#$helper_mode & 8#222) == 0 )) \
  || fail 'candidate device-gate helper must be executable and not writable'
local_helper_digest="$(sha256sum "${BASH_SOURCE[0]}" | cut -d' ' -f1)"
remote_helper_digest="$(ssh_exec_deadlined sha256sum -- "$REMOTE_HELPER" | awk '{print $1}')" \
  || fail 'could not hash candidate device-gate helper'
[[ "$remote_helper_digest" == "$local_helper_digest" ]] \
  || fail 'candidate device-gate helper digest does not match the local gate source'

actual_machine_id="$(ssh_exec_deadlined cat /etc/machine-id | tr -d '\r\n')" || fail 'could not read remote machine-id'
actual_hostname="$(ssh_exec_deadlined hostname | tr -d '\r\n')" || fail 'could not read remote hostname'
[[ "$actual_machine_id" == "$EXPECTED_MACHINE_ID" ]] || fail "remote machine-id mismatch for $HOST"
[[ "$actual_hostname" == "$EXPECTED_HOSTNAME" ]] || fail "remote hostname mismatch for $HOST"

local_temp_ledger=''
mutation_active=false verification_active=false reconcile_active=false attempt_remote_active=false rollback_persistent=false
ledger_identity='' state_content='' baseline_predicates='' accepted_private_state=''
attempt_nonce='' attempt_boot_id=''
old_korrid_active=false old_korrid_enabled=false
old_sunshine_active=false old_sunshine_enabled=false
old_x11_active=false old_x11_enabled=false
old_pairing_config_mode='' old_pairing_state_mode=''
failure_resume_boot_id='' verification_resume_state=''
write_state() {
  local next="$1" boot_id="${2:-}" resume_state="${3:-}" attempt_nonce="${4:-}" next_content
  case "$next" in
    pending-mutation|pending-mutation-starting|rollback-reboot-verifying|rollback-reboot-verifying-starting|candidate-reboot-verifying|candidate-reboot-verifying-starting)
      [[ "$attempt_nonce" =~ ^[0-9a-f]{64}$ ]] || fail "in-progress state $next requires an exact private attempt nonce"
      ;;
  esac
  next_content="$(
    printf 'state=%s\n' "$next"
    printf 'machine_id=%s\n' "$actual_machine_id"
    printf 'hostname=%s\n' "$actual_hostname"
    printf 'candidate=%s\n' "$CANDIDATE"
    printf 'rollback=%s\n' "$ROLLBACK"
    printf 'expected_controller_id=%s\n' "$EXPECTED_CONTROLLER_ID"
    printf 'production_profile=%s\n' "$PRODUCTION_PROFILE"
    printf 'boot_id=%s\n' "$boot_id"
    printf 'resume_state=%s\n' "$resume_state"
    printf 'attempt_nonce=%s\n' "$attempt_nonce"
  )"
  printf '%s\n' "$next_content" | write_replace_ledger_proof state \
    || fail 'ledger state could not be replaced safely'
  state_content="$next_content"
}

run_remote_deadlined() {
  ssh_transport "$REMOTE_HELPER" --remote deadline "$REMOTE_COMMAND_TIMEOUT" "$@"
}
run_remote_control() {
  ssh_transport "$REMOTE_HELPER" --remote locked-root "$REMOTE_COMMAND_TIMEOUT" "$LOCK_WAIT_TIMEOUT" "$@"
}
run_remote_attempt() {
  [[ "$attempt_nonce" =~ ^[0-9a-f]{64}$ ]] || fail 'private attempt nonce is unavailable'
  ssh_transport "$REMOTE_HELPER" --remote attempt-command "$REMOTE_COMMAND_TIMEOUT" "$LOCK_WAIT_TIMEOUT" \
    "$attempt_nonce" "$CANDIDATE" "$@"
}

cleanup() {
  local status=$? rollback_ok=false
  trap - EXIT INT TERM
  set +e
  if [[ "$mutation_active" == true && -n "$ROLLBACK" ]]; then
    if [[ "$attempt_remote_active" == true ]]; then
      if run_remote_attempt restore "$ROLLBACK" "$rollback_persistent" "$RUNTIME_USER" \
        "$old_korrid_active" "$old_korrid_enabled" "$old_sunshine_active" "$old_sunshine_enabled" \
        "$old_x11_active" "$old_x11_enabled" "$old_pairing_config_mode" "$old_pairing_state_mode" >/dev/null 2>&1; then
        rollback_ok=true
      else
        printf 'device gate: cleanup rollback failed or could not acquire the root gate lock; inspect the recorded rollback generation without retrying mutation\n' >&2
      fi
      run_remote_control attempt-finish-root "$attempt_nonce" "$CANDIDATE" >/dev/null 2>&1 || true
      attempt_remote_active=false
    fi
    if [[ -n "$ledger_identity" ]]; then
      write_state failed-needs-inspection "${failure_resume_boot_id:-}" "${resume_after_failure:-}" "$attempt_nonce"
      printf 'device gate: mutation failed; fresh reconcile is required before retry (rollback=%s)\n' "$rollback_ok" >&2
    fi
  elif [[ "$verification_active" == true && -n "$ledger_identity" ]]; then
    [[ "$attempt_remote_active" != true ]] \
      || run_remote_control attempt-finish-root "$attempt_nonce" "$CANDIDATE" >/dev/null 2>&1 || true
    write_state failed-needs-inspection "${failure_resume_boot_id:-}" "$verification_resume_state" "$attempt_nonce"
    if [[ "$verification_resume_state" == candidate-await-reboot ]]; then
      printf 'device gate: candidate reboot verification failed; fresh reconcile is required before retry\n' >&2
    else
      printf 'device gate: rollback reboot verification failed; fresh reconcile is required before retry\n' >&2
    fi
  elif [[ "$reconcile_active" == true && -n "$ledger_identity" ]]; then
    write_state failed-needs-inspection "${failure_resume_boot_id:-}" "$verification_resume_state" "$attempt_nonce"
    printf 'device gate: stale attempt reconciliation failed; inspection is required before retry\n' >&2
  fi
  [[ -z "$local_temp_ledger" ]] || rm -rf -- "$local_temp_ledger"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ "$MODE" == inspect ]]; then
  [[ -z "$CONFIRM$ROLLBACK$LEDGER$EXPECTED_CONTROLLER_ID$PRODUCTION_PROFILE" ]] \
    || fail 'inspect mode refuses mutation-only arguments'
  umask 077
  local_temp_ledger="$(mktemp -d "${TMPDIR:-/tmp}/korri-device-inspect.XXXXXX")"
  run_remote_deadlined inspect "$RUNTIME_USER" >"$local_temp_ledger/inspection.txt"
  chmod 0600 "$local_temp_ledger/inspection.txt"
  cat "$local_temp_ledger/inspection.txt"
  printf 'inspection=complete mutation=none ledger=private-temporary\n'
  exit 0
fi

select_ledger_proof_helper() {
  local gate_real="$1" override="$2"
  if [[ -n "$override" ]]; then
    [[ "$gate_real" != /nix/store/* ]] \
      || fail 'immutable device gate refuses a ledger proof helper override'
    printf '%s\n' "$override"
  else
    [[ -n "$gate_real" ]] || fail 'device gate path is unavailable'
    printf '%s/korri-ledger-proof\n' "$(dirname "$gate_real")"
  fi
}

ledger_proof_helper() {
  local gate_real
  if [[ -z "$LEDGER_PROOF_HELPER" ]]; then
    gate_real="$(realpath -e -- "$0" 2>/dev/null || true)"
    LEDGER_PROOF_HELPER="$(select_ledger_proof_helper "$gate_real" "${KORRI_LEDGER_PROOF_HELPER:-}")"
  fi
  [[ -x "$LEDGER_PROOF_HELPER" ]] || fail 'ledger proof helper is unavailable'
  printf '%s\n' "$LEDGER_PROOF_HELPER"
}

test_gate_hook() {
  local name="$1" gate_real ready release
  [[ "${KORRI_DEVICE_GATE_TEST_HOOK:-}" == "$name" ]] || return 0
  gate_real="$(realpath -e -- "$0" 2>/dev/null || true)"
  [[ "$gate_real" != /nix/store/* ]] || fail 'immutable device gate refuses test hooks'
  ready="${KORRI_DEVICE_GATE_TEST_HOOK_READY:?}"
  release="${KORRI_DEVICE_GATE_TEST_HOOK_RELEASE:?}"
  : >"$ready"
  while [[ ! -e "$release" ]]; do sleep 0.01; done
}

read_ledger_proof() {
  local name="$1" helper
  helper="$(ledger_proof_helper)"
  "$helper" read "$LEDGER" "$ledger_identity" "$name"
}

read_optional_ledger_proof() {
  local name="$1" helper
  helper="$(ledger_proof_helper)"
  "$helper" read-optional "$LEDGER" "$ledger_identity" "$name"
}

write_new_ledger_proof() {
  local name="$1" helper
  helper="$(ledger_proof_helper)"
  "$helper" write-new "$LEDGER" "$ledger_identity" "$name"
}

write_replace_ledger_proof() {
  local name="$1" helper
  helper="$(ledger_proof_helper)"
  "$helper" write-replace "$LEDGER" "$ledger_identity" "$name"
}

root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
ledger_parent="$(dirname "$LEDGER")"
mkdir -p "$ledger_parent"
ledger_parent="$(cd "$ledger_parent" && pwd -P)"
LEDGER="$ledger_parent/$(basename "$LEDGER")"
case "$LEDGER/" in "$root/"*) fail 'ledger must be outside the repository' ;; esac
if [[ -e "$LEDGER" ]]; then
  [[ -d "$LEDGER" && ! -L "$LEDGER" && "$(stat -c %a "$LEDGER")" == 700 ]] || fail 'existing ledger must be a real mode 0700 directory'
else
  mkdir -m 0700 "$LEDGER"
fi
umask 077
helper="$(ledger_proof_helper)"
ledger_identity="$("$helper" identity "$LEDGER")" \
  || fail 'ledger directory identity could not be captured safely'
[[ "$ledger_identity" =~ ^[0-9]+:[0-9]+$ ]] || fail 'ledger directory identity is invalid'
state_content="$(read_optional_ledger_proof state)" \
  || fail 'ledger state is unsafe or unreadable'
test_gate_hook after-state-read
state="$(awk -F= '$1 == "state" {print $2}' <<<"$state_content")"
if [[ -n "$state_content" ]]; then
  for state_key in state machine_id hostname candidate rollback expected_controller_id production_profile boot_id resume_state attempt_nonce; do
    [[ "$(grep -c "^$state_key=" <<<"$state_content")" -eq 1 ]] \
      || fail 'ledger state has duplicate or incomplete fields'
  done
  grep -Fx "machine_id=$actual_machine_id" <<<"$state_content" >/dev/null || fail 'ledger machine identity differs from target'
  grep -Fx "hostname=$actual_hostname" <<<"$state_content" >/dev/null || fail 'ledger hostname differs from target'
  grep -Fx "candidate=$CANDIDATE" <<<"$state_content" >/dev/null || fail 'ledger candidate differs from request'
  grep -Fx "rollback=$ROLLBACK" <<<"$state_content" >/dev/null || fail 'ledger rollback differs from request'
  ledger_controller="$(awk -F= '$1 == "expected_controller_id" {print $2}' <<<"$state_content")"
  ledger_profile="$(awk -F= '$1 == "production_profile" {print $2}' <<<"$state_content")"
  [[ -z "$ledger_controller" || "$ledger_controller" == "$EXPECTED_CONTROLLER_ID" ]] || fail 'ledger controller identity differs from target'
  [[ -z "$ledger_profile" || "$ledger_profile" == "$PRODUCTION_PROFILE" ]] || fail 'ledger production profile differs from target'
fi

if [[ "$MODE" == candidate-reboot-verify && "$state" == candidate-await-reboot ]]; then
  prior_boot="$(awk -F= '$1 == "boot_id" {print $2}' <<<"$state_content")"
  failure_resume_boot_id="$prior_boot"
  verification_resume_state='candidate-await-reboot'
fi

preflight="$(run_remote_deadlined preflight "$CANDIDATE" "$ROLLBACK" "$EXPECTED_CONTROLLER_ID" "$PRODUCTION_PROFILE")"
remote_candidate="$(awk -F= '$1 == "candidate" {print substr($0, index($0, "=") + 1)}' <<<"$preflight")"
remote_rollback="$(awk -F= '$1 == "rollback" {print substr($0, index($0, "=") + 1)}' <<<"$preflight")"
[[ "$remote_candidate" == "$CANDIDATE" ]] || fail 'candidate generation is unavailable or not canonical on the target'
[[ "$remote_rollback" == "$ROLLBACK" ]] || fail 'rollback generation is unavailable or not canonical on the target'
grep -Fx 'candidate-switch=yes' <<<"$preflight" >/dev/null || fail 'candidate has no switch-to-configuration executable'
grep -Fx 'rollback-switch=yes' <<<"$preflight" >/dev/null || fail 'rollback generation has no switch-to-configuration executable'
grep -Fx 'temporary-artifacts-dirty=no' <<<"$preflight" >/dev/null || fail 'dirty or untracked U7 temporary devices/profiles are present'
if [[ "$MODE" == candidate-test || "$MODE" == persistent-switch || "$MODE" == candidate-reboot-verify ]]; then
  grep -Fx 'expected-controller=yes' <<<"$preflight" >/dev/null \
    || fail 'expected supported physical controller identity is not uniquely live'
fi

identity_key="$actual_machine_id|$actual_hostname|$CANDIDATE"
expected_confirm="CONFIRM-$(printf '%s' "$identity_key" | sha256sum | cut -c1-16)"
if [[ "$MODE" != reconcile && "$CONFIRM" != "$expected_confirm" ]]; then
  printf 'Required confirmation for host=%s candidate=%s: %s\n' "$actual_hostname" "$CANDIDATE" "$expected_confirm" >&2
  fail 'mutation confirmation token is missing or does not match the captured host and candidate generation'
fi

validate_private_state_predicates() {
  local predicates="$1" present digest
  [[ "$(grep -c '^sunshine.pairing-state-present=' <<<"$predicates")" -eq 1
    && "$(grep -c '^sunshine.private-state-digest=' <<<"$predicates")" -eq 1 ]] \
    || fail 'Sunshine baseline has duplicate or incomplete private-state predicates'
  present="$(awk -F= '$1 == "sunshine.pairing-state-present" {print $2}' <<<"$predicates")"
  digest="$(awk -F= '$1 == "sunshine.private-state-digest" {print $2}' <<<"$predicates")"
  [[ "$present" == true ]] || fail 'Sunshine baseline pairing state is absent'
  [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || fail 'Sunshine baseline private-state digest is invalid'
}


extract_automated_private_digest() {
  local evidence="$1" digest
  digest="$(sed -n 's/^sunshine-private-state=protected digest=//p' <<<"$evidence")"
  [[ "$(grep -c '^sunshine-private-state=protected digest=' <<<"$evidence")" -eq 1
    && "$digest" =~ ^[0-9a-f]{64}$ ]] \
    || fail 'automated Sunshine private-state proof is invalid'
  printf '%s\n' "$digest"
}

save_accepted_private_digest() {
  local evidence="$1" digest
  digest="$(extract_automated_private_digest "$evidence")"
  printf '%s\n' "$digest" | write_new_ledger_proof sunshine-private-state.accepted \
    || fail 'accepted Sunshine private-state proof could not be stored safely'
  if [[ "${KORRI_DEVICE_GATE_TEST_FAIL_AFTER_ACCEPTED_PROOF:-}" == true ]]; then
    [[ "$(realpath -e -- "$0")" != /nix/store/* ]] || fail 'immutable device gate refuses test failure injection'
    fail 'modeled failure after accepted Sunshine proof commit'
  fi
}

verify_accepted_private_digest() {
  local evidence="$1" current
  if [[ ! "${accepted_private_state:-}" =~ ^[0-9a-f]{64}$ ]]; then
    accepted_private_state="$(read_ledger_proof sunshine-private-state.accepted 2>/dev/null)" \
      || fail 'accepted Sunshine private-state proof is absent or unsafe'
  fi
  [[ "$accepted_private_state" =~ ^[0-9a-f]{64}$ ]] \
    || fail 'accepted Sunshine private-state proof is invalid'
  current="$(extract_automated_private_digest "$evidence")"
  [[ "$current" == "$accepted_private_state" ]] \
    || fail 'Sunshine private state changed across candidate reboot'
}

if baseline_predicates="$(read_ledger_proof baseline.predicates 2>/dev/null)"; then
  :
else
  [[ -z "$state_content" ]] || fail 'ledger state exists without a safe baseline proof'
  baseline_predicates="$(run_remote_deadlined predicates "$RUNTIME_USER")"
  grep -Fx "generation.current=$ROLLBACK" <<<"$baseline_predicates" >/dev/null \
    || fail 'baseline current generation is not the rollback generation'
  grep -Fx "generation.default=$ROLLBACK" <<<"$baseline_predicates" >/dev/null \
    || fail 'baseline default generation is not the rollback generation'
  validate_private_state_predicates "$baseline_predicates"
  printf '%s\n' "$baseline_predicates" | write_new_ledger_proof baseline.predicates \
    || fail 'Sunshine baseline proof could not be stored safely'
  baseline_predicates="$(read_ledger_proof baseline.predicates 2>/dev/null)" \
    || fail 'Sunshine baseline proof could not be read safely'
  run_remote_deadlined inspect "$RUNTIME_USER" >"$LEDGER/baseline.txt"
  chmod 0600 "$LEDGER/baseline.txt"
  sync -f "$LEDGER/baseline.txt"
fi
validate_private_state_predicates "$baseline_predicates"
grep -Fx "generation.current=$ROLLBACK" <<<"$baseline_predicates" >/dev/null \
  || fail 'retained baseline current generation differs from requested rollback'
grep -Fx "generation.default=$ROLLBACK" <<<"$baseline_predicates" >/dev/null \
  || fail 'retained baseline default generation differs from requested rollback'
test_gate_hook before-accepted-proof-read
case "$state" in
  candidate-accepted-pending-boot|candidate-await-reboot|candidate-reboot-verifying|candidate-reboot-verifying-starting|complete)
    accepted_private_state="$(read_ledger_proof sunshine-private-state.accepted 2>/dev/null)" \
      || fail 'accepted Sunshine private-state proof is absent or unsafe'
    [[ "$accepted_private_state" =~ ^[0-9a-f]{64}$ ]] \
      || fail 'accepted Sunshine private-state proof is invalid'
    ;;
esac
old_korrid_active="$(awk -F= '$1 == "old-user.korrid.active" {print $2}' <<<"$baseline_predicates")"
old_korrid_enabled="$(awk -F= '$1 == "old-user.korrid.enabled" {print $2}' <<<"$baseline_predicates")"
old_sunshine_active="$(awk -F= '$1 == "old-user.sunshine.active" {print $2}' <<<"$baseline_predicates")"
old_sunshine_enabled="$(awk -F= '$1 == "old-user.sunshine.enabled" {print $2}' <<<"$baseline_predicates")"
old_x11_active="$(awk -F= '$1 == "old-user.x11-headless.active" {print $2}' <<<"$baseline_predicates")"
old_x11_enabled="$(awk -F= '$1 == "old-user.x11-headless.enabled" {print $2}' <<<"$baseline_predicates")"
old_pairing_modes="$(awk -F= '$1 == "sunshine.pairing-state-modes" {print $2}' <<<"$baseline_predicates")"
old_pairing_config_mode="${old_pairing_modes%%:*}"
old_pairing_state_mode="${old_pairing_modes#*:}"
[[ "$old_pairing_config_mode" =~ ^[0-7]{3,4}$ && "$old_pairing_state_mode" =~ ^[0-7]{3,4}$ ]] \
  || fail 'invalid baseline Sunshine pairing-state modes'
for old_state in "$old_korrid_active" "$old_korrid_enabled" "$old_sunshine_active" \
  "$old_sunshine_enabled" "$old_x11_active" "$old_x11_enabled"; do
  [[ "$old_state" == true || "$old_state" == false ]] || fail 'invalid baseline old user-unit predicate'
done

resume_after_failure="$state"
start_attempt() {
  local in_progress_state="$1"
  attempt_nonce="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
  [[ "$attempt_nonce" =~ ^[0-9a-f]{64}$ ]] || fail 'could not generate attempt nonce'
  write_state "${in_progress_state}-starting" "$attempt_boot_id" "$resume_after_failure" "$attempt_nonce"
  run_remote_control attempt-start-root "$attempt_nonce" "$CANDIDATE" "$ATTEMPT_TIMEOUT"
  attempt_remote_active=true
  write_state "$in_progress_state" "$attempt_boot_id" "$resume_after_failure" "$attempt_nonce"
}
finish_attempt() {
  run_remote_control attempt-finish-root "$attempt_nonce" "$CANDIDATE"
  attempt_remote_active=false
}
verify_fingerprint_unchanged() {
  local evidence="$1" require_physical="$2" expected current
  expected="$(awk -F= '$1 == "acceptance-fingerprint" {print substr($0, index($0, "=") + 1)}' <<<"$evidence")"
  [[ -n "$expected" && "$(grep -c '^acceptance-fingerprint=' <<<"$evidence")" -eq 1 ]]     || fail 'automated acceptance fingerprint is invalid'
  current="$(run_remote_attempt acceptance-fingerprint "$RUNTIME_USER" "$EXPECTED_CONTROLLER_ID" "$PRODUCTION_PROFILE" "$require_physical")"
  printf '%s\n' "$expected" | write_replace_ledger_proof fingerprint.expected     || fail 'expected acceptance fingerprint could not be stored safely'
  printf '%s\n' "$current" | write_replace_ledger_proof fingerprint.current     || fail 'current acceptance fingerprint could not be stored safely'
  [[ "$(read_ledger_proof fingerprint.expected)" == "$expected"     && "$(read_ledger_proof fingerprint.current)" == "$current"     && "$expected" == "$current" ]]     || fail 'normalized target, Sunshine provenance, or expected physical controller proof changed before acceptance'
}

store_automated_evidence() {
  local name="$1" evidence="$2"
  printf '%s\n' "$evidence" | write_replace_ledger_proof "$name"     || fail "automated evidence could not be stored safely: $name"
  [[ "$(read_ledger_proof "$name")" == "$evidence" ]]     || fail "automated evidence could not be rebound safely: $name"
}

save_candidate_controller_proof() {
  local evidence="$1" controller
  controller="$(sed -n 's/^controller-evidence=//p' <<<"$evidence")"
  [[ -n "$controller" && "$(grep -c '^controller-evidence=' <<<"$evidence")" -eq 1 ]]     || fail 'candidate controller evidence is invalid'
  printf '%s\n' "$controller" | write_replace_ledger_proof candidate-controller.accepted     || fail 'candidate controller proof could not be stored safely'
}

compare_baseline() {
  local current
  if [[ "$attempt_remote_active" == true ]]; then
    current="$(run_remote_attempt predicates "$RUNTIME_USER")"
  else
    current="$(run_remote_deadlined predicates "$RUNTIME_USER")"
  fi
  [[ "$baseline_predicates" == "$current" ]] \
    || fail 'rollback predicates differ from the sanitized baseline; inspection is required'
}

if [[ "$MODE" == reconcile ]]; then
  case "$state" in
    failed-needs-inspection|pending-mutation|pending-mutation-starting|rollback-reboot-verifying|rollback-reboot-verifying-starting|candidate-reboot-verifying|candidate-reboot-verifying-starting) ;;
    *) fail 'reconcile requires a failed or in-progress ledger state' ;;
  esac
  grep -q '^resume_state=' <<<"$state_content" || fail 'reconcile requires an explicit ledger resume state'
  stale_nonce="$(awk -F= '$1 == "attempt_nonce" {print $2}' <<<"$state_content")"
  resume="$(awk -F= '$1 == "resume_state" {print $2}' <<<"$state_content")"
  resume_boot="$(awk -F= '$1 == "boot_id" {print $2}' <<<"$state_content")"
  case "$state" in
    pending-mutation|pending-mutation-starting)
      case "$resume" in
        ''|candidate-green|automatic-rollback-green|rollback-reboot-green) ;;
        *) fail 'pending mutation ledger has an invalid resume state' ;;
      esac
      ;;
    rollback-reboot-verifying|rollback-reboot-verifying-starting)
      [[ "$resume" == rollback-await-reboot ]] \
        || fail 'rollback reboot verification ledger has an invalid resume state'
      ;;
    candidate-reboot-verifying|candidate-reboot-verifying-starting)
      [[ "$resume" == candidate-await-reboot ]] \
        || fail 'candidate reboot verification ledger has an invalid resume state'
      if ! grep -Fx "expected_controller_id=$EXPECTED_CONTROLLER_ID" <<<"$state_content" >/dev/null \
        || ! grep -Fx "production_profile=$PRODUCTION_PROFILE" <<<"$state_content" >/dev/null; then
        fail 'candidate reboot verification ledger lacks the exact controller and profile'
      fi
      ;;
  esac
  if [[ "$stale_nonce" =~ ^[0-9a-f]{64}$ ]]; then
    require_stale_marker=false
    case "$state" in
      pending-mutation|rollback-reboot-verifying|candidate-reboot-verifying) require_stale_marker=true ;;
    esac
    run_remote_control attempt-reconcile-root "$stale_nonce" "$CANDIDATE" "$require_stale_marker"
  elif [[ "$state" != failed-needs-inspection ]]; then
    fail 'in-progress reconcile requires an exact private attempt nonce'
  fi
  if [[ "$state" != failed-needs-inspection ]]; then
    reconcile_active=true
    verification_resume_state="$resume"
    failure_resume_boot_id="$resume_boot"
    attempt_nonce="$stale_nonce"
  fi
  case "$state" in
    candidate-reboot-verifying|candidate-reboot-verifying-starting)
      [[ "$(run_remote_deadlined current-generation)" == "$CANDIDATE" ]] \
        || fail 'candidate reboot reconcile requires the candidate generation to remain active'
      grep -Fx 'expected-controller=yes' <<<"$preflight" >/dev/null \
        || fail 'candidate reboot reconcile requires the exact expected physical controller and production profile'
      attempt_boot_id="$(run_remote_deadlined boot-id)"
      resume_after_failure="$resume"
      failure_resume_boot_id="$resume_boot"
      verification_resume_state="$resume"
      reconcile_active=false
      verification_active=true
      start_attempt candidate-reboot-verifying
      automated_evidence="$(run_remote_attempt automated-gates "$RUNTIME_USER" "$EXPECTED_CONTROLLER_ID" "$PRODUCTION_PROFILE" true)"
      printf '%s\n' "$automated_evidence"
      store_automated_evidence reconcile-candidate-reboot.txt "$automated_evidence"
      verify_accepted_private_digest "$automated_evidence"
      verify_fingerprint_unchanged "$automated_evidence" true
      finish_attempt
      verification_active=false
      write_state "$resume" "$resume_boot"
      ;;
    rollback-reboot-verifying|rollback-reboot-verifying-starting)
      [[ "$(run_remote_deadlined current-generation)" == "$ROLLBACK" ]] \
        || fail 'rollback reboot reconcile requires the rollback generation to be active'
      attempt_boot_id="$(run_remote_deadlined boot-id)"
      resume_after_failure="$resume"
      failure_resume_boot_id="$resume_boot"
      verification_resume_state="$resume"
      reconcile_active=false
      verification_active=true
      start_attempt rollback-reboot-verifying
      run_remote_attempt rollback-gates "$RUNTIME_USER" | tee "$LEDGER/reconcile-rollback-reboot.txt"
      compare_baseline
      finish_attempt
      verification_active=false
      write_state "$resume" "$resume_boot"
      ;;
    pending-mutation|pending-mutation-starting)
      [[ "$(run_remote_deadlined current-generation)" == "$ROLLBACK" ]] \
        || fail 'pending mutation reconcile requires the rollback generation to be active'
      compare_baseline
      write_state "$resume"
      reconcile_active=false
      ;;
    failed-needs-inspection)
      if [[ "$resume" == candidate-await-reboot ]]; then
        [[ "$(run_remote_deadlined current-generation)" == "$CANDIDATE" ]] \
          || fail 'candidate reboot reconcile requires the candidate generation to remain active'
        grep -Fx 'expected-controller=yes' <<<"$preflight" >/dev/null \
          || fail 'candidate reboot reconcile requires the exact expected physical controller and production profile'
        attempt_boot_id="$(run_remote_deadlined boot-id)"
        resume_after_failure="$resume"
        failure_resume_boot_id="$resume_boot"
        verification_resume_state="$resume"
        verification_active=true
        start_attempt candidate-reboot-verifying
        automated_evidence="$(run_remote_attempt automated-gates "$RUNTIME_USER" "$EXPECTED_CONTROLLER_ID" "$PRODUCTION_PROFILE" true)"
        printf '%s\n' "$automated_evidence"
        store_automated_evidence reconcile-candidate-reboot.txt "$automated_evidence"
        verify_accepted_private_digest "$automated_evidence"
        verify_fingerprint_unchanged "$automated_evidence" true
        finish_attempt
        verification_active=false
        write_state candidate-await-reboot "$resume_boot"
      else
        [[ "$(run_remote_deadlined current-generation)" == "$ROLLBACK" ]] \
          || fail 'reconcile requires the rollback generation to be active'
        compare_baseline
        write_state "$resume"
      fi
      ;;
  esac
  printf 'device-gate mode=reconcile state=%s host=%s mutation=none\n' "${resume:-baseline}" "$actual_hostname"
  exit 0
fi
[[ "$state" != failed-needs-inspection && "$state" != pending-mutation ]] || fail 'fresh reconcile is required before retry'

hitl_gates=(normalized-gameplay health-recovery-ambiguity dbus-spoof-and-exclusive-grab exact-stop-and-races direct-action-isolation sunshine-video-controller-recovery catalog-and-session)
require_hitl() {
  local ledger_state="$1" gate expected supplied consumed_key elapsed remaining read_timeout
  local started=$SECONDS
  [[ -r /dev/tty && -w /dev/tty ]] || fail 'HITL stages require an interactive terminal'
  for gate in "${hitl_gates[@]}"; do
    elapsed=$((SECONDS - started))
    remaining=$((HITL_OVERALL_TIMEOUT - elapsed))
    ((remaining > 0)) || fail 'overall HITL stage timeout expired'
    read_timeout="$HITL_READ_TIMEOUT"
    ((read_timeout <= remaining)) || read_timeout="$remaining"
    consumed_key="$attempt_nonce|$attempt_boot_id|$ledger_state|$gate"
    grep -Fqx "$consumed_key" "$LEDGER/consumed-gates" 2>/dev/null && fail "HITL token was already consumed: $gate"
    expected="PASS-$(printf '%s' "$actual_machine_id|$actual_hostname|$CANDIDATE|$attempt_nonce|$attempt_boot_id|$ledger_state|$gate" | sha256sum | cut -c1-16)"
    printf 'HITL stage: %s\nRequired one-time token: %s\n' "$gate" "$expected" >&2
    printf 'Complete the documented stage now, then enter its token: ' >/dev/tty
    IFS= read -r -t "$read_timeout" supplied </dev/tty \
      || fail "HITL stage timed out: $gate"
    [[ "$supplied" == "$expected" ]] || fail "HITL stage is not confirmed: $gate"
    printf '%s\n' "$consumed_key" >>"$LEDGER/consumed-gates"
    chmod 0600 "$LEDGER/consumed-gates"
    sync -f "$LEDGER/consumed-gates"
    if [[ "$gate" == direct-action-isolation ]]; then
      run_remote_attempt compositor-game-gate "$RUNTIME_USER"
    fi
  done
}

begin_mutation() {
  rollback_persistent="$1"
  compare_baseline
  attempt_boot_id="$(run_remote_deadlined boot-id)"
  mutation_active=true
  start_attempt pending-mutation
}
accept_and_disarm() {
  finish_attempt
  mutation_active=false
  write_state "$1" "${2:-}"
}
case "$MODE" in
  candidate-test)
    [[ -z "$state" || "$state" == candidate-green ]] || fail "candidate-test cannot follow ledger state $state"
    begin_mutation false
    run_remote_attempt activate-test "$CANDIDATE" "$RUNTIME_USER"
    automated_evidence="$(run_remote_attempt automated-gates "$RUNTIME_USER" "$EXPECTED_CONTROLLER_ID" "$PRODUCTION_PROFILE" true)"
    printf '%s\n' "$automated_evidence"
    store_automated_evidence candidate-automated.txt "$automated_evidence"
    require_hitl pending-mutation
    run_remote_attempt nvenc-stream-gate
    verify_fingerprint_unchanged "$automated_evidence" true
    save_candidate_controller_proof "$automated_evidence"
    run_remote_attempt restore "$ROLLBACK" false "$RUNTIME_USER" \
      "$old_korrid_active" "$old_korrid_enabled" "$old_sunshine_active" "$old_sunshine_enabled" \
      "$old_x11_active" "$old_x11_enabled" "$old_pairing_config_mode" "$old_pairing_state_mode"
    compare_baseline
    accept_and_disarm candidate-green "$attempt_boot_id"
    ;;
  inject-health-failure)
    [[ "$state" == candidate-green ]] || fail 'injected failure requires candidate-green ledger state'
    begin_mutation false
    run_remote_attempt activate-test "$CANDIDATE" "$RUNTIME_USER"
    run_remote_attempt inject-health-failure "$ROLLBACK" false "$RUNTIME_USER" \
      "$old_korrid_active" "$old_korrid_enabled" "$old_sunshine_active" "$old_sunshine_enabled" \
      "$old_x11_active" "$old_x11_enabled" "$old_pairing_config_mode" "$old_pairing_state_mode"
    compare_baseline
    accept_and_disarm automatic-rollback-green "$attempt_boot_id"
    ;;
  rollback)
    [[ "$state" == automatic-rollback-green ]] || fail 'explicit rollback requires automatic-rollback-green ledger state'
    begin_mutation true
    run_remote_attempt activate-test "$CANDIDATE" "$RUNTIME_USER"
    run_remote_attempt restore "$ROLLBACK" true "$RUNTIME_USER" \
      "$old_korrid_active" "$old_korrid_enabled" "$old_sunshine_active" "$old_sunshine_enabled" \
      "$old_x11_active" "$old_x11_enabled" "$old_pairing_config_mode" "$old_pairing_state_mode"
    compare_baseline
    accept_and_disarm rollback-await-reboot "$attempt_boot_id"
    ;;
  rollback-reboot-verify)
    [[ "$state" == rollback-await-reboot ]] || fail 'rollback reboot verification requires rollback-await-reboot ledger state'
    prior_boot="$(awk -F= '$1 == "boot_id" {print $2}' <<<"$state_content")"
    current_boot="$(run_remote_deadlined boot-id)"
    [[ "$current_boot" != "$prior_boot" ]] || fail 'rollback reboot verification requires a new boot ID'
    [[ "$(run_remote_deadlined current-generation)" == "$ROLLBACK" ]] || fail 'rebooted system is not the rollback generation'
    attempt_boot_id="$current_boot"
    resume_after_failure='rollback-await-reboot'
    failure_resume_boot_id="$prior_boot"
    verification_resume_state='rollback-await-reboot'
    verification_active=true
    start_attempt rollback-reboot-verifying
    run_remote_attempt rollback-gates "$RUNTIME_USER" | tee "$LEDGER/rollback-reboot.txt"
    compare_baseline
    finish_attempt
    verification_active=false
    write_state rollback-reboot-green "$current_boot"
    ;;
  persistent-switch)
    candidate_controller_proof="$(read_ledger_proof candidate-controller.accepted 2>/dev/null)" \
      || fail 'persistent switch requires safe prior candidate controller proof'
    [[ -n "$candidate_controller_proof" ]] \
      || fail 'persistent switch requires prior candidate proof with the exact production controller profile'
    if [[ "$state" == candidate-accepted-pending-boot ]]; then
      current_boot="$(run_remote_deadlined boot-id)"
      write_state candidate-await-reboot "$current_boot"
    else
      [[ "$state" == rollback-reboot-green ]] || fail 'persistent switch requires rollback-reboot-green ledger state'
      begin_mutation true
      run_remote_attempt persistent-switch "$CANDIDATE" "$RUNTIME_USER"
      automated_evidence="$(run_remote_attempt automated-gates "$RUNTIME_USER" "$EXPECTED_CONTROLLER_ID" "$PRODUCTION_PROFILE" true)"
      printf '%s\n' "$automated_evidence"
      store_automated_evidence persistent-automated.txt "$automated_evidence"
      require_hitl pending-mutation
      run_remote_attempt nvenc-stream-gate
      verify_fingerprint_unchanged "$automated_evidence" true
      save_accepted_private_digest "$automated_evidence"
      # This durable accepted state makes a failed boot-ID fetch resumable.
      finish_attempt
      mutation_active=false
      write_state candidate-accepted-pending-boot '' '' ''
      current_boot="$(run_remote_deadlined boot-id)"
      write_state candidate-await-reboot "$current_boot"
    fi
    ;;
  candidate-reboot-verify)
    [[ "$state" == candidate-await-reboot ]] || fail 'candidate reboot verification requires candidate-await-reboot ledger state'
    verification_active=true
    current_boot="$(run_remote_deadlined boot-id)"
    [[ "$current_boot" != "$prior_boot" ]] || fail 'candidate reboot verification requires a new boot ID'
    [[ "$(run_remote_deadlined current-generation)" == "$CANDIDATE" ]] || fail 'rebooted system is not the candidate generation'
    attempt_boot_id="$current_boot"
    start_attempt candidate-reboot-verifying
    automated_evidence="$(run_remote_attempt automated-gates "$RUNTIME_USER" "$EXPECTED_CONTROLLER_ID" "$PRODUCTION_PROFILE" true)"
    printf '%s\n' "$automated_evidence"
    store_automated_evidence candidate-reboot.txt "$automated_evidence"
    verify_accepted_private_digest "$automated_evidence"
    require_hitl candidate-reboot-verifying
    run_remote_attempt nvenc-stream-gate
    verify_fingerprint_unchanged "$automated_evidence" true
    finish_attempt
    verification_active=false
    write_state complete "$current_boot"
    ;;
esac

printf 'device-gate mode=%s state=%s host=%s mutation=confirmed\n' "$MODE" "$(awk -F= '$1 == "state" {print $2}' <<<"$state_content")" "$actual_hostname"
