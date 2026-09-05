#!/usr/bin/env bash
set -Eeuo pipefail

# This file is also the modeled SSH endpoint used by the tests below.
case "$(basename "$0")" in
  id)
    if [[ "${1:-}" == -u && -n "${2:-}" ]]; then
      [[ "$2" == "${HARNESS_RUNTIME_USER:-korri}" ]] || exit 1
      printf '%s\n' "${HARNESS_RUNTIME_UID:-1000}"
      exit 0
    fi
    [[ "${1:-}" == -u ]] && printf '%s\n' "${HARNESS_CALLER_UID:-0}" && exit 0
    exit 1
    ;;
  sudo)
    [[ "${1:-}" == -n ]] || exit 1
    shift
    [[ "${1:-}" == -u && "${2:-}" == "${HARNESS_RUNTIME_USER:-korri}" ]] || exit 1
    [[ "${HARNESS_SUDO_DENY:-no}" != yes ]] || exit 1
    printf 'sudo-user=%s caller=%s argv=' "$2" "${HARNESS_CALLER_UID:-0}" >>"${HARNESS_USER_SCOPE_LOG:?}"
    printf '%q ' "$@" >>"$HARNESS_USER_SCOPE_LOG"
    printf '\n' >>"$HARNESS_USER_SCOPE_LOG"
    shift 2
    export HARNESS_MODELED_USER=korri
    exec "$@"
    ;;
  getent|user-scope-getent)
    case "${1:-}" in
      passwd) printf 'korri:x:1000:1000::%s:/bin/sh\n' "${HARNESS_RUNTIME_HOME:?}" ;;
      group) printf '%s:x:%s:\n' "${2:-korri}" "${2:-1000}" ;;
      *) exit 1 ;;
    esac
    exit 0
    ;;
  systemctl)
    scope=system unit='' values_only=false
    properties=()
    for arg in "$@"; do
      [[ "$arg" != --user ]] || scope=user
      [[ "$arg" != *.service ]] || unit="$arg"
      [[ "$arg" != --value ]] || values_only=true
    done
    while [[ $# -gt 0 ]]; do
      if [[ "$1" == -p ]]; then
        properties+=("${2:-}")
        shift 2
        continue
      fi
      shift
    done
    property="$(IFS=,; printf '%s' "${properties[*]}")"
    if [[ "$scope" == user ]]; then
      [[ "${HARNESS_MODELED_USER:-root}" == korri ]] || exit 1
      [[ "${XDG_RUNTIME_DIR:-}" == "/run/user/${HARNESS_RUNTIME_UID:-1000}" ]] || exit 1
      [[ "${DBUS_SESSION_BUS_ADDRESS:-}" == "unix:path=/run/user/${HARNESS_RUNTIME_UID:-1000}/bus" ]] || exit 1
      printf 'systemctl-user=%s property=%s runtime=%s bus=%s\n' \
        "$unit" "$property" "$XDG_RUNTIME_DIR" "$DBUS_SESSION_BUS_ADDRESS" >>"${HARNESS_USER_SCOPE_LOG:?}"
      [[ "${HARNESS_USER_QUERY_ERROR:-}" != all ]] || exit 69
      for queried_property in "${properties[@]}"; do
        [[ "${HARNESS_USER_QUERY_ERROR:-}" != "$queried_property:$unit" ]] || exit 69
      done
      active="${HARNESS_USER_ACTIVE_STATE:-active}"
      enabled="${HARNESS_USER_ENABLED_STATE-enabled}"
      load="${HARNESS_USER_LOAD_STATE-loaded}"
    else
      active=active enabled=enabled load=loaded
    fi
    for queried_property in "${properties[@]}"; do
      case "$queried_property" in
        ActiveState) value="$active" ;;
        UnitFileState) value="$enabled" ;;
        LoadState) value="$load" ;;
        SubState) value=running ;;
        StatusText) value='' ;;
        *) value='' ;;
      esac
      if [[ "$values_only" == true ]]; then
        printf '%s\n' "$value"
      else
        printf '%s=%s\n' "$queried_property" "$value"
      fi
    done
    exit 0
    ;;
  ssh-command-harness)
    command="${*: -1}"
    printf 'ssh-command=%s\n' "$command" >>"$HARNESS_LOG"
    # Model the remote login shell. If production quoting regresses, this eval
    # makes the adversarial tests execute their sentinel and fail.
    eval "set -- $command"
    transport_deadlined=false
    if [[ "${1:-}" == timeout ]]; then
      transport_deadlined=true
      shift
      while [[ "${1:-}" == -* ]]; do shift; done
      [[ "${1:-}" =~ ^[0-9]+s$ ]] || exit 72
      shift
    fi
    if [[ "$transport_deadlined" != true ]]; then
      if [[ "${2:-}" != --remote || ! "${3:-}" =~ ^(deadline|locked-root|attempt-command)$ ]]; then
        printf 'bare SSH operation without a remote deadline\n' >&2
        exit 73
      fi
    fi
    if [[ "${1:-}" == stat && "${*: -1}" == */sw/bin/korri-device-gate ]]; then
      printf '%s\n' "${HARNESS_HELPER_STAT:-0:555}"
      exit 0
    fi
    if [[ "${1:-}" == sha256sum && "${*: -1}" == */sw/bin/korri-device-gate ]]; then
      digest="$(sha256sum "${HARNESS_GATE_SOURCE:?}" | awk '{print $1}')"
      [[ "${HARNESS_HELPER_DIGEST:-valid}" == valid ]] || digest=ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
      printf '%s  %s\n' "$digest" "${*: -1}"
      exit 0
    fi
    if [[ "${1:-}" == cat && "${2:-}" == /etc/machine-id ]]; then
      printf '%s\n' "${HARNESS_MACHINE_ID:-0123456789abcdef0123456789abcdef}"
      exit 0
    fi
    if [[ "${1:-}" == hostname ]]; then
      printf '%s\n' "${HARNESS_HOSTNAME:-u7-test-host}"
      exit 0
    fi
    if [[ "${1:-}" == mktemp ]]; then
      printf '/tmp/korri-device-gate.harness1\n'
      exit 0
    fi
    if [[ "${1:-}" == rm ]]; then
      exit 0
    fi
    if [[ "${2:-}" == --remote ]]; then
      helper_path="$1"
      action="${3:-}"
      shift 3
      wrapper=direct lock_owned=false
      if [[ "$action" == deadline ]]; then
        wrapper=deadline
        remote_deadline="$1"
        action="$2"
        shift 2
      elif [[ "$action" == locked-root ]]; then
        wrapper=locked-root
        remote_deadline="$1"
        lock_wait="$2"
        action="$3"
        shift 3
      elif [[ "$action" == attempt-command ]]; then
        wrapper=attempt-command
        remote_deadline="$1"
        lock_wait="$2"
        attempt_nonce="$3"
        attempt_candidate="$4"
        action="$5"
        shift 5
      fi
      if [[ "$wrapper" == locked-root || "$wrapper" == attempt-command ]]; then
        lock_dir="${HARNESS_GATE_LOCK:-$HARNESS_LOG.lock}"
        for _ in $(seq 1 500); do
          if mkdir "$lock_dir" 2>/dev/null; then
            lock_owned=true
            break
          fi
          printf 'lock-wait action=%s\n' "$action" >>"$HARNESS_LOG"
          sleep 0.01
        done
        [[ "$lock_owned" == true ]] || {
          printf 'modeled root gate lock timeout for %s\n' "$action" >&2
          exit 75
        }
        trap '[[ "$lock_owned" != true ]] || rmdir "$lock_dir" 2>/dev/null || true' EXIT
      fi
      if [[ "$wrapper" == attempt-command ]]; then
        grep -Fx "nonce=$attempt_nonce" "$HARNESS_ATTEMPT_MARKER" >/dev/null 2>&1 \
          && grep -Fx "candidate=$attempt_candidate" "$HARNESS_ATTEMPT_MARKER" >/dev/null 2>&1 \
          && [[ -e "$HARNESS_ATTEMPT_LEASE" ]] || {
            printf 'modeled attempt marker validation failed\n' >&2
            exit 76
          }
      fi
      case "$action" in
        acceptance-fingerprint|automated-gates|rollback-gates|activate-test|inject-health-failure|restore|persistent-switch)
          if [[ "$wrapper" != attempt-command \
            || "$helper_path" != "$attempt_candidate/sw/bin/korri-device-gate" ]]; then
            printf 'root mutation or gate argv did not name the candidate store helper\n' >&2
            exit 79
          fi
          ;;
        predicates|boot-id|current-generation)
          if [[ "$wrapper" == attempt-command \
            && "$helper_path" != "$attempt_candidate/sw/bin/korri-device-gate" ]]; then
            printf 'root attempt argv did not name the candidate store helper\n' >&2
            exit 79
          fi
          ;;
      esac
      {
        printf 'helper=%q wrapper=%s remote-deadline=%s lock-wait=%s action=%s argv=' \
          "$helper_path" "$wrapper" "${remote_deadline:-none}" "${lock_wait:-none}" "$action"
        printf '%q ' "$@"
        printf '\n'
      } >>"$HARNESS_LOG"
      case "$action" in
        attempt-start-root)
          nonce="$1" candidate="$2"
          if [[ -e "$HARNESS_ATTEMPT_MARKER" || -e "$HARNESS_ATTEMPT_LEASE" ]]; then
            printf 'device gate: another device-gate attempt marker already exists\n' >&2
            exit 77
          fi
          if [[ "${HARNESS_ATTEMPT_START_PAUSE:-}" == pre-marker ]]; then
            : >"${HARNESS_PAUSE_MARKER:?}"
            sleep "${HARNESS_PAUSE_SECONDS:-30}"
          fi
          printf 'nonce=%s\ncandidate=%s\n' "$nonce" "$candidate" >"$HARNESS_ATTEMPT_MARKER"
          : >"$HARNESS_ATTEMPT_LEASE"
          if [[ "${HARNESS_ATTEMPT_START_PAUSE:-}" == post-marker ]]; then
            : >"${HARNESS_PAUSE_MARKER:?}"
            sleep "${HARNESS_PAUSE_SECONDS:-30}"
          fi
          exit 0
          ;;
        attempt-finish-root)
          grep -Fx "nonce=$1" "$HARNESS_ATTEMPT_MARKER" >/dev/null \
            && grep -Fx "candidate=$2" "$HARNESS_ATTEMPT_MARKER" >/dev/null || exit 76
          rm -f "$HARNESS_ATTEMPT_LEASE" "$HARNESS_ATTEMPT_MARKER"
          exit 0
          ;;
        attempt-reconcile-root)
          if [[ ! -e "$HARNESS_ATTEMPT_MARKER" ]]; then
            [[ "${3:-false}" == false ]] || {
              printf 'device gate: in-progress device-gate marker is missing\n' >&2
              exit 76
            }
            exit 0
          fi
          if ! grep -Fx "nonce=$1" "$HARNESS_ATTEMPT_MARKER" >/dev/null \
            || ! grep -Fx "candidate=$2" "$HARNESS_ATTEMPT_MARKER" >/dev/null; then
            printf 'device gate: stale device-gate marker does not match this private attempt\n' >&2
            exit 76
          fi
          [[ ! -e "$HARNESS_ATTEMPT_LEASE" ]] || {
            printf 'device gate: device-gate attempt is still live; reconcile refuses to race it\n' >&2
            exit 78
          }
          rm -f "$HARNESS_ATTEMPT_MARKER"
          exit 0
          ;;
      esac
      if [[ "${HARNESS_PAUSE_ACTION:-}" == "$action" ]]; then
        : >"${HARNESS_PAUSE_MARKER:?}"
        sleep "${HARNESS_PAUSE_SECONDS:-30}"
      fi
      if [[ "${HARNESS_FAIL_ACTION_ONCE:-}" == "$action" ]]; then
        fail_marker="${HARNESS_FAIL_ACTION_MARKER:?}"
        if [[ ! -e "$fail_marker" ]]; then
          : >"$fail_marker"
          printf 'modeled transient SSH failure for %s\n' "$action" >&2
          exit 255
        fi
      fi
      case "$action" in
        inspect)
          [[ "${1:-}" == "${HARNESS_RUNTIME_USER:-korri}" ]] || exit 87
          printf 'identity machine-id=%s hostname=%s\n' \
            "${HARNESS_MACHINE_ID:-0123456789abcdef0123456789abcdef}" \
            "${HARNESS_HOSTNAME:-u7-test-host}"
          printf 'generation current=%s default=%s\n' "$ROLLBACK" "$ROLLBACK"
          printf 'units:\nsystem/inputplumber.service LoadState=loaded ActiveState=active SubState=running UnitFileState=enabled StatusText=\n'
          printf 'user/korrid.service LoadState=loaded ActiveState=inactive SubState=dead UnitFileState=disabled StatusText=\n'
          old_sunshine_active="${HARNESS_OLD_SUNSHINE_ACTIVE:-true}"
          old_sunshine_enabled="${HARNESS_OLD_SUNSHINE_ENABLED:-true}"
          old_x11_active="${HARNESS_OLD_X11_ACTIVE:-true}"
          old_x11_enabled="${HARNESS_OLD_X11_ENABLED:-true}"
          [[ "$old_sunshine_active" != true ]] || old_sunshine_active=active
          [[ "$old_sunshine_active" != false ]] || old_sunshine_active=inactive
          [[ "$old_sunshine_enabled" != true ]] || old_sunshine_enabled=enabled
          [[ "$old_sunshine_enabled" != false ]] || old_sunshine_enabled=disabled
          [[ "$old_x11_active" != true ]] || old_x11_active=active
          [[ "$old_x11_active" != false ]] || old_x11_active=inactive
          [[ "$old_x11_enabled" != true ]] || old_x11_enabled=enabled
          [[ "$old_x11_enabled" != false ]] || old_x11_enabled=disabled
          printf 'system/sunshine.service LoadState=loaded ActiveState=%s SubState=dead UnitFileState=%s StatusText=\n' \
            "${HARNESS_SYSTEM_SUNSHINE_ACTIVE:-inactive}" "${HARNESS_SYSTEM_SUNSHINE_ENABLED:-disabled}"
          printf 'system/x11-headless.service LoadState=loaded ActiveState=%s SubState=dead UnitFileState=%s StatusText=\n' \
            "${HARNESS_SYSTEM_X11_ACTIVE:-inactive}" "${HARNESS_SYSTEM_X11_ENABLED:-disabled}"
          printf 'system/korri-compositor.service LoadState=loaded ActiveState=%s SubState=dead UnitFileState=%s StatusText=\n' \
            "${HARNESS_SYSTEM_COMPOSITOR_ACTIVE:-inactive}" "${HARNESS_SYSTEM_COMPOSITOR_ENABLED:-disabled}"
          printf 'user/sunshine.service LoadState=loaded ActiveState=%s SubState=running UnitFileState=%s StatusText=\n' \
            "$old_sunshine_active" "$old_sunshine_enabled"
          printf 'user/x11-headless.service LoadState=loaded ActiveState=%s SubState=running UnitFileState=%s StatusText=\n' \
            "$old_x11_active" "$old_x11_enabled"
          printf 'temporary-artifacts-dirty=%s catalog=Ok\n' "${HARNESS_DIRTY:-no}"
          printf 'physical-controller-candidates:\n'
          printf 'controller-candidate identity=%s name=Observed_Controller sysfs=/sys/devices/pci0000:00/input/input8/event8 event=event8\n' \
            "${HARNESS_CONTROLLER_ID:-0003:045e:02ea:050b}"
          ;;
        predicates)
          if [[ "${1:-}" == "${HARNESS_RUNTIME_USER:-korri}" ]]; then
            printf 'predicates-user=korri wrapper=%s\n' "$wrapper" >>"$HARNESS_LOG"
          else
            printf 'predicates-user=root wrapper=%s\n' "$wrapper" >>"$HARNESS_LOG"
          fi
          printf 'generation.current=%s\n' "${HARNESS_PREDICATE_GENERATION:-$ROLLBACK}"
          printf 'generation.default=%s\n' "${HARNESS_PREDICATE_DEFAULT:-$ROLLBACK}"
          printf 'old-user.korrid.active=%s\n' "${HARNESS_OLD_KORRID_ACTIVE:-${HARNESS_OLD_ACTIVE:-false}}"
          printf 'old-user.korrid.enabled=%s\n' "${HARNESS_OLD_KORRID_ENABLED:-${HARNESS_OLD_ENABLED:-false}}"
          printf 'old-user.sunshine.active=%s\n' "${HARNESS_OLD_SUNSHINE_ACTIVE:-true}"
          printf 'old-user.sunshine.enabled=%s\n' "${HARNESS_OLD_SUNSHINE_ENABLED:-true}"
          printf 'old-user.x11-headless.active=%s\n' "${HARNESS_OLD_X11_ACTIVE:-true}"
          printf 'old-user.x11-headless.enabled=%s\n' "${HARNESS_OLD_X11_ENABLED:-true}"
          printf 'system.korrid.active=%s\n' "${HARNESS_SYSTEM_KORRID_ACTIVE:-inactive}"
          printf 'system.korrid.enabled=%s\n' "${HARNESS_SYSTEM_KORRID_ENABLED:-disabled}"
          printf 'system.sunshine.active=%s\n' "${HARNESS_SYSTEM_SUNSHINE_ACTIVE:-inactive}"
          printf 'system.sunshine.enabled=%s\n' "${HARNESS_SYSTEM_SUNSHINE_ENABLED:-disabled}"
          printf 'system.x11-headless.active=%s\n' "${HARNESS_SYSTEM_X11_ACTIVE:-inactive}"
          printf 'system.x11-headless.enabled=%s\n' "${HARNESS_SYSTEM_X11_ENABLED:-disabled}"
          printf 'system.korri-compositor.active=%s\n' "${HARNESS_SYSTEM_COMPOSITOR_ACTIVE:-inactive}"
          printf 'system.korri-compositor.enabled=%s\n' "${HARNESS_SYSTEM_COMPOSITOR_ENABLED:-disabled}"
          printf 'topology.target=%s\n' "${HARNESS_TARGET_TOPOLOGY:-target-baseline}"
          printf 'topology.raw=%s\n' "${HARNESS_RAW_TOPOLOGY:-raw-baseline}"
          printf 'input.acl-readability=%s\n' "${HARNESS_ACL_BASELINE:-acl-baseline}"
          printf 'input.sources-artifacts=%s\n' "${HARNESS_ARTIFACTS_BASELINE:-artifacts-clean}"
          printf 'inputplumber.active=%s\n' "${HARNESS_IP_ACTIVE:-active}"
          printf 'inputplumber.enabled=%s\n' "${HARNESS_IP_ENABLED:-enabled}"
          printf 'sunshine.pairing-state-modes=%s\n' "${HARNESS_PAIRING_MODES:-700:600}"
          printf 'sunshine.pairing-state-present=%s\n' "${HARNESS_PAIRING_PRESENT:-true}"
          if [[ "${HARNESS_LEDGER:-}" == *baseline-private-* ]]; then
            private_digest=invalid
          else
            private_digest="${HARNESS_PRIVATE_STATE_DIGEST:-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}"
          fi
          printf 'sunshine.private-state-digest=%s\n' "$private_digest"
          printf 'catalog.health=%s\n' "${HARNESS_CATALOG:-Ok}"
          ;;
        preflight)
          candidate="$1"
          rollback="$2"
          printf 'candidate=%s\n' "$candidate"
          printf 'candidate-switch=%s\n' "${HARNESS_CANDIDATE_SWITCH:-yes}"
          printf 'rollback=%s\n' "$rollback"
          printf 'rollback-switch=%s\n' "${HARNESS_ROLLBACK_SWITCH:-yes}"
          expected_identity="${3:-}"
          profile="${4:-}"
          printf 'temporary-artifacts-dirty=%s\n' "${HARNESS_DIRTY:-no}"
          if [[ -n "$expected_identity" && -n "$profile" ]]; then
            if [[ "${HARNESS_CONTROLLER_MODEL:-valid}" == valid \
              && "$expected_identity" == "${HARNESS_CONTROLLER_ID:-0003:045e:02ea:050b}" \
              && "$profile" == korri-60-xbox_one_gamepad.yaml ]]; then
              printf 'expected-controller=yes\n'
              printf 'controller-evidence=identity=%s event=event8 sysfs=/sys/devices/pci0000:00/input/input8/event8 profile=%s\n' "$expected_identity" "$profile"
            else
              printf 'expected-controller=no\n'
            fi
          fi
          ;;
        boot-id)
          if [[ -n "${HARNESS_BOOT_COUNT_FILE:-}" ]]; then
            count="$(cat "$HARNESS_BOOT_COUNT_FILE" 2>/dev/null || printf 0)"
            count=$((count + 1))
            printf '%s\n' "$count" >"$HARNESS_BOOT_COUNT_FILE"
            [[ "$count" != "${HARNESS_FAIL_BOOT_ID_AT:-0}" ]] || exit 71
          elif [[ "${HARNESS_FAIL_BOOT_ID:-no}" == yes ]]; then
            exit 71
          fi
          printf '%s' "${HARNESS_BOOT_ID:-boot-one}"
          ;;
        current-generation)
          printf '%s\n' "${HARNESS_CURRENT_GENERATION:-$ROLLBACK}"
          ;;
        automated-gates)
          expected_identity="${2:-}"
          profile="${3:-}"
          require_physical="${4:-false}"
          if [[ "${HARNESS_AUTOMATED_INTERRUPT:-no}" == yes ]]; then
            printf 'modeled interruption immediately after activation\n' >&2
            exit 130
          fi
          [[ "${HARNESS_STALE_MANAGER_CREDENTIALS:-no}" != yes ]] || {
            printf 'device gate: fresh runtime user manager retains forbidden group: input\n' >&2
            exit 80
          }
          [[ -z "${HARNESS_CREDENTIAL_FAILURE:-}" ]] || {
            printf 'device gate: modeled /proc Groups credential rejection: %s\n' "$HARNESS_CREDENTIAL_FAILURE" >&2
            exit 85
          }
          case "${HARNESS_SUNSHINE_PACKAGE:-valid}" in
            valid) ;;
            stock)
              printf 'device gate: candidate Sunshine package is not sunshine-korri\n' >&2
              exit 87
              ;;
            wrong-executable)
              printf 'device gate: running Sunshine executable differs from the candidate unit\n' >&2
              exit 87
              ;;
            bad-provenance)
              printf 'device gate: sunshine-korri provenance does not match the approved patch-set digest\n' >&2
              exit 87
              ;;
            duplicate-patch|reordered-patch|wrong-patch-hash)
              printf 'device gate: sunshine-korri provenance does not match the approved ordered patch manifest\n' >&2
              exit 87
              ;;
            wrong-base)
              printf 'device gate: sunshine-korri provenance has an invalid base_sunshine_version field\n' >&2
              exit 87
              ;;
            wrong-libavcodec)
              printf 'device gate: sunshine-korri provenance has an invalid reviewed_libavcodec_version field\n' >&2
              exit 87
              ;;
            wrong-ffmpeg-commit)
              printf 'device gate: sunshine-korri provenance has an invalid reviewed_ffmpeg_commit field\n' >&2
              exit 87
              ;;
            wrong-ffmpeg-source)
              printf 'device gate: sunshine-korri provenance has an invalid reviewed_ffmpeg_source_hash field\n' >&2
              exit 87
              ;;
            wrong-nvenc-api)
              printf 'device gate: sunshine-korri provenance has an invalid reviewed_nvenc_api field\n' >&2
              exit 87
              ;;
            *) exit 87 ;;
          esac
          [[ "${HARNESS_PAIRING_PRESENT:-true}" == true ]] || {
            printf 'device gate: Sunshine pairing-state file is absent\n' >&2
            exit 81
          }
          [[ "${HARNESS_PRIVATE_STATE_MODEL:-valid}" == valid ]] || {
            printf 'device gate: Sunshine private configuration tree is unsafe or incomplete\n' >&2
            exit 88
          }
          [[ "${HARNESS_TOPOLOGY_MODEL:-one}" == one ]] || {
            printf 'modeled topology has %s normalized targets\n' "$HARNESS_TOPOLOGY_MODEL" >&2
            exit 61
          }
          [[ "${HARNESS_PROVENANCE_MODEL:-valid}" == valid ]] || {
            printf 'modeled target provenance/capability fingerprint is invalid\n' >&2
            exit 62
          }
          [[ "${HARNESS_ACL_MODEL:-normalized-only}" == normalized-only ]] || {
            printf 'modeled runtime-user ACL exposes raw input\n' >&2
            exit 63
          }
          if [[ -n "${HARNESS_TOPOLOGY_FIXTURE:-}" ]]; then
            normalized_node=''
            readable_raw=0
            while IFS='|' read -r fixture_node fixture_name fixture_provenance fixture_joystick fixture_readable; do
              [[ -n "$fixture_node" && "$fixture_node" != \#* ]] || continue
              [[ "$fixture_name" == 'Microsoft X-Box 360 pad' ]] || exit 62
              if [[ "$fixture_provenance" == normalized-inputplumber ]]; then
                [[ -z "$normalized_node" ]] || exit 61
                normalized_node="$fixture_node"
                continue
              fi
              if [[ "$fixture_joystick" == yes && "$fixture_readable" == yes ]]; then
                readable_raw=$((readable_raw + 1))
              fi
            done <"$HARNESS_TOPOLOGY_FIXTURE"
            [[ "$normalized_node" == /dev/input/event9 ]] || exit 62
            [[ "$readable_raw" -eq 0 ]] || {
              printf 'device gate: runtime user can read %s raw controller node(s)\n' "$readable_raw" >&2
              exit 63
            }
          fi
          [[ "${HARNESS_DELEGATE:-yes}" == yes ]] || {
            printf 'device gate: inputd Delegate is not enabled\n' >&2
            exit 66
          }
          [[ " ${HARNESS_DELEGATE_CONTROLLERS:-cpu pids memory} " == *' pids '* ]] || {
            printf 'device gate: inputd DelegateControllers does not contain pids\n' >&2
            exit 67
          }
          if [[ "$require_physical" == true ]]; then
            [[ "${HARNESS_CONTROLLER_MODEL:-valid}" == valid \
              && "$expected_identity" == "${HARNESS_CONTROLLER_ID:-0003:045e:02ea:050b}" \
              && "$profile" == korri-60-xbox_one_gamepad.yaml ]] || {
              printf 'expected physical controller is not live, supported, and selected with the production profile\n' >&2
              exit 68
            }
          fi
          [[ "${HARNESS_CATALOG:-Ok}" == Ok ]] || exit 64
          normalized="${HARNESS_FINGERPRINT:-node=/dev/input/event9 sysfs=/sys/devices/virtual/input/input9/event9 dev=13:73 inode=1:9 inputplumber=/nix/store/provider/bin/inputplumber version=0.75.2 keys=exact abs=exact ff=yes}"
          physical="identity=$expected_identity event=event8 sysfs=/sys/devices/pci0000:00/input/input8/event8 profile=$profile"
          printf 'compositor-gate=pass renderer=gles2 output=HEADLESS-1 mode=1920x1080@60 performance=not-required wayland=stable xwayland=:0 sunshine-control=denied\n'
          printf 'automated-gates=pass raw-readable=0 inputd-status=Ready system-korrid=active system-korri-compositor=active system-sunshine=active pairing-state=present credentials=service-specific sunshine-package=attested catalog=Ok delegate=yes controllers=pids\n'
          printf 'sunshine-executable=/nix/store/sunshine-korri/bin/sunshine-2025.924.154138-korri patch-set-sha256=%064d patches=15 base-version=2025.924.154138 libavcodec=62.11.100\n' 0
          printf 'sunshine-private-state=protected digest=%s\n' "${HARNESS_PRIVATE_STATE_DIGEST:-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}"
          printf 'normalized-fingerprint=%s\n' "$normalized"
          [[ "$require_physical" != true ]] || printf 'controller-evidence=%s\n' "$physical"
          sunshine='sunshine-executable=/nix/store/sunshine-korri/bin/sunshine-2025.924.154138-korri patch-set-sha256=0000000000000000000000000000000000000000000000000000000000000000 patches=15 base-version=2025.924.154138 libavcodec=62.11.100'
          printf 'acceptance-fingerprint=normalized=%s sunshine=%q private-state=%s' "$normalized" "$sunshine" "${HARNESS_PRIVATE_STATE_DIGEST:-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}"
          [[ "$require_physical" != true ]] || printf ' physical=%s' "$physical"
          printf '\n'
          ;;
        acceptance-fingerprint)
          [[ "${1:-}" == "${HARNESS_RUNTIME_USER:-korri}" ]] || exit 87
          expected_identity="${2:-}"
          profile="${3:-}"
          require_physical="${4:-false}"
          if [[ "${HARNESS_REPLACE_TARGET:-no}" == yes ]]; then
            normalized='node=/dev/input/event10 sysfs=/sys/devices/virtual/input/input10/event10 dev=13:74 inode=1:10 inputplumber=/nix/store/provider/bin/inputplumber version=0.75.2 keys=exact abs=exact ff=yes'
          else
            normalized="${HARNESS_FINGERPRINT:-node=/dev/input/event9 sysfs=/sys/devices/virtual/input/input9/event9 dev=13:73 inode=1:9 inputplumber=/nix/store/provider/bin/inputplumber version=0.75.2 keys=exact abs=exact ff=yes}"
          fi
          if [[ "${HARNESS_REPLACE_SUNSHINE:-no}" == yes ]]; then
            sunshine='sunshine-executable=/nix/store/replaced-sunshine/bin/sunshine patch-set-sha256=ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff patches=15 base-version=2025.924.154138 libavcodec=62.11.100'
          else
            sunshine='sunshine-executable=/nix/store/sunshine-korri/bin/sunshine-2025.924.154138-korri patch-set-sha256=0000000000000000000000000000000000000000000000000000000000000000 patches=15 base-version=2025.924.154138 libavcodec=62.11.100'
          fi
          if [[ "${HARNESS_REPLACE_PRIVATE_STATE:-no}" == yes ]]; then
            private_state=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
          else
            private_state="${HARNESS_PRIVATE_STATE_DIGEST:-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa}"
          fi
          printf 'normalized=%s sunshine=%q private-state=%s' "$normalized" "$sunshine" "$private_state"
          if [[ "$require_physical" == true ]]; then
            [[ "${HARNESS_CONTROLLER_MODEL:-valid}" == valid ]] || exit 68
            printf ' physical=identity=%s event=event8 sysfs=/sys/devices/pci0000:00/input/input8/event8 profile=%s' "$expected_identity" "$profile"
          fi
          printf '\n'
          ;;
        compositor-game-gate)
          printf 'game-compositor-gate=pass xwayland=visible wayland=hidden control=hidden procfs=isolated unit=one\n'
          ;;
        nvenc-stream-gate)
          printf 'nvenc-stream-gate=pass encoder=h264_nvenc strict=yes capture=wayland\n'
          ;;
        rollback-gates)
          printf 'rollback-gates=pass\n'
          ;;
        activate-test|inject-health-failure|restore|persistent-switch)
          grep -Fx 'state=pending-mutation' "$HARNESS_LEDGER/state" >/dev/null || {
            printf 'mutation was not armed in the ledger: %s\n' "$action" >&2
            exit 65
          }
          case "${HARNESS_ACTIVE_GAME:-none}" in
            exact-running)
              printf 'device gate: exact local game status is running; refusing service mutation\n' >&2
              exit 82
              ;;
            exact-stopping)
              printf 'device gate: exact local game status is stopping; refusing service mutation\n' >&2
              exit 82
              ;;
            unit-live)
              printf 'device gate: a Korri game unit is live; refusing service mutation\n' >&2
              exit 82
              ;;
          esac
          if [[ "$action" == activate-test || "$action" == persistent-switch ]]; then
            [[ "${2:-}" == "${HARNESS_RUNTIME_USER:-korri}" ]]
            [[ "${HARNESS_POST_STOP_QUERY_ERROR:-no}" != yes ]] || {
              printf 'device gate: old user unit active state query failed after stop: korrid.service\n' >&2
              exit 86
            }
            printf 'candidate-service-mutation=started\n' >>"$HARNESS_LOG"
            if [[ -n "${HARNESS_CANDIDATE_SERVICE_FAILURE:-}" ]]; then
              printf 'device gate: timed out waiting for %s ActiveState=active\n' "$HARNESS_CANDIDATE_SERVICE_FAILURE" >&2
              exit 83
            fi
            printf 'user-manager=fresh candidate-services=active credentials=service-specific\n' >>"$HARNESS_LOG"
          fi
          if [[ "$action" == restore ]]; then
            [[ "${2:-}" == false || "${2:-}" == true ]]
            [[ "${3:-}" == "${HARNESS_RUNTIME_USER:-korri}" ]]
            [[ "${4:-}" == "${HARNESS_OLD_KORRID_ACTIVE:-${HARNESS_OLD_ACTIVE:-false}}" ]]
            [[ "${5:-}" == "${HARNESS_OLD_KORRID_ENABLED:-${HARNESS_OLD_ENABLED:-false}}" ]]
            [[ "${6:-}" == "${HARNESS_OLD_SUNSHINE_ACTIVE:-true}" ]]
            [[ "${7:-}" == "${HARNESS_OLD_SUNSHINE_ENABLED:-true}" ]]
            [[ "${8:-}" == "${HARNESS_OLD_X11_ACTIVE:-true}" ]]
            [[ "${9:-}" == "${HARNESS_OLD_X11_ENABLED:-true}" ]]
            pairing_modes="${HARNESS_PAIRING_MODES:-700:600}"
            [[ "${10:-}" == "${pairing_modes%%:*}" ]]
            [[ "${11:-}" == "${pairing_modes#*:}" ]]
            printf 'candidate-services=disabled-stopped user-manager=fresh rollback-groups=restored pairing-modes=%s:%s\n' "${10:-}" "${11:-}" >>"$HARNESS_LOG"
            printf 'restore-enable x11-headless=%s sunshine=%s korrid=%s\n' "$9" "$7" "$5" >>"$HARNESS_LOG"
            if [[ "${HARNESS_RESTORE_ORDER:-dependency}" == sunshine-first ]]; then
              restore_units=(sunshine x11-headless korrid)
            else
              restore_units=(x11-headless sunshine korrid)
            fi
            x11_started=false
            for restore_unit in "${restore_units[@]}"; do
              case "$restore_unit" in
                x11-headless) restore_active="$8" ;;
                sunshine) restore_active="$6" ;;
                korrid) restore_active="$4" ;;
              esac
              [[ "$restore_active" == true ]] || continue
              if [[ "$restore_unit" == sunshine && "$8" == true && "$x11_started" != true ]]; then
                printf 'device gate: Sunshine restore started before X11\n' >&2
                exit 84
              fi
              printf 'restore-start %s\n' "$restore_unit" >>"$HARNESS_LOG"
              [[ "$restore_unit" != x11-headless ]] || x11_started=true
            done
            printf 'restored-old-user korrid=%s/%s sunshine=%s/%s x11-headless=%s/%s\n' \
              "$4" "$5" "$6" "$7" "$8" "$9" >>"$HARNESS_LOG"
          fi
          if [[ "$action" == activate-test && "${HARNESS_MUTATION_SLEEP:-0}" != 0 ]]; then
            if ((HARNESS_MUTATION_SLEEP > remote_deadline)); then
              sleep "$remote_deadline"
              exit 124
            fi
            sleep "$HARNESS_MUTATION_SLEEP"
          fi
          if [[ "$action" == activate-test && "${HARNESS_MUTATION_OUTLIVES_TRANSPORT:-no}" == yes \
            && ! -e "${HARNESS_OUTLIVE_MARKER:?}" ]]; then
            : >"$HARNESS_OUTLIVE_MARKER"
            lock_owned=false
            (
              sleep 0.2
              printf 'remote-mutation-finished\n' >>"$HARNESS_LOG"
              rmdir "$lock_dir"
            ) &
            printf 'local-transport-ended\n' >>"$HARNESS_LOG"
            exit 255
          fi
          ;;
        *) printf 'unexpected remote action: %s\n' "$action" >&2; exit 70 ;;
      esac
      exit 0
    fi
    printf 'unexpected SSH argv: %q\n' "$command" >&2
    exit 69
    ;;
esac

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE="$HERE/device-check.sh"
grep -F "EXPECTED_SUNSHINE_PATCH_SET_SHA256='e330647511163d07c252901908e7c29c435b36459246201162be58bb2eb007d5'" "$GATE" >/dev/null
grep -F 'patch=0020-add-korrid-certificate-control.patch sha256=5ab5b2b5a464c4839f18aa1fc0f304b42b5b8c941fbc4ad5605b34eeaff525e0' "$GATE" >/dev/null
grep -F 'patches=15' "$GATE" >/dev/null
KORRI_LEDGER_PROOF_HELPER="${KORRI_LEDGER_PROOF_HELPER:-${CARGO_TARGET_DIR:-$HERE/../target}/debug/korri-ledger-proof}"
export KORRI_LEDGER_PROOF_HELPER
[[ -x "$KORRI_LEDGER_PROOF_HELPER" ]] || {
  printf 'ledger proof helper is unavailable for tests\n' >&2
  exit 1
}
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
proof_ledger="$TMP/proof-ledger"
proof_outside="$TMP/proof-outside"
mkdir -m 0700 "$proof_ledger"
printf '%s\n' outside >"$proof_outside"
ln -s "$proof_outside" "$proof_ledger/baseline.predicates.next"
proof_identity="$("$KORRI_LEDGER_PROOF_HELPER" identity "$proof_ledger")"
printf '%s\n' proof | "$KORRI_LEDGER_PROOF_HELPER" write-new "$proof_ledger" "$proof_identity" baseline.predicates
[[ "$(cat "$proof_outside")" == outside ]]
[[ "$("$KORRI_LEDGER_PROOF_HELPER" read "$proof_ledger" "$proof_identity" baseline.predicates)" == proof ]]
# The test extracts the exact gate function under test.
# shellcheck disable=SC1090
source <(awk '
  /^remote_canonical_acl\(\) \{/ { capture = 1 }
  capture { print }
  capture && /^}/ { exit }
' "$GATE")
acl_fixture="$TMP/acl-canonicalization"
: >"$acl_fixture"
chmod 0660 "$acl_fixture"
setfacl -b "$acl_fixture"
simple_acl="$(remote_canonical_acl "$acl_fixture")"
[[ "$simple_acl" == ',group::rw-,other::---,user::rw-,' ]]
setfacl -m m::rw- "$acl_fixture"
[[ "$(remote_canonical_acl "$acl_fixture")" == "$simple_acl" ]]
setfacl -m u:1234:r-- "$acl_fixture"
named_read_acl="$(remote_canonical_acl "$acl_fixture")"
[[ "$named_read_acl" != "$simple_acl" ]]
grep -F 'user:1234:r--' <<<"$named_read_acl" >/dev/null
setfacl -m u:1234:rw- "$acl_fixture"
[[ "$(remote_canonical_acl "$acl_fixture")" != "$named_read_acl" ]]

source_gate_function() {
  local name="$1"
  # shellcheck disable=SC1090 # The test extracts the exact gate function under test.
  source <(awk -v signature="$name() {" '
    $0 == signature { capture = 1 }
    capture { print }
    capture && /^}/ { exit }
  ' "$GATE")
}

source_gate_function remote_assert_pid_namespace_hides_compositor
source_gate_function remote_assert_game_display_namespace

modeled_namespace_test() {
  local pid="$1" include_pid_namespace="$2"
  shift 2
  case "$*" in
    'test -S /tmp/.X11-unix/X0') [[ "${MODEL_X0_VISIBLE:-yes}" == yes ]] ;;
    "test -e /run/user/${MODEL_UID:-1000}/korri-wayland") [[ "${MODEL_WAYLAND_ALIAS_VISIBLE:-no}" == yes ]] ;;
    'test -e /run/korri-compositor/sway-ipc.sock') [[ "${MODEL_CONTROL_VISIBLE:-no}" == yes ]] ;;
    "test -e /proc/${MODEL_SWAY_PID:-222}")
      [[ "$include_pid_namespace" == yes && "${MODEL_SWAY_PROCESS_VISIBLE:-no}" == yes ]]
      ;;
    "test -e /proc/${MODEL_SWAY_PID:-222}/root/run/korri-compositor/sway-ipc.sock")
      [[ "$include_pid_namespace" == yes && "${MODEL_SWAY_ROOT_VISIBLE:-no}" == yes ]]
      ;;
    *) printf 'unexpected modeled namespace test for pid %s: %s\n' "$pid" "$*" >&2; return 2 ;;
  esac
}

run_modeled_game_namespace_gate() (
  # shellcheck disable=SC2329 # The extracted production functions call these test doubles indirectly.
  fail() { printf '%s\n' "$1" >&2; exit 1; }
  # shellcheck disable=SC2329 # The extracted production functions call these test doubles indirectly.
  remote_namespace_test() { modeled_namespace_test "$@"; }
  # shellcheck disable=SC2329 # The extracted production functions call these test doubles indirectly.
  remote_namespace_first_extra_x11_socket() { printf '%s' "${MODEL_EXTRA_X11_SOCKET:-}"; }
  # shellcheck disable=SC2329 # The extracted production functions call these test doubles indirectly.
  remote_namespace_first_wayland_socket() { printf '%s' "${MODEL_WAYLAND_SOCKET:-}"; }
  # shellcheck disable=SC2034 # The extracted production function reads this global indirectly.
  COMPOSITOR_CONTROL_SOCKET=/run/korri-compositor/sway-ipc.sock
  # shellcheck disable=SC2034 # The extracted production function reads this global indirectly.
  COMPOSITOR_WAYLAND_ALIAS=korri-wayland
  remote_assert_game_display_namespace 111 "/run/user/${MODEL_UID:-1000}"
  remote_assert_pid_namespace_hides_compositor game 111 "${MODEL_SWAY_PID:-222}"
)

run_modeled_sunshine_namespace_gate() (
  # shellcheck disable=SC2329 # The extracted production function calls this test double indirectly.
  fail() { printf '%s\n' "$1" >&2; exit 1; }
  # shellcheck disable=SC2329 # The extracted production function calls this test double indirectly.
  remote_namespace_test() { modeled_namespace_test "$@"; }
  # shellcheck disable=SC2034 # The extracted production function reads this global indirectly.
  COMPOSITOR_CONTROL_SOCKET=/run/korri-compositor/sway-ipc.sock
  remote_assert_pid_namespace_hides_compositor Sunshine 333 "${MODEL_SWAY_PID:-222}"
)

assert_modeled_namespace_fails() {
  local expected="$1"
  shift
  : >"$TMP/namespace.stdout"
  : >"$TMP/namespace.stderr"
  if "$@" >"$TMP/namespace.stdout" 2>"$TMP/namespace.stderr"; then
    printf 'modeled namespace gate unexpectedly passed: %s\n' "$expected" >&2
    exit 1
  fi
  grep -F "$expected" "$TMP/namespace.stderr" >/dev/null
}

run_modeled_game_namespace_gate
MODEL_X0_VISIBLE=no assert_modeled_namespace_fails \
  'Xwayland display is not visible inside the live game unit' run_modeled_game_namespace_gate
MODEL_EXTRA_X11_SOCKET=/tmp/.X11-unix/X1 assert_modeled_namespace_fails \
  'live game unit can access an unapproved X11 socket' run_modeled_game_namespace_gate
MODEL_WAYLAND_ALIAS_VISIBLE=yes assert_modeled_namespace_fails \
  'stable Wayland socket is visible inside the live game unit' run_modeled_game_namespace_gate
MODEL_WAYLAND_SOCKET=/run/user/1000/wayland-1 assert_modeled_namespace_fails \
  'numeric Wayland socket is visible inside the live game unit' run_modeled_game_namespace_gate
MODEL_CONTROL_VISIBLE=yes assert_modeled_namespace_fails \
  'compositor control is visible inside the live game unit' run_modeled_game_namespace_gate
MODEL_SWAY_PROCESS_VISIBLE=yes assert_modeled_namespace_fails \
  'game PID namespace exposes the compositor process' run_modeled_game_namespace_gate
MODEL_SWAY_ROOT_VISIBLE=yes assert_modeled_namespace_fails \
  'game can reach compositor control through procfs' run_modeled_game_namespace_gate
run_modeled_sunshine_namespace_gate
MODEL_SWAY_PROCESS_VISIBLE=yes assert_modeled_namespace_fails \
  'Sunshine PID namespace exposes the compositor process' run_modeled_sunshine_namespace_gate
MODEL_SWAY_ROOT_VISIBLE=yes assert_modeled_namespace_fails \
  'Sunshine can reach compositor control through procfs' run_modeled_sunshine_namespace_gate

SELF="$(realpath "$0")"
ln -s "$SELF" "$TMP/ssh-command-harness"
mkdir "$TMP/user-scope-bin"
ln -s "$SELF" "$TMP/user-scope-bin/user-scope-id"
ln -s "$SELF" "$TMP/user-scope-bin/user-scope-sudo"
ln -s "$SELF" "$TMP/user-scope-bin/user-scope-systemctl"
ln -s "$SELF" "$TMP/user-scope-bin/user-scope-getent"
ln -s user-scope-id "$TMP/user-scope-bin/id"
ln -s user-scope-sudo "$TMP/user-scope-bin/sudo"
ln -s user-scope-systemctl "$TMP/user-scope-bin/systemctl"
ln -s user-scope-getent "$TMP/user-scope-bin/getent"
export KORRI_DEVICE_GATE_SSH="$TMP/ssh-command-harness"
export HARNESS_LOG="$TMP/commands.log" HARNESS_GATE_SOURCE="$GATE"
HARNESS_RUNTIME_UID="$(id -u)"
export HARNESS_RUNTIME_UID HARNESS_USER_SCOPE_LOG="$TMP/user-scope.log"
export HARNESS_ATTEMPT_MARKER="$TMP/remote-attempt" HARNESS_ATTEMPT_LEASE="$TMP/remote-attempt.lease"
export HARNESS_RUNTIME_USER=korri HARNESS_RUNTIME_HOME="$TMP/korri-home"
mkdir -p "$HARNESS_RUNTIME_HOME/.config/sunshine"
printf '%s\n' 'sunshine-config' >"$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine.conf"
: >"$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine_state.json"
chmod 0700 "$HARNESS_RUNTIME_HOME/.config/sunshine"
chmod 0600 "$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine.conf"
chmod 0600 "$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine_state.json"
export HARNESS_OLD_SUNSHINE_ACTIVE=true HARNESS_OLD_SUNSHINE_ENABLED=true
export HARNESS_OLD_X11_ACTIVE=true HARNESS_OLD_X11_ENABLED=true
MACHINE_ID=0123456789abcdef0123456789abcdef
HOSTNAME=u7-test-host
CANDIDATE=/nix/store/00000000000000000000000000000000-nixos-system-u7-test-host-1
ROLLBACK=/nix/store/11111111111111111111111111111111-nixos-system-u7-test-host-0
RUNTIME_USER=korri
CONTROLLER_ID=0003:045e:02ea:050b
PRODUCTION_PROFILE=korri-60-xbox_one_gamepad.yaml
export CANDIDATE ROLLBACK

run_gate() {
  "$GATE" --host "$HOSTNAME" --expected-machine-id "$MACHINE_ID" --expected-hostname "$HOSTNAME" \
    --candidate "$CANDIDATE" --runtime-user "$RUNTIME_USER" "$@"
}

assert_fails_with() {
  local expected="$1"
  shift
  : >"$TMP/failure.stdout"
  : >"$TMP/failure.stderr"
  if "$@" >"$TMP/failure.stdout" 2>"$TMP/failure.stderr"; then
    printf 'expected command to fail: %s\n' "$*" >&2
    exit 1
  fi
  grep -F "$expected" "$TMP/failure.stderr" >/dev/null || {
    printf 'missing failure text %q; stderr follows:\n' "$expected" >&2
    cat "$TMP/failure.stderr" >&2
    exit 1
  }
}

assert_fails() {
  if "$@"; then
    printf 'expected command to fail: %s\n' "$*" >&2
    exit 1
  fi
}

assert_no_mutation() {
  if grep -E 'action=(activate-test|persistent-switch|restore|inject-health-failure)' "$HARNESS_LOG" >/dev/null; then
    printf 'unexpected mutation; log follows:\n' >&2
    cat "$HARNESS_LOG" >&2
    exit 1
  fi
}

SELECT_LEDGER_HELPER_SOURCE="$(awk '
  /^select_ledger_proof_helper\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
run_helper_selection() (
  # shellcheck disable=SC2329 # Invoked by the production selector loaded with eval.
  fail() { printf 'device gate: %s\n' "$*" >&2; exit 1; }
  eval "$SELECT_LEDGER_HELPER_SOURCE"
  select_ledger_proof_helper "$1" "$2"
)
assert_fails_with 'immutable device gate refuses a ledger proof helper override' \
  run_helper_selection /nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-korri-inputd/bin/korri-device-gate /tmp/forged-helper
[[ "$(run_helper_selection "$GATE" "$KORRI_LEDGER_PROOF_HELPER")" == "$KORRI_LEDGER_PROOF_HELPER" ]]

common_for() {
  local ledger="$1"
  printf '%s\0' --candidate "$CANDIDATE" --rollback-generation "$ROLLBACK" \
    --runtime-user "$RUNTIME_USER" --ledger "$ledger" \
    --expected-controller-id "$CONTROLLER_ID" --production-profile "$PRODUCTION_PROFILE"
}
confirm="CONFIRM-$(printf '%s' "$MACHINE_ID|$HOSTNAME|$CANDIDATE" | sha256sum | cut -c1-16)"

# The root-owned systemd attempt holder starts outside an interactive shell.
# Its immutable candidate-scoped PATH must resolve the byte-identical helper's
# env/bash shebang and bounded holder command.
# This is a literal source-level regression check.
# shellcheck disable=SC2016
grep -F -- '--setenv="PATH=$candidate/sw/bin"' "$GATE" >/dev/null
grep -Fx 'UNHEALTHY_OBSERVE_SECONDS=60' "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariants.
grep -F 'expected="$(readlink -f -- "$wrapper_link"' "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariants.
grep -F 'package_root="${declared%/bin/sunshine}"' "$GATE" >/dev/null
# The process executable is a resolved regular file and may use Sunshine's
# versioned target name. The declared unit path remains bin/sunshine.
# shellcheck disable=SC2016 # Literal rejected production source.
if grep -F '[[ "$running" == /nix/store/*/bin/sunshine ]]' "$GATE" >/dev/null; then
  exit 1
fi
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F 'remote_resolve_sunshine_executable "$running" "$declared"' "$GATE" >/dev/null
grep -F "grep -F 'SUNSHINE_STRICT_ENCODER=1'" "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production journal filter.
grep -F '_SYSTEMD_INVOCATION_ID="$invocation"' "$GATE" >/dev/null
grep -F "first_h264=\"\$(grep -F 'Creating encoder [h264_'" "$GATE" >/dev/null
grep -F "! grep -F 'Creating encoder [h264_vaapi]'" "$GATE" >/dev/null
grep -F "! grep -F 'Unsupported screencopy SHM format:'" "$GATE" >/dev/null
grep -F "! grep -F 'Frame capture failed'" "$GATE" >/dev/null
grep -F "! grep -F \"Couldn't scale frame:\"" "$GATE" >/dev/null
[[ "$(grep -Fc 'run_remote_attempt nvenc-stream-gate' "$GATE")" -eq 3 ]]
grep -F 'PREDICATE_SYSTEM_UNITS=(korrid.service sunshine.service x11-headless.service korri-compositor.service)' "$GATE" >/dev/null
grep -F 'remote_wait_unit korri-compositor.service' "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F 'remote_compositor_gate "$runtime_user"' "$GATE" >/dev/null
COMPOSITOR_MODE_SOURCE="$(awk '
  /^remote_parse_compositor_mode\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$COMPOSITOR_MODE_SOURCE" == remote_parse_compositor_mode* ]]
run_compositor_mode_parser() (
  eval "$COMPOSITOR_MODE_SOURCE"
  remote_parse_compositor_mode "$1"
)
[[ "$(run_compositor_mode_parser '1920x1080@60Hz')" == '1920 1080 60000 1920x1080@60Hz' ]]
[[ "$(run_compositor_mode_parser '1920x1080@120Hz')" == '1920 1080 120000 1920x1080@120Hz' ]]
assert_fails_with 'invalid compositor mode' run_compositor_mode_parser '1920x1080@0Hz'
assert_fails_with 'invalid compositor mode' run_compositor_mode_parser '1920x1080@120'
HIGH_REFRESH_PERFORMANCE_SOURCE="$(awk '
  /^remote_high_refresh_performance_gate\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$HIGH_REFRESH_PERFORMANCE_SOURCE" == remote_high_refresh_performance_gate* ]]
performance_root="$TMP/performance-gate"
mkdir -p "$performance_root/sys/firmware/acpi" "$performance_root/sys/devices/system/cpu/intel_pstate"
printf 'performance\n' >"$performance_root/sys/firmware/acpi/platform_profile"
printf '40\n' >"$performance_root/sys/devices/system/cpu/intel_pstate/min_perf_pct"
printf '60\n' >"$performance_root/sys/devices/system/cpu/intel_pstate/max_perf_pct"
run_high_refresh_performance_gate() (
  model="$1"
  refresh="$2"
  root="$3"
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  fail() { printf 'device gate: %s\n' "$*" >&2; exit 1; }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  systemctl() { [[ "$model" == active ]]; }
  eval "$HIGH_REFRESH_PERFORMANCE_SOURCE"
  remote_high_refresh_performance_gate "$refresh" "$root"
)
run_high_refresh_performance_gate inactive 60000 "$performance_root"
run_high_refresh_performance_gate active 120000 "$performance_root"
assert_fails_with 'performance profile is inactive' run_high_refresh_performance_gate inactive 120000 "$performance_root"
printf 'balanced\n' >"$performance_root/sys/firmware/acpi/platform_profile"
assert_fails_with 'platform profile is not performance' run_high_refresh_performance_gate active 120000 "$performance_root"
printf 'performance\n' >"$performance_root/sys/firmware/acpi/platform_profile"
printf '39\n' >"$performance_root/sys/devices/system/cpu/intel_pstate/min_perf_pct"
assert_fails_with 'minimum Intel pstate bound is not 40' run_high_refresh_performance_gate active 120000 "$performance_root"
printf '40\n' >"$performance_root/sys/devices/system/cpu/intel_pstate/min_perf_pct"
printf '61\n' >"$performance_root/sys/devices/system/cpu/intel_pstate/max_perf_pct"
assert_fails_with 'maximum Intel pstate bound is not 60' run_high_refresh_performance_gate active 120000 "$performance_root"
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F 'run_remote_attempt compositor-game-gate "$RUNTIME_USER"' "$GATE" >/dev/null
grep -F -- '--property=PrivatePIDs=yes' "$HERE/../../korrid/src/host/systemd_unit.rs" >/dev/null
grep -F -- '--property=BindReadOnlyPaths=/tmp/.X11-unix/X0' "$HERE/../../korrid/src/host/systemd_unit.rs" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F 'remote_assert_pid_namespace_hides_compositor Sunshine "$sunshine_pid" "$sway_pid"' "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F '[[ "$jq_helper" == /nix/store/*/bin/jq && -x "$jq_helper" ]]' "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F 'mapfile -t wrapped_targets < <(grep -oE' "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F 'declared_real="$(readlink -f -- "$declared_link"' "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F '[[ "$running" == "$running_target" ]]' "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F '[[ "$control_metadata" == "$uid:$gid:700" ]]' "$GATE" >/dev/null
grep -F 'live game unit can access an unapproved X11 socket' "$GATE" >/dev/null
if grep -F -- '--fork' "$GATE" >/dev/null; then
  printf 'device gate must use the util-linux default PID namespace fork behavior\n' >&2
  exit 1
fi
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F '"$subject PID namespace exposes the compositor process"' "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F '"$subject can reach compositor control through procfs"' "$GATE" >/dev/null
grep -F 'system-korri-compositor=active' "$GATE" >/dev/null
# Candidate and rollback activation must select the bundle declared by that
# generation. A NixOS switch does not replace an existing selector by itself.
# shellcheck disable=SC2016 # Literal production source invariants.
[[ "$(grep -Fc 'remote_switch_generation_bundle "$candidate"' "$GATE")" -eq 2 ]]
# shellcheck disable=SC2016 # Literal production source invariants.
grep -F 'if remote_generation_has_bundle_selector "$rollback"; then' "$GATE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F 'remote_switch_generation_bundle "$rollback"' "$GATE" >/dev/null
grep -F 'systemctl stop korri-bundle-selector.service' "$GATE" >/dev/null
grep -F 'for unit in sunshine.service korrid.service korri-compositor.service korri-inputd.service inputplumber.service' "$GATE" >/dev/null
RESTORE_SOURCE="$(awk '
  /^remote_restore\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
stop_line="$(grep -n 'remote_stop_candidate_services' <<<"$RESTORE_SOURCE" | cut -d: -f1)"
raw_line="$(grep -n 'remote_restore_raw_joystick_udev' <<<"$RESTORE_SOURCE" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal production function call.
clear_line="$(grep -n 'clear_bundle_selector_root "$BUNDLE_SELECTOR_ROOT" 0 0' <<<"$RESTORE_SOURCE" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal production function call.
activate_line="$(grep -n 'remote_activate_generation "$rollback"' <<<"$RESTORE_SOURCE" | head -n1 | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal production function call.
bundle_line="$(grep -n 'remote_switch_generation_bundle "$rollback"' <<<"$RESTORE_SOURCE" | cut -d: -f1)"
[[ -n "$stop_line" && -n "$raw_line" && -n "$clear_line" && -n "$activate_line" && -n "$bundle_line" ]]
[[ "$stop_line" -lt "$raw_line" && "$raw_line" -lt "$clear_line" && "$clear_line" -lt "$activate_line" && "$activate_line" -lt "$bundle_line" ]]
run_modeled_restore() (
  model="$1"
  log="$TMP/modeled-restore-$model.log"
  : >"$log"
  # shellcheck disable=SC2329 # The extracted production function calls these test doubles indirectly.
  fail() { printf '%s\n' "$1" >&2; exit 1; }
  # shellcheck disable=SC2329
  remote_quiesce_launch_authority() { printf 'quiesce\n' >>"$log"; }
  # shellcheck disable=SC2329
  remote_refuse_active_game() { printf 'refuse\n' >>"$log"; }
  # shellcheck disable=SC2329
  remote_stop_candidate_services() { printf 'stop\n' >>"$log"; }
  # shellcheck disable=SC2329
  remote_restore_raw_joystick_udev() { printf 'raw\n' >>"$log"; }
  # shellcheck disable=SC2329
  remote_generation_has_bundle_selector() { [[ "$model" == selector ]]; }
  # shellcheck disable=SC2329
  systemctl() { printf 'selector-stop\n' >>"$log"; }
  # shellcheck disable=SC2329
  clear_bundle_selector_root() { printf 'selector-clear\n' >>"$log"; }
  # shellcheck disable=SC2329
  remote_activate_generation() { printf 'activate-%s\n' "$2" >>"$log"; }
  # shellcheck disable=SC2329
  remote_switch_generation_bundle() { printf 'bundle-switch\n' >>"$log"; }
  # shellcheck disable=SC2329
  remote_set_pairing_state_modes() { printf 'pairing\n' >>"$log"; }
  # shellcheck disable=SC2329
  remote_restore_old_user_units() { printf 'old-units\n' >>"$log"; }
  # shellcheck disable=SC2329
  remote_generation() { printf '%s\n' /nix/store/11111111111111111111111111111111-nixos-system-rollback-1; }
  # shellcheck disable=SC2034 # The extracted production function reads this global.
  BUNDLE_SELECTOR_ROOT=/nix/var/nix/gcroots/korri-bundle
  eval "$RESTORE_SOURCE"
  remote_restore /nix/store/11111111111111111111111111111111-nixos-system-rollback-1 \
    false korri false false false false false false 700 600
  cat "$log"
)
[[ "$(run_modeled_restore selector)" == $'quiesce\nrefuse\nstop\nraw\nactivate-test\nbundle-switch\npairing\nold-units' ]]
[[ "$(run_modeled_restore no-selector)" == $'quiesce\nrefuse\nstop\nraw\nselector-stop\nselector-clear\nactivate-test\npairing\nold-units' ]]

NVENC_LOG_GATE_SOURCE="$(awk '
  /^remote_nvenc_stream_log_gate\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$NVENC_LOG_GATE_SOURCE" == remote_nvenc_stream_log_gate* ]]
run_nvenc_log_gate() (
  # shellcheck disable=SC2034 # Used by the production function loaded with eval.
  COMPOSITOR_WAYLAND_ALIAS=korri-wayland
  eval "$NVENC_LOG_GATE_SOURCE"
  remote_nvenc_stream_log_gate "$1"
)
expect_nvenc_log_failure() {
  if run_nvenc_log_gate "$1"; then
    printf 'NVENC log fixture was unexpectedly accepted\n' >&2
    return 1
  fi
}
wayland_capture=$'Found display [korri-wayland]\n-------- Start of Wayland monitor list --------\nSelected monitor [HEADLESS-1] for streaming\n'
nvenc_success="$wayland_capture"$'New streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [h264_nvenc]'
run_nvenc_log_gate "$nvenc_success"
expect_nvenc_log_failure $'New streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [h264_nvenc]'
expect_nvenc_log_failure "$wayland_capture"$'Streaming display: HEADLESS-1 with res 1920x1080 offset by 0x0\nNew streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [h264_nvenc]'
expect_nvenc_log_failure "$wayland_capture"$'New streaming session started [active sessions: 2]\nCLIENT CONNECTED\nCreating encoder [h264_nvenc]'
expect_nvenc_log_failure "$wayland_capture"$'New streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [hevc_nvenc]'
expect_nvenc_log_failure "$wayland_capture"$'New streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [h264_vaapi]\nCreating encoder [h264_nvenc]'
expect_nvenc_log_failure "$wayland_capture"$'New streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [h264_nvenc]\nCreating encoder [h264_vaapi]'
expect_nvenc_log_failure "$wayland_capture"$'Unsupported screencopy SHM format: 875710274\nNew streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [h264_nvenc]'
expect_nvenc_log_failure "$wayland_capture"$'Frame capture failed\nNew streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [h264_nvenc]'
expect_nvenc_log_failure "$wayland_capture"$"Couldn't scale frame: Invalid argument"$'\nNew streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [h264_nvenc]'
run_nvenc_log_gate "$wayland_capture"$'New streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [h264_vaapi]\nNew streaming session started [active sessions: 1]\nCLIENT CONNECTED\nCreating encoder [h264_nvenc]'
# stat(1) canonicalizes 0700/0600 to 700/600; candidate calls must use the
# canonical forms so post-chmod verification compares equal.
# shellcheck disable=SC2016
[[ "$(grep -Fc 'remote_set_pairing_state_modes "$runtime_user" 700 600' "$GATE")" -eq 2 ]]

# Exercise the production primary-group policy directly. The remote endpoint
# model cannot reproduce a forged primary GID without replacing /proc.
GROUP_POLICY_SOURCE="$(awk '
  /^remote_process_group_policy\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$GROUP_POLICY_SOURCE" == remote_process_group_policy* ]]
run_production_group_policy() (
  local unit="$1" primary_gid="$2" sunshine_supplementary="${3:-yes}"
  local input_gid=10 uinput_gid="${4-20}" control_gid=30 sunshine_gid=40
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  fail() {
    printf 'device gate: %s\n' "$*" >&2
    exit 1
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_pid_reject_supplementary_gid() { :; }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_pid_has_supplementary_gid() {
    [[ "$unit" == sunshine.service && "$sunshine_supplementary" == yes \
      && "$2" == "$sunshine_gid" ]]
  }
  eval "$GROUP_POLICY_SOURCE"
  remote_process_group_policy "$unit" "$$" "$primary_gid" \
    "$input_gid" "$uinput_gid" "$control_gid" "$sunshine_gid"
)
for credential_unit in korrid.service korri-compositor.service sunshine.service \
  korri-inputd.service korri-game-harness.service 'runtime user manager'; do
  assert_fails_with "forbidden input primary group leaked to $credential_unit" \
    run_production_group_policy "$credential_unit" 10
  assert_fails_with "forbidden uinput primary group leaked to $credential_unit" \
    run_production_group_policy "$credential_unit" 20
done
run_production_group_policy korri-inputd.service 30
assert_fails_with 'inputd does not use the control primary group' \
  run_production_group_policy korri-inputd.service 50
assert_fails_with 'control primary group leaked to korrid.service' \
  run_production_group_policy korrid.service 30
run_production_group_policy sunshine.service 50
# Removing the broad legacy uinput group is safer than retaining it. Its absent
# GID must skip only the leak checks tied to that nonexistent authority.
run_production_group_policy sunshine.service 50 yes ''
assert_fails_with 'Sunshine dedicated uinput group must be supplementary' \
  run_production_group_policy sunshine.service 40
assert_fails_with 'system Sunshine lacks its dedicated uinput group' \
  run_production_group_policy sunshine.service 50 no

# Cleanup may use the persisted host-session state only when exact private RPC
# is unavailable and no live game unit exists. Exercise the production proof
# functions directly so a stale control socket cannot strand a safe rollback.
PRIVATE_SESSION_SOURCE="$(awk '
  /^remote_private_session_state_absent\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$PRIVATE_SESSION_SOURCE" == remote_private_session_state_absent* ]]
run_private_session_state_check() (
  local root="$1"
  eval "$PRIVATE_SESSION_SOURCE"
  export KORRID_HOST_SESSION_ROOT="$root"
  remote_private_session_state_absent
)
missing_session_root="$TMP/missing-host-session"
run_private_session_state_check "$missing_session_root"
private_session_root="$TMP/private-host-session"
mkdir -m 0700 "$private_session_root"
run_private_session_state_check "$private_session_root"
chmod 0755 "$private_session_root"
assert_fails run_private_session_state_check "$private_session_root"
chmod 0700 "$private_session_root"
for persisted_session_record in launch-id active.json; do
  : >"$private_session_root/$persisted_session_record"
  assert_fails run_private_session_state_check "$private_session_root"
  rm "$private_session_root/$persisted_session_record"
done
ln -s "$private_session_root" "$TMP/linked-host-session"
assert_fails run_private_session_state_check "$TMP/linked-host-session"

OBSOLETE_SESSION_SOURCE="$(awk '
  /^remote_remove_obsolete_launch_id_atom\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$OBSOLETE_SESSION_SOURCE" == remote_remove_obsolete_launch_id_atom* ]]
run_obsolete_session_cut() (
  local root="$1"
  eval "$OBSOLETE_SESSION_SOURCE"
  export KORRID_HOST_SESSION_ROOT="$root"
  remote_remove_obsolete_launch_id_atom
)
obsolete_session_root="$TMP/obsolete-host-session"
mkdir -m 0700 "$obsolete_session_root"
printf '%s' aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa >"$obsolete_session_root/launch-id"
chmod 0600 "$obsolete_session_root/launch-id"
run_obsolete_session_cut "$obsolete_session_root"
run_private_session_state_check "$obsolete_session_root"
printf '%s' malformed >"$obsolete_session_root/launch-id"
chmod 0600 "$obsolete_session_root/launch-id"
assert_fails run_obsolete_session_cut "$obsolete_session_root"
rm "$obsolete_session_root/launch-id"
: >"$obsolete_session_root/active.json"
chmod 0600 "$obsolete_session_root/active.json"
assert_fails run_obsolete_session_cut "$obsolete_session_root"

RAW_TOPOLOGY_SOURCE="$(awk '
  /^remote_stable_raw_topology_record\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$RAW_TOPOLOGY_SOURCE" == remote_stable_raw_topology_record* ]]
run_raw_topology_record() (
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  fail() {
    printf 'device gate: %s\n' "$*" >&2
    exit 1
  }
  eval "$RAW_TOPOLOGY_SOURCE"
  remote_stable_raw_topology_record "$@"
)
raw_props=$'ID_SERIAL=Microsoft_Controller_exact\nID_PATH=pci-0000:00:14.0-usb-0:4:1.0'
raw_record_input35="$(run_raw_topology_record \
  'Microsoft Xbox Series S|X Controller' 'usb-0000:00:14.0-4/input0' '' \
  '0003:045e:0b12:0501' "$raw_props" \
  '/sys/devices/pci0000:00/0000:00:14.0/usb3/3-4/3-4:1.0/input/input35/event16' yes)"
raw_record_input47="$(run_raw_topology_record \
  'Microsoft Xbox Series S|X Controller' 'usb-0000:00:14.0-4/input0' '' \
  '0003:045e:0b12:0501' "$raw_props" \
  '/sys/devices/pci0000:00/0000:00:14.0/usb3/3-4/3-4:1.0/input/input47/event16' yes)"
[[ "$raw_record_input35" == "$raw_record_input47" ]]
changed_serial_props=$'ID_SERIAL=Microsoft_Controller_replaced\nID_PATH=pci-0000:00:14.0-usb-0:4:1.0'
changed_path_props=$'ID_SERIAL=Microsoft_Controller_exact\nID_PATH=pci-0000:00:14.0-usb-0:5:1.0'
[[ "$raw_record_input35" != "$(run_raw_topology_record \
  'Microsoft Xbox Series S|X Controller' 'usb-0000:00:14.0-4/input0' '' \
  '0003:045e:0b12:0501' "$changed_serial_props" \
  '/sys/devices/pci0000:00/0000:00:14.0/usb3/3-4/3-4:1.0/input/input47/event16' yes)" ]]
[[ "$raw_record_input35" != "$(run_raw_topology_record \
  'Microsoft Xbox Series S|X Controller' 'usb-0000:00:14.0-5/input0' '' \
  '0003:045e:0b12:0501' "$changed_path_props" \
  '/sys/devices/pci0000:00/0000:00:14.0/usb3/3-5/3-5:1.0/input/input47/event16' yes)" ]]
assert_fails_with 'raw controller ID_SERIAL is unavailable' \
  run_raw_topology_record Controller phys '' '0003:045e:0b12:0501' \
  'ID_PATH=pci-0000:00:14.0-usb-0:4:1.0' \
  '/sys/devices/pci0000:00/usb3/3-4/3-4:1.0/input/input47/event16' yes
assert_fails_with 'raw controller ID_PATH is unavailable' \
  run_raw_topology_record Controller phys '' '0003:045e:0b12:0501' \
  'ID_SERIAL=Microsoft_Controller_exact' \
  '/sys/devices/pci0000:00/usb3/3-4/3-4:1.0/input/input47/event16' yes
assert_fails_with 'raw controller sysfs path is not canonical' \
  run_raw_topology_record Controller phys '' '0003:045e:0b12:0501' "$raw_props" \
  '/sys/devices/pci0000:00/usb3/3-4/event16' yes

SELECTOR_CLEAR_SOURCE="$(awk '
  /^clear_bundle_selector_root\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$SELECTOR_CLEAR_SOURCE" == clear_bundle_selector_root* ]]
run_selector_clear() (
  local root="$1"
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  fail() {
    printf 'device gate: %s\n' "$*" >&2
    exit 1
  }
  eval "$SELECTOR_CLEAR_SOURCE"
  clear_bundle_selector_root "$root" "$(id -u)" "$(id -g)"
)
selector_absent="$TMP/selector-absent"
grep -Fx 'bundle-selector=absent' < <(run_selector_clear "$selector_absent") >/dev/null
for selector_mode in 700 711; do
  selector_root="$TMP/selector-$selector_mode"
  mkdir -m "$selector_mode" "$selector_root"
  ln -s /nix/store/00000000000000000000000000000000-korri-bundle-0.0.0 "$selector_root/active"
  ln -s /nix/store/11111111111111111111111111111111-korri-bundle-0.0.0 "$selector_root/previous"
  grep -Fx 'bundle-selector=cleared-orphan' < <(run_selector_clear "$selector_root") >/dev/null
  [[ ! -e "$selector_root" ]]
done
selector_unexpected="$TMP/selector-unexpected"
mkdir -m 0700 "$selector_unexpected"
: >"$selector_unexpected/other"
assert_fails_with 'orphan bundle selector contains an unexpected entry: other' \
  run_selector_clear "$selector_unexpected"
selector_bad_target="$TMP/selector-bad-target"
mkdir -m 0700 "$selector_bad_target"
ln -s /tmp/mutable-bundle "$selector_bad_target/active"
assert_fails_with 'orphan bundle selector target is invalid: active' \
  run_selector_clear "$selector_bad_target"
selector_bad_mode="$TMP/selector-bad-mode"
mkdir -m 0755 "$selector_bad_mode"
assert_fails_with 'orphan bundle selector root ownership or mode is invalid' \
  run_selector_clear "$selector_bad_mode"
ln -s "$TMP" "$TMP/selector-root-link"
assert_fails_with 'orphan bundle selector root is a symbolic link' \
  run_selector_clear "$TMP/selector-root-link"
[[ "$(grep -Fc 'remote_clear_orphan_bundle_selector' "$GATE")" -eq 3 ]]

SELECTOR_SERVICE_SOURCE="$(awk '
  /^remote_bundle_selector_service_loaded\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$SELECTOR_SERVICE_SOURCE" == remote_bundle_selector_service_loaded* ]]
run_selector_service_check() (
  local modeled_load="$1"
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  fail() {
    printf 'device gate: %s\n' "$*" >&2
    exit 1
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  systemctl() {
    [[ "$modeled_load" != query-failure ]] || return 1
    printf '%s\n' "$modeled_load"
  }
  eval "$SELECTOR_SERVICE_SOURCE"
  remote_bundle_selector_service_loaded
)
run_selector_service_check loaded
assert_fails run_selector_service_check not-found
assert_fails_with 'bundle selector service state is unavailable' \
  run_selector_service_check query-failure
assert_fails_with 'bundle selector service has an unexpected load state: masked' \
  run_selector_service_check masked

USER_UNIT_ENABLED_SOURCE="$(awk '
  /^remote_user_unit_enabled\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$USER_UNIT_ENABLED_SOURCE" == remote_user_unit_enabled* ]]
run_user_unit_enabled_check() (
  local modeled_load="$1" modeled_state="${2:-}"
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_user_systemctl() {
    case "$modeled_load" in
      query-failure) return 1 ;;
      incomplete)
        printf 'LoadState=loaded\n'
        return 0
        ;;
    esac
    printf 'LoadState=%s\nUnitFileState=%s\n' "$modeled_load" "$modeled_state"
  }
  eval "$USER_UNIT_ENABLED_SOURCE"
  remote_user_unit_enabled korri legacy.service
)
[[ "$(run_user_unit_enabled_check loaded enabled)" == true ]]
for disabled_state in disabled static masked masked-runtime; do
  [[ "$(run_user_unit_enabled_check loaded "$disabled_state")" == false ]]
done
for masked_state in masked masked-runtime; do
  [[ "$(run_user_unit_enabled_check masked "$masked_state")" == false ]]
done
[[ "$(run_user_unit_enabled_check not-found '')" == false ]]
assert_fails_with 'unexpected user unit enablement state for legacy.service: LoadState=not-found UnitFileState=disabled' \
  run_user_unit_enabled_check not-found disabled
assert_fails_with 'unexpected user unit enablement state for legacy.service: LoadState=masked UnitFileState=disabled' \
  run_user_unit_enabled_check masked disabled
assert_fails_with 'unexpected user unit enablement state for legacy.service: LoadState=loaded UnitFileState=<empty>' \
  run_user_unit_enabled_check loaded ''
assert_fails_with 'incomplete user unit enablement state for legacy.service' \
  run_user_unit_enabled_check incomplete
assert_fails run_user_unit_enabled_check query-failure

# Candidate deployment must not restart the runtime user manager or mutate
# declarative system-unit enablement. Only targeted user daemon reloads and
# targeted Korri service start/stop operations are allowed.
if grep -F 'remote_restart_user_manager' "$GATE" >/dev/null \
  || grep -E 'systemctl (start|stop) "user@' "$GATE" >/dev/null; then
  printf 'device gate can stop or restart the complete runtime user manager\n' >&2
  exit 1
fi
START_CANDIDATE_SOURCE="$(awk '
  /^remote_start_candidate_services\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
STOP_CANDIDATE_SOURCE="$(awk '
  /^remote_stop_candidate_services\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
RESTORE_OLD_USER_SOURCE="$(awk '
  /^remote_restore_old_user_units\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$START_CANDIDATE_SOURCE" == remote_start_candidate_services* ]]
[[ "$STOP_CANDIDATE_SOURCE" == remote_stop_candidate_services* ]]
[[ "$RESTORE_OLD_USER_SOURCE" == remote_restore_old_user_units* ]]
grep -F "systemctl start \"\$unit\"" <<<"$START_CANDIDATE_SOURCE" >/dev/null
grep -F "systemctl stop \"\$unit\"" <<<"$STOP_CANDIDATE_SOURCE" >/dev/null
if grep -E 'systemctl (enable|disable)' <<<"$START_CANDIDATE_SOURCE$STOP_CANDIDATE_SOURCE" >/dev/null; then
  printf 'device gate mutates declarative system-unit enablement\n' >&2
  exit 1
fi
grep -F "remote_user_systemctl \"\$runtime_user\" daemon-reload" <<<"$RESTORE_OLD_USER_SOURCE" >/dev/null

RESOLVE_CONTROLLER_NODE_SOURCE="$(awk '
  /^remote_resolve_physical_controller_node\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$RESOLVE_CONTROLLER_NODE_SOURCE" == remote_resolve_physical_controller_node* ]]
run_controller_node_resolution() (
  eval "$RESOLVE_CONTROLLER_NODE_SOURCE"
  remote_resolve_physical_controller_node "$1" "$2"
)
missing_node="$TMP/missing-controller-node"
[[ "$(run_controller_node_resolution /dev/null "$missing_node")" == 'direct|/dev/null' ]]
[[ "$(run_controller_node_resolution "$missing_node" /dev/null)" == 'isolated|/dev/null' ]]
assert_fails run_controller_node_resolution /dev/null /dev/zero
assert_fails run_controller_node_resolution "$missing_node" "$TMP/also-missing-controller-node"
printf 'not a device\n' >"$TMP/regular-controller-node"
assert_fails run_controller_node_resolution "$TMP/regular-controller-node" "$missing_node"
ln -s /dev/null "$TMP/controller-node-link"
assert_fails run_controller_node_resolution "$TMP/controller-node-link" "$missing_node"

DBUS_OWNER_SOURCE="$(awk '
  /^remote_dbus_unique_owner\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$DBUS_OWNER_SOURCE" == remote_dbus_unique_owner* ]]
run_dbus_owner_check() (
  local model="$1" call_log="$TMP/dbus-owner-$RANDOM.log" owner
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  busctl() {
    printf '%s\n' "$*" >"$call_log"
    case "$model" in
      owned) printf 's ":1.42"\n' ;;
      malformed) printf 's "org.shadowblip.InputPlumber"\n' ;;
      query-failure) return 1 ;;
    esac
  }
  eval "$DBUS_OWNER_SOURCE"
  owner="$(remote_dbus_unique_owner "${2:-org.shadowblip.InputPlumber}")" || return 1
  printf '%s\n' "$owner"
  cat "$call_log"
)
dbus_owner_output="$(run_dbus_owner_check owned)"
[[ "$(head -n 1 <<<"$dbus_owner_output")" == ':1.42' ]]
grep -Fx -- '--system call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetNameOwner s org.shadowblip.InputPlumber' \
  <<<"$dbus_owner_output" >/dev/null
assert_fails run_dbus_owner_check malformed
assert_fails run_dbus_owner_check query-failure
assert_fails run_dbus_owner_check owned invalid-name
if grep -F 'get-name-owner' "$GATE" >/dev/null; then
  printf 'device gate uses a busctl command that systemd 259 does not provide\n' >&2
  exit 1
fi

KORRID_PORT_SOURCE="$(awk '
  /^remote_korrid_api_port\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$KORRID_PORT_SOURCE" == remote_korrid_api_port* ]]
run_korrid_port_check() (
  local model="$1"
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  systemctl() {
    case "$model" in
      port-39217) printf 'PATH=/bin KORRID_ADDRESS=0.0.0.0:39217 OTHER=value\n' ;;
      port-43117) printf 'KORRID_ADDRESS=127.0.0.1:43117\n' ;;
      missing) printf 'PATH=/bin\n' ;;
      duplicate) printf 'KORRID_ADDRESS=0.0.0.0:39217 KORRID_ADDRESS=127.0.0.1:43117\n' ;;
      bad-host) printf 'KORRID_ADDRESS=192.168.1.243:39217\n' ;;
      zero) printf 'KORRID_ADDRESS=0.0.0.0:0\n' ;;
      too-large) printf 'KORRID_ADDRESS=0.0.0.0:65536\n' ;;
      query-failure) return 1 ;;
    esac
  }
  eval "$KORRID_PORT_SOURCE"
  remote_korrid_api_port
)
[[ "$(run_korrid_port_check port-39217)" == 39217 ]]
[[ "$(run_korrid_port_check port-43117)" == 43117 ]]
for invalid_port_model in missing duplicate bad-host zero too-large query-failure; do
  assert_fails run_korrid_port_check "$invalid_port_model"
done
CATALOG_HEALTH_SOURCE="$(awk '
  /^remote_catalog_health\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$CATALOG_HEALTH_SOURCE" == remote_catalog_health* ]]
grep -F 'remote_korrid_api_port' <<<"$CATALOG_HEALTH_SOURCE" >/dev/null
grep -F "http://127.0.0.1:\$port/rpc" <<<"$CATALOG_HEALTH_SOURCE" >/dev/null
if grep -F 'http://127.0.0.1:43117/rpc' <<<"$CATALOG_HEALTH_SOURCE" >/dev/null; then
  printf 'catalog health retained a fixed legacy korrid port\n' >&2
  exit 1
fi
ROLLBACK_GATES_SOURCE="$(awk '
  /^remote_rollback_gates\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$ROLLBACK_GATES_SOURCE" == remote_rollback_gates* ]]
grep -F 'remote_refuse_active_game' <<<"$ROLLBACK_GATES_SOURCE" >/dev/null
grep -F 'remote_private_session_state_absent' <<<"$ROLLBACK_GATES_SOURCE" >/dev/null
if grep -F 'remote_wait_unit inputplumber.service' <<<"$ROLLBACK_GATES_SOURCE" >/dev/null \
  || grep -F 'remote_catalog_health' <<<"$ROLLBACK_GATES_SOURCE" >/dev/null; then
  printf 'rollback gates assume one fixed legacy service layout\n' >&2
  exit 1
fi

PROFILE_SELECT_SOURCE="$(awk '
  /^remote_profile_selects_event\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$PROFILE_SELECT_SOURCE" == remote_profile_selects_event* ]]
grep -F 'busctl --system tree org.shadowblip.InputPlumber --list --no-pager' \
  <<<"$PROFILE_SELECT_SOURCE" >/dev/null
if grep -F 'tree org.shadowblip.InputPlumber /org/shadowblip/InputPlumber' \
  <<<"$PROFILE_SELECT_SOURCE" >/dev/null; then
  printf 'InputPlumber object enumeration passes an object path as a second service name\n' >&2
  exit 1
fi

PHYSICAL_CONTROLLER_SOURCE="$(awk '
  /^remote_physical_controller_evidence\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$PHYSICAL_CONTROLLER_SOURCE" == remote_physical_controller_evidence* ]]
grep -F 'remote_resolve_physical_controller_node' <<<"$PHYSICAL_CONTROLLER_SOURCE" >/dev/null
grep -F "udevadm info --query=property --path=\"\$sysfs\"" <<<"$PHYSICAL_CONTROLLER_SOURCE" >/dev/null
grep -F "stat -Lc '%t:%T' \"\$node\"" <<<"$PHYSICAL_CONTROLLER_SOURCE" >/dev/null
grep -F 'source=%s' <<<"$PHYSICAL_CONTROLLER_SOURCE" >/dev/null
if grep -F "&& -e \"\$node\"" <<<"$PHYSICAL_CONTROLLER_SOURCE" >/dev/null; then
  printf 'physical controller evidence still requires the hidden original node\n' >&2
  exit 1
fi

RESTORE_RAW_JOYSTICK_SOURCE="$(awk '
  /^remote_restore_raw_joystick_udev\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$RESTORE_RAW_JOYSTICK_SOURCE" == remote_restore_raw_joystick_udev* ]]
run_raw_joystick_restore() (
  local model="$1" restore_log="$TMP/raw-joystick-restore-$RANDOM.log"
  : >"$restore_log"
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  fail() {
    printf 'device gate: %s\n' "$*" >&2
    exit 1
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  systemctl() {
    [[ "$model" != active ]] && printf 'inactive\n' || printf 'active\n'
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  find() {
    [[ "$model" != hide-rules ]] || printf '/run/udev/rules.d/50-inputplumber-hide-fixture.rules\n'
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_raw_joystick_events() {
    [[ "$model" == no-events ]] || printf '/sys/class/input/event16\n/sys/class/input/event18\n'
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  sudo() {
    [[ "$model" != sudo-failure ]] || return 1
    printf '%s\n' "$*" >>"$restore_log"
  }
  eval "$RESTORE_RAW_JOYSTICK_SOURCE"
  remote_restore_raw_joystick_udev
  cat "$restore_log"
)
restore_output="$(run_raw_joystick_restore success)"
grep -Fx -- '-n udevadm control --reload-rules' <<<"$restore_output" >/dev/null
grep -Fx -- '-n udevadm trigger --action=add --settle /sys/class/input/event16' <<<"$restore_output" >/dev/null
grep -Fx -- '-n udevadm trigger --action=add --settle /sys/class/input/event18' <<<"$restore_output" >/dev/null
grep -Fx -- '-n udevadm settle --timeout=30' <<<"$restore_output" >/dev/null
[[ "$(grep -c 'trigger --action=add' <<<"$restore_output")" -eq 2 ]]
restore_output="$(run_raw_joystick_restore no-events)"
[[ "$(grep -c 'trigger --action=add' <<<"$restore_output" || true)" -eq 0 ]]
assert_fails_with 'InputPlumber is still active during raw joystick restore' \
  run_raw_joystick_restore active
assert_fails_with 'InputPlumber hide rules remain during raw joystick restore' \
  run_raw_joystick_restore hide-rules
assert_fails_with 'raw joystick udev rule reload failed' \
  run_raw_joystick_restore sudo-failure

ACL_DIGEST_SOURCE="$(awk '
  /^remote_acl_digest\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$ACL_DIGEST_SOURCE" == remote_acl_digest* ]]
grep -F 'remote_raw_joystick_events' <<<"$ACL_DIGEST_SOURCE" >/dev/null
grep -F 'remote_stable_raw_topology_record' <<<"$ACL_DIGEST_SOURCE" >/dev/null
grep -F "stat -Lc '%a:%u:%g'" <<<"$ACL_DIGEST_SOURCE" >/dev/null
if grep -F "stat -Lc '%a:%u:%g:%t:%T'" <<<"$ACL_DIGEST_SOURCE" >/dev/null \
  || grep -F "\"\${event##*/}\"" <<<"$ACL_DIGEST_SOURCE" >/dev/null; then
  printf 'rollback ACL digest retained a volatile event index or device number\n' >&2
  exit 1
fi
REMOTE_RESTORE_SOURCE="$(awk '
  /^remote_restore\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
grep -F 'remote_restore_raw_joystick_udev' <<<"$REMOTE_RESTORE_SOURCE" >/dev/null
[[ "$(grep -Fc 'remote_restore_raw_joystick_udev' "$GATE")" -eq 2 ]]

CURRENT_AUTHORITY_SOURCE="$(awk '
  /^remote_current_launch_authority\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
QUIESCE_AUTHORITY_SOURCE="$(awk '
  /^remote_quiesce_launch_authority\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$CURRENT_AUTHORITY_SOURCE" == remote_current_launch_authority* ]]
[[ "$QUIESCE_AUTHORITY_SOURCE" == remote_quiesce_launch_authority* ]]
run_modeled_authority_quiesce() (
  local log="$TMP/authority-quiesce.log" system_service=active system_socket=active user_service=false
  : >"$log"
  # shellcheck disable=SC2329 # Invoked by the extracted production functions.
  fail() { printf 'device gate: %s\n' "$*" >&2; exit 1; }
  # shellcheck disable=SC2329 # Invoked by the extracted production functions.
  systemctl() {
    printf '%s\n' "$*" >>"$log"
    case "$*" in
      'is-active --quiet korrid.service') [[ "$system_service" == active ]] ;;
      'is-active --quiet korrid-control.socket') [[ "$system_socket" == active ]] ;;
      'stop korrid-control.socket') system_socket=inactive ;;
      'stop korrid.service') system_service=inactive ;;
      *) return 2 ;;
    esac
  }
  # shellcheck disable=SC2329 # Invoked by the extracted production functions.
  remote_user_unit_active() { printf '%s\n' "$user_service"; }
  # shellcheck disable=SC2329 # Invoked by the extracted production functions.
  remote_user_systemctl() {
    printf 'user %s\n' "${*:2}" >>"$log"
    [[ "${*:2}" != 'stop korrid.service' ]] || user_service=false
  }
  KORRID_CONTROL_SOCKET_UNIT=korrid-control.socket
  eval "$CURRENT_AUTHORITY_SOURCE"
  eval "$QUIESCE_AUTHORITY_SOURCE"
  remote_quiesce_launch_authority gameplay >/dev/null
  cat "$log"
)
authority_quiesce_log="$(run_modeled_authority_quiesce)"
socket_stop_line="$(grep -n '^stop korrid-control.socket$' <<<"$authority_quiesce_log" | cut -d: -f1)"
service_stop_line="$(grep -n '^stop korrid.service$' <<<"$authority_quiesce_log" | cut -d: -f1)"
user_stop_line="$(grep -n '^user stop korrid.service$' <<<"$authority_quiesce_log" | cut -d: -f1)"
[[ "$socket_stop_line" =~ ^[0-9]+$ && "$service_stop_line" =~ ^[0-9]+$ && "$user_stop_line" =~ ^[0-9]+$ ]]
[[ "$socket_stop_line" -lt "$service_stop_line" && "$service_stop_line" -lt "$user_stop_line" ]]

grep -F 'systemctl stop "$KORRID_CONTROL_SOCKET_UNIT"' "$GATE" >/dev/null
grep -F 'systemctl is-active --quiet "$KORRID_CONTROL_SOCKET_UNIT"' "$GATE" >/dev/null

for authority_cut_function in remote_activate_test remote_restore remote_inject_health_failure remote_persistent_switch; do
  authority_cut_source="$(awk -v function_name="$authority_cut_function" '
    $0 == function_name "() {" { found=1 }
    found { print }
    found && /^}$/ { exit }
  ' "$GATE")"
  quiesce_line="$(grep -n 'remote_quiesce_launch_authority' <<<"$authority_cut_source" | head -n1 | cut -d: -f1)"
  refuse_line="$(grep -n 'remote_refuse_active_game' <<<"$authority_cut_source" | head -n1 | cut -d: -f1)"
  [[ "$quiesce_line" =~ ^[0-9]+$ && "$refuse_line" =~ ^[0-9]+$ && "$quiesce_line" -lt "$refuse_line" ]]
done
grep -F 'rollback-gates) remote_rollback_gates "${1:?}"' "$GATE" >/dev/null
grep -F 'run_remote_attempt rollback-gates "$GAMEPLAY_USER"' "$GATE" >/dev/null

ACTIVE_GAME_SOURCE="$(awk '
  /^remote_refuse_active_game\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$ACTIVE_GAME_SOURCE" == remote_refuse_active_game* ]]
grep -F "sudo -n -u \"\$KORRID_CONTROL_PEER_USER\" curl" <<<"$ACTIVE_GAME_SOURCE" >/dev/null
quiesced_proof_line="$(grep -n 'remote_launch_authority_quiesced' <<<"$ACTIVE_GAME_SOURCE" | cut -d: -f1)"
live_unit_proof_line="$(grep -n 'systemctl list-units' <<<"$ACTIVE_GAME_SOURCE" | cut -d: -f1)"
cut_atom_line="$(grep -n 'remote_remove_obsolete_launch_id_atom' <<<"$ACTIVE_GAME_SOURCE" | cut -d: -f1)"
[[ "$quiesced_proof_line" -lt "$live_unit_proof_line" && "$live_unit_proof_line" -lt "$cut_atom_line" ]]
run_production_active_game_check() (
  local model="$1" private_state="${2:-empty}" modeled_live_units="${3:-none}"
  export KORRID_CONTROL_PEER_USER=korri-inputd
  export KORRID_CONTROL_SOCKET=/run/korrid-control/control.sock
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  fail() {
    printf 'device gate: %s\n' "$*" >&2
    exit 1
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_launch_authority_quiesced() {
    [[ "$model" != authority-live ]]
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_control_socket_present() {
    [[ "$model" != no-socket ]]
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  sudo() {
    [[ "$1" == -n && "$2" == -u && "$3" == "$KORRID_CONTROL_PEER_USER" ]] || return 1
    shift 3
    "$@"
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  curl() {
    case "$model" in
      rpc-none) printf '%s\n' '{"_tag":"app.session.status","outcome":{"_tag":"Ok","payload":{"active":null}}}' ;;
      rpc-running) printf '%s\n' '{"_tag":"app.session.status","outcome":{"_tag":"Ok","payload":{"active":{"phase":"Running"}}}}' ;;
      rpc-unavailable) return 22 ;;
      no-socket) return 1 ;;
      *) return 1 ;;
    esac
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  systemctl() {
    if [[ "$modeled_live_units" == live ]]; then
      printf '%s\n' 'korri-game-fixture.service loaded active running Fixture'
    fi
    return 0
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_private_session_state_absent() {
    [[ "$private_state" == empty ]]
  }
  remote_remove_obsolete_launch_id_atom() {
    case "$private_state" in
      empty) return 0 ;;
      obsolete) private_state=empty; return 0 ;;
      *) return 1 ;;
    esac
  }
  eval "$ACTIVE_GAME_SOURCE"
  remote_refuse_active_game gameplay
)
grep -Fx 'active-game-check=clear source=local-state' \
  < <(run_production_active_game_check rpc-unavailable) >/dev/null
grep -Fx 'active-game-check=clear source=local-state' \
  < <(run_production_active_game_check no-socket) >/dev/null
assert_fails_with 'obsolete private launch state cannot be removed safely' \
  run_production_active_game_check rpc-unavailable ambiguous
assert_fails_with 'a Korri game unit is live' \
  run_production_active_game_check rpc-unavailable empty live
assert_fails_with 'exact local game status is Running' \
  run_production_active_game_check rpc-running
assert_fails_with 'korrid launch authority is not quiesced' \
  run_production_active_game_check authority-live
# Quiescence plus no live systemd unit permits the one-off removal of only
# the obsolete launch-id atom, even after stopping the activation socket makes
# the exact RPC unavailable. Active or ambiguous new state still blocks.
grep -Fx 'active-game-check=clear source=local-state' \
  < <(run_production_active_game_check no-socket obsolete) >/dev/null
grep -Fx 'active-game-check=clear source=rpc' \
  < <(run_production_active_game_check rpc-none obsolete) >/dev/null
assert_fails_with 'obsolete private launch state cannot be removed safely' \
  run_production_active_game_check rpc-none ambiguous

# Observe both instantaneous failures and restart-counter growth. A service
# that happens to be active during one sample still enters the baseline when
# it restarts during the bounded observation window.
OBSERVED_UNHEALTHY_SOURCE="$(awk '
  /^remote_observed_unhealthy_system_units\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$OBSERVED_UNHEALTHY_SOURCE" == remote_observed_unhealthy_system_units* ]]
run_production_unhealthy_observation() (
  local model="$1" health_probe="$TMP/health-probe-$RANDOM" restart_probe="$TMP/restart-probe-$RANDOM"
  printf '0\n' >"$health_probe"
  printf '0\n' >"$restart_probe"
  export UNHEALTHY_OBSERVE_SECONDS=60
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_unhealthy_system_units() {
    local call
    call="$(<"$health_probe")"
    printf '%s\n' "$((call + 1))" >"$health_probe"
    case "$model:$call" in
      instantaneous:*) printf '%s\n' 'failed-fixture.service' ;;
      resolved:0) printf '%s\n' 'failed-fixture.service' ;;
      *) : ;;
    esac
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_service_restart_counts() {
    local call
    call="$(<"$restart_probe")"
    printf '%s\n' "$((call + 1))" >"$restart_probe"
    case "$model:$call" in
      restart-loop:0) printf '%s\n' 'loop-fixture.service=4' ;;
      restart-loop:1) printf '%s\n' 'loop-fixture.service=5' ;;
      *) printf '%s\n' 'stable-fixture.service=2' ;;
    esac
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  sleep() { :; }
  eval "$OBSERVED_UNHEALTHY_SOURCE"
  remote_observed_unhealthy_system_units
)
[[ -z "$(run_production_unhealthy_observation stable)" ]]
grep -Fx 'loop-fixture.service' < <(run_production_unhealthy_observation restart-loop) >/dev/null
grep -Fx 'failed-fixture.service' < <(run_production_unhealthy_observation instantaneous) >/dev/null
grep -Fx 'failed-fixture.service' < <(run_production_unhealthy_observation resolved) >/dev/null

# NixOS switch-to-configuration reports exit 4 for failed units. Accept that
# narrow status only when the requested generation became current and every
# unhealthy unit was already unhealthy before activation.
ACTIVATE_GENERATION_SOURCE="$(awk '
  /^remote_activate_generation\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$GATE")"
[[ "$ACTIVATE_GENERATION_SOURCE" == remote_activate_generation* ]]
run_production_generation_activation() (
  local model="$1" probe="$TMP/activation-probe-$RANDOM" candidate="$CANDIDATE"
  printf '0\n' >"$probe"
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  fail() {
    printf 'device gate: %s\n' "$*" >&2
    exit 1
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_observed_unhealthy_system_units() {
    local call
    call="$(<"$probe")"
    printf '%s\n' "$((call + 1))" >"$probe"
    if [[ "$call" -eq 0 ]]; then
      printf '%s\n' 'tsnet-proxy-fixture.service'
      return 0
    fi
    case "$model" in
      exit-zero|exit-four-same|generation-mismatch) printf '%s\n' 'tsnet-proxy-fixture.service' ;;
      exit-four-new) printf '%s\n' 'korri-inputd.service' 'tsnet-proxy-fixture.service' ;;
      exit-four-resolved|exit-one) : ;;
    esac
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  sudo() {
    case "$model" in
      exit-zero) return 0 ;;
      exit-one) return 1 ;;
      *) return 4 ;;
    esac
  }
  # shellcheck disable=SC2329 # Invoked by the production function loaded below.
  remote_generation() {
    [[ "$model" != generation-mismatch ]] && printf '%s\n' "$candidate" \
      || printf '%s\n' "$ROLLBACK"
  }
  eval "$ACTIVATE_GENERATION_SOURCE"
  remote_activate_generation "$candidate" test
)
run_production_generation_activation exit-zero
grep -Fx 'activation=accepted status=4 remaining-unhealthy=tsnet-proxy-fixture.service' \
  < <(run_production_generation_activation exit-four-same) >/dev/null
grep -Fx 'activation=accepted status=4 remaining-unhealthy=none' \
  < <(run_production_generation_activation exit-four-resolved) >/dev/null
assert_fails_with 'activation introduced unhealthy system unit: korri-inputd.service' \
  run_production_generation_activation exit-four-new
assert_fails_with 'activation did not make the requested generation current' \
  run_production_generation_activation generation-mismatch
assert_fails_with 'switch-to-configuration failed with status 1' \
  run_production_generation_activation exit-one

# Exercise the production user-manager and pairing proof paths. Both root
# without SUDO_UID and a different SSH caller must target the explicit user.
run_production_predicates() {
  env -u SUDO_UID PATH="$TMP/user-scope-bin:$PATH" HARNESS_MODELED_USER=root \
    KORRI_DEVICE_GATE_TEST_PRIVATE_DIGEST=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
    "$@" "$GATE" --remote predicates "$RUNTIME_USER"
}
: >"$HARNESS_USER_SCOPE_LOG"
user_scope_predicates="$(run_production_predicates HARNESS_CALLER_UID=0)"
grep -Fx 'old-user.sunshine.active=true' <<<"$user_scope_predicates" >/dev/null
grep -Fx 'old-user.sunshine.enabled=true' <<<"$user_scope_predicates" >/dev/null
grep -Fx 'old-user.x11-headless.active=true' <<<"$user_scope_predicates" >/dev/null
grep -Fx 'sunshine.pairing-state-present=true' <<<"$user_scope_predicates" >/dev/null
[[ "$(grep -c '^systemctl-user=' "$HARNESS_USER_SCOPE_LOG")" -eq 6 ]]
if grep '^sudo-user=' "$HARNESS_USER_SCOPE_LOG" | grep -Fv 'sudo-user=korri caller=0 ' >/dev/null; then
  printf 'a root user-scope operation targeted a user other than korri\n' >&2
  exit 1
fi
grep -F "runtime=/run/user/$HARNESS_RUNTIME_UID bus=unix:path=/run/user/$HARNESS_RUNTIME_UID/bus" \
  "$HARNESS_USER_SCOPE_LOG" >/dev/null
: >"$HARNESS_USER_SCOPE_LOG"
run_production_predicates HARNESS_CALLER_UID=2000 >/dev/null
[[ "$(grep -c '^systemctl-user=' "$HARNESS_USER_SCOPE_LOG")" -eq 6 ]]
if grep '^sudo-user=' "$HARNESS_USER_SCOPE_LOG" | grep -Fv 'sudo-user=korri caller=2000 ' >/dev/null; then
  printf 'an SSH user-scope operation targeted a user other than korri\n' >&2
  exit 1
fi
assert_fails_with 'old user unit active state query failed' \
  run_production_predicates HARNESS_CALLER_UID=2000 HARNESS_SUDO_DENY=yes

# Manager errors are not converted to false. Disabled and static are valid
# false enablement states, while inactive is a valid false activity state.
assert_fails_with 'old user unit active state query failed' \
  run_production_predicates HARNESS_USER_QUERY_ERROR=ActiveState:korrid.service
if grep -F 'old-user.korrid.active=false' "$TMP/failure.stdout" >/dev/null; then
  printf 'a user-manager query error was recorded as false\n' >&2
  exit 1
fi
assert_fails_with 'old user unit active state query failed' \
  run_production_predicates HARNESS_USER_ACTIVE_STATE=failed
for enabled_state in disabled static masked masked-runtime; do
  predicates="$(run_production_predicates HARNESS_USER_ACTIVE_STATE=inactive HARNESS_USER_ENABLED_STATE="$enabled_state")"
  grep -Fx 'old-user.korrid.active=false' <<<"$predicates" >/dev/null
  grep -Fx 'old-user.korrid.enabled=false' <<<"$predicates" >/dev/null
done
predicates="$(run_production_predicates \
  HARNESS_USER_ACTIVE_STATE=inactive HARNESS_USER_LOAD_STATE=not-found HARNESS_USER_ENABLED_STATE='')"
grep -Fx 'old-user.korrid.active=false' <<<"$predicates" >/dev/null
grep -Fx 'old-user.korrid.enabled=false' <<<"$predicates" >/dev/null
grep -Fx 'old-user.sunshine.enabled=false' <<<"$predicates" >/dev/null
grep -Fx 'old-user.x11-headless.enabled=false' <<<"$predicates" >/dev/null
for masked_state in masked masked-runtime; do
  predicates="$(run_production_predicates \
    HARNESS_USER_ACTIVE_STATE=inactive HARNESS_USER_LOAD_STATE=masked HARNESS_USER_ENABLED_STATE="$masked_state")"
  grep -Fx 'old-user.korrid.enabled=false' <<<"$predicates" >/dev/null
  grep -Fx 'old-user.sunshine.enabled=false' <<<"$predicates" >/dev/null
done
assert_fails_with 'old user unit enablement query failed' \
  run_production_predicates HARNESS_USER_LOAD_STATE=not-found HARNESS_USER_ENABLED_STATE=disabled

# Sunshine private-state proof rejects permissions and links. It records only
# a combined digest and never exposes pairing or configuration contents.
pairing_secret='PAIRING-CONTENTS-MUST-STAY-PRIVATE'
printf '%s\n' "$pairing_secret" >"$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine_state.json"
predicates="$(run_production_predicates)"
grep -Fx 'sunshine.pairing-state-modes=700:600' <<<"$predicates" >/dev/null
grep -E '^sunshine.private-state-digest=[0-9a-f]{64}$' <<<"$predicates" >/dev/null
private_digest_before="$(sed -n 's/^sunshine.private-state-digest=//p' <<<"$predicates")"
changed_predicates="$(run_production_predicates \
  KORRI_DEVICE_GATE_TEST_PRIVATE_DIGEST=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb)"
private_digest_after="$(sed -n 's/^sunshine.private-state-digest=//p' <<<"$changed_predicates")"
[[ "$private_digest_before" != "$private_digest_after" ]]
predicates="$(run_production_predicates)"
grep -Fx "sunshine.private-state-digest=$private_digest_before" <<<"$predicates" >/dev/null
if grep -F "PRIVATE-CONFIG-CONTENTS-MUST-STAY-PRIVATE" <<<"$changed_predicates" >/dev/null; then
  printf 'Sunshine private configuration leaked into predicates\n' >&2
  exit 1
fi
grep -Fx 'sunshine.pairing-state-present=true' <<<"$predicates" >/dev/null
if grep -F "$pairing_secret" <<<"$predicates" >/dev/null; then
  printf 'pairing-state contents leaked into predicates\n' >&2
  exit 1
fi
chmod 0640 "$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine_state.json"
predicates="$(run_production_predicates)"
grep -Fx 'sunshine.pairing-state-present=false' <<<"$predicates" >/dev/null
chmod 0600 "$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine_state.json"
chmod 0750 "$HARNESS_RUNTIME_HOME/.config/sunshine"
predicates="$(run_production_predicates)"
grep -Fx 'sunshine.pairing-state-present=false' <<<"$predicates" >/dev/null
chmod 0700 "$HARNESS_RUNTIME_HOME/.config/sunshine"
predicates="$(run_production_predicates HARNESS_RUNTIME_UID=1001)"
grep -Fx 'sunshine.pairing-state-present=false' <<<"$predicates" >/dev/null
mv "$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine_state.json" "$TMP/pairing-state"
ln -s "$TMP/pairing-state" "$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine_state.json"
predicates="$(run_production_predicates)"
grep -Fx 'sunshine.pairing-state-present=false' <<<"$predicates" >/dev/null
rm "$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine_state.json"
mv "$TMP/pairing-state" "$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine_state.json"
chmod 0600 "$HARNESS_RUNTIME_HOME/.config/sunshine/sunshine_state.json"
mv "$HARNESS_RUNTIME_HOME/.config/sunshine" "$TMP/sunshine-outside"
ln -s "$TMP/sunshine-outside" "$HARNESS_RUNTIME_HOME/.config/sunshine"
predicates="$(run_production_predicates)"
grep -Fx 'sunshine.pairing-state-present=false' <<<"$predicates" >/dev/null
rm "$HARNESS_RUNTIME_HOME/.config/sunshine"
mv "$TMP/sunshine-outside" "$HARNESS_RUNTIME_HOME/.config/sunshine"

: >"$HARNESS_LOG"
assert_fails_with 'an explicit --host is required' "$GATE" --expected-machine-id "$MACHINE_ID" --expected-hostname "$HOSTNAME"
[[ ! -s "$HARNESS_LOG" ]]
assert_fails_with 'every mode requires an explicit runtime user' "$GATE" --host "$HOSTNAME" \
  --expected-machine-id "$MACHINE_ID" --expected-hostname "$HOSTNAME" --candidate "$CANDIDATE"
[[ ! -s "$HARNESS_LOG" ]]

# Candidate and rollback paths are rejected before the first SSH call. These
# values would create sentinels if interpolated into a remote shell command.
for which in candidate rollback; do
  : >"$HARNESS_LOG"
  sentinel="$TMP/${which}-executed"
  if [[ "$which" == candidate ]]; then
    bad_candidate="${CANDIDATE};touch${IFS}$sentinel"
    assert_fails_with 'strictly valid Nix store generation path' run_gate --mode candidate-test \
      --candidate "$bad_candidate" --rollback-generation "$ROLLBACK" --runtime-user "$RUNTIME_USER" --ledger "$TMP/path-ledger"
  else
    bad_rollback="${ROLLBACK}\$(touch $sentinel)"
    assert_fails_with 'strictly valid Nix store generation path' run_gate --mode candidate-test \
      --candidate "$CANDIDATE" --rollback-generation "$bad_rollback" --runtime-user "$RUNTIME_USER" --ledger "$TMP/path-ledger"
  fi
  [[ ! -e "$sentinel" && ! -s "$HARNESS_LOG" ]]
done
: >"$HARNESS_LOG"
assert_fails_with 'invalid host target' "$GATE" --host "host;touch-$TMP/host-executed" \
  --expected-machine-id "$MACHINE_ID" --expected-hostname "$HOSTNAME"
[[ ! -e "$TMP/host-executed" && ! -s "$HARNESS_LOG" ]]

: >"$HARNESS_LOG"
assert_fails_with 'remote machine-id mismatch' "$GATE" --host "$HOSTNAME" \
  --expected-machine-id ffffffffffffffffffffffffffffffff --expected-hostname "$HOSTNAME" \
  --candidate "$CANDIDATE" --runtime-user "$RUNTIME_USER"
assert_no_mutation

# The candidate helper must be immutable store content and byte-identical to
# the local source before the gate uses it.
export HARNESS_HELPER_STAT=0:755
assert_fails_with 'must be executable and not writable' run_gate
unset HARNESS_HELPER_STAT
export HARNESS_HELPER_DIGEST=mismatch
assert_fails_with 'digest does not match the local gate source' run_gate
unset HARNESS_HELPER_DIGEST

: >"$HARNESS_LOG"
inspection="$(run_gate)"
grep -F 'inspection=complete mutation=none' <<<"$inspection" >/dev/null
grep -F "controller-candidate identity=$CONTROLLER_ID" <<<"$inspection" >/dev/null
assert_no_mutation

# A valid Nix name containing shell metacharacters remains one exact argv item.
quoted_candidate='/nix/store/22222222222222222222222222222222-nixos-system-u7-test-host?variant=one'
quoted_ledger="$TMP/ledger with quote'and space"
export HARNESS_LEDGER="$quoted_ledger"
: >"$HARNESS_LOG"
assert_fails_with 'confirmation token is missing' run_gate --mode candidate-test \
  --candidate "$quoted_candidate" --rollback-generation "$ROLLBACK" --runtime-user "$RUNTIME_USER" --ledger "$quoted_ledger" \
  --expected-controller-id "$CONTROLLER_ID" --production-profile "$PRODUCTION_PROFILE"
grep -F "$(printf '%q' "$quoted_candidate")" "$HARNESS_LOG" >/dev/null
assert_no_mutation
rm -rf "$quoted_ledger"

run_baseline_failure_model() {
  local name="$1" variable="$2" value="$3" expected="$4" ledger
  ledger="$TMP/$name-ledger"
  local -a args
  mapfile -d '' -t args < <(common_for "$ledger")
  export HARNESS_LEDGER="$ledger"
  export "$variable=$value"
  : >"$HARNESS_LOG"
  assert_fails_with "$expected" run_gate --mode candidate-test "${args[@]}" --confirm "$confirm"
  assert_no_mutation
  [[ ! -f "$ledger/state" ]]
  unset "$variable"
}
run_baseline_failure_model baseline-pairing-absent HARNESS_PAIRING_PRESENT false   'Sunshine baseline pairing state is absent'
for private_baseline_model in invalid link special unsafe empty; do
  run_baseline_failure_model "baseline-private-$private_baseline_model"     HARNESS_BASELINE_PRIVATE_MODEL "$private_baseline_model"     'Sunshine baseline private-state digest is invalid'
done

run_failure_model() {
  local name="$1" variable="$2" value="$3" expected="$4" ledger
  ledger="$TMP/$name-ledger"
  local -a args
  mapfile -d '' -t args < <(common_for "$ledger")
  export HARNESS_LEDGER="$ledger"
  export "$variable=$value"
  : >"$HARNESS_LOG"
  assert_fails_with "$expected" run_gate --mode candidate-test "${args[@]}" --confirm "$confirm"
  grep -Fx 'state=failed-needs-inspection' "$ledger/state" >/dev/null
  grep -F 'action=restore' "$HARNESS_LOG" >/dev/null
  unset "$variable"
}
run_failure_model topology HARNESS_TOPOLOGY_MODEL two 'modeled topology has two normalized targets'
run_failure_model provenance HARNESS_PROVENANCE_MODEL invalid 'modeled target provenance/capability fingerprint is invalid'
run_failure_model acl HARNESS_ACL_MODEL raw-readable 'modeled runtime-user ACL exposes raw input'
run_failure_model same-name-raw HARNESS_TOPOLOGY_FIXTURE \
  "$HERE/fixtures/topology-same-name-raw.txt" 'runtime user can read 1 raw controller node'
run_failure_model delegate HARNESS_DELEGATE no 'inputd Delegate is not enabled'
run_failure_model delegate-controllers HARNESS_DELEGATE_CONTROLLERS cpu 'DelegateControllers does not contain pids'
run_failure_model stale-manager HARNESS_STALE_MANAGER_CREDENTIALS yes 'fresh runtime user manager retains forbidden group: input'
for credential_failure in korrid-input korri-compositor-uinput sunshine-control inputd-input game-control korrid-sunshine-uinput inputd-control-primary; do
  run_failure_model "credentials-$credential_failure" HARNESS_CREDENTIAL_FAILURE "$credential_failure" \
    'modeled /proc Groups credential rejection'
done
run_failure_model sunshine-stock HARNESS_SUNSHINE_PACKAGE stock 'candidate Sunshine package is not sunshine-korri'
run_failure_model sunshine-executable HARNESS_SUNSHINE_PACKAGE wrong-executable 'running Sunshine executable differs from the candidate unit'
run_failure_model sunshine-provenance HARNESS_SUNSHINE_PACKAGE bad-provenance 'sunshine-korri provenance does not match the approved patch-set digest'
for provenance_model in duplicate-patch reordered-patch wrong-patch-hash; do
  run_failure_model "sunshine-$provenance_model" HARNESS_SUNSHINE_PACKAGE "$provenance_model"     'sunshine-korri provenance does not match the approved ordered patch manifest'
done
run_failure_model sunshine-wrong-base HARNESS_SUNSHINE_PACKAGE wrong-base   'sunshine-korri provenance has an invalid base_sunshine_version field'
run_failure_model sunshine-wrong-libavcodec HARNESS_SUNSHINE_PACKAGE wrong-libavcodec   'sunshine-korri provenance has an invalid reviewed_libavcodec_version field'
run_failure_model sunshine-wrong-ffmpeg-commit HARNESS_SUNSHINE_PACKAGE wrong-ffmpeg-commit   'sunshine-korri provenance has an invalid reviewed_ffmpeg_commit field'
run_failure_model sunshine-wrong-ffmpeg-source HARNESS_SUNSHINE_PACKAGE wrong-ffmpeg-source   'sunshine-korri provenance has an invalid reviewed_ffmpeg_source_hash field'
run_failure_model sunshine-wrong-nvenc-api HARNESS_SUNSHINE_PACKAGE wrong-nvenc-api   'sunshine-korri provenance has an invalid reviewed_nvenc_api field'
run_failure_model sunshine-private-state HARNESS_PRIVATE_STATE_MODEL unsafe 'Sunshine private configuration tree is unsafe or incomplete'
run_failure_model post-stop-query-error HARNESS_POST_STOP_QUERY_ERROR yes 'old user unit active state query failed after stop'

# Candidate activation refuses every observed active-game signal before it
# stops an old user unit or starts a system replacement. Cleanup also refuses
# to kill the same game.
for active_game_model in exact-running exact-stopping unit-live; do
  active_game_ledger="$TMP/active-game-$active_game_model-ledger"
  mapfile -d '' -t active_game_args < <(common_for "$active_game_ledger")
  export HARNESS_LEDGER="$active_game_ledger" HARNESS_ACTIVE_GAME="$active_game_model"
  : >"$HARNESS_LOG"
  case "$active_game_model" in
    exact-running) refusal='exact local game status is running' ;;
    exact-stopping) refusal='exact local game status is stopping' ;;
    unit-live) refusal='a Korri game unit is live' ;;
  esac
  assert_fails_with "$refusal" run_gate --mode candidate-test "${active_game_args[@]}" --confirm "$confirm"
  if grep -F 'candidate-service-mutation=' "$HARNESS_LOG" >/dev/null; then
    printf 'active-game refusal reached candidate service mutation: %s\n' "$active_game_model" >&2
    exit 1
  fi
  unset HARNESS_ACTIVE_GAME
done

# All 64 active/enabled combinations for the three observed user units are
# captured and handed back to rollback exactly. The modeled Sunshine failure
# occurs after candidate activation, so cleanup exercises complete restoration.
for korrid_active in false true; do
  for korrid_enabled in false true; do
    for sunshine_active in false true; do
      for sunshine_enabled in false true; do
        for x11_active in false true; do
          for x11_enabled in false true; do
            combination="$korrid_active$korrid_enabled$sunshine_active$sunshine_enabled$x11_active$x11_enabled"
            combination_ledger="$TMP/baseline-combination-$combination-ledger"
            mapfile -d '' -t combination_args < <(common_for "$combination_ledger")
            export HARNESS_LEDGER="$combination_ledger"
            export HARNESS_OLD_KORRID_ACTIVE="$korrid_active" HARNESS_OLD_KORRID_ENABLED="$korrid_enabled"
            export HARNESS_OLD_SUNSHINE_ACTIVE="$sunshine_active" HARNESS_OLD_SUNSHINE_ENABLED="$sunshine_enabled"
            export HARNESS_OLD_X11_ACTIVE="$x11_active" HARNESS_OLD_X11_ENABLED="$x11_enabled"
            export HARNESS_CANDIDATE_SERVICE_FAILURE=sunshine.service
            : >"$HARNESS_LOG"
            assert_fails_with 'timed out waiting for sunshine.service ActiveState=active' \
              run_gate --mode candidate-test "${combination_args[@]}" --confirm "$confirm"
            grep -F "restored-old-user korrid=$korrid_active/$korrid_enabled sunshine=$sunshine_active/$sunshine_enabled x11-headless=$x11_active/$x11_enabled" \
              "$HARNESS_LOG" >/dev/null
            grep -F "restore-enable x11-headless=$x11_enabled sunshine=$sunshine_enabled korrid=$korrid_enabled" \
              "$HARNESS_LOG" >/dev/null
            expected_starts=''
            [[ "$x11_active" != true ]] || expected_starts+='x11-headless '
            [[ "$sunshine_active" != true ]] || expected_starts+='sunshine '
            [[ "$korrid_active" != true ]] || expected_starts+='korrid '
            actual_starts="$(awk '/^restore-start / {printf "%s ", $2}' "$HARNESS_LOG")"
            [[ "$actual_starts" == "$expected_starts" ]]
          done
        done
      done
    done
  done
done
unset HARNESS_OLD_KORRID_ACTIVE HARNESS_OLD_KORRID_ENABLED
unset HARNESS_OLD_SUNSHINE_ACTIVE HARNESS_OLD_SUNSHINE_ENABLED
unset HARNESS_OLD_X11_ACTIVE HARNESS_OLD_X11_ENABLED HARNESS_CANDIDATE_SERVICE_FAILURE
export HARNESS_OLD_SUNSHINE_ACTIVE=true HARNESS_OLD_SUNSHINE_ENABLED=true
export HARNESS_OLD_X11_ACTIVE=true HARNESS_OLD_X11_ENABLED=true

# The restore model rejects Sunshine when X11 should have started first.
restore_order_ledger="$TMP/restore-order-ledger"
mapfile -d '' -t restore_order_args < <(common_for "$restore_order_ledger")
export HARNESS_LEDGER="$restore_order_ledger" HARNESS_RESTORE_ORDER=sunshine-first
export HARNESS_OLD_KORRID_ACTIVE=true HARNESS_OLD_KORRID_ENABLED=true
export HARNESS_CANDIDATE_SERVICE_FAILURE=sunshine.service
assert_fails_with 'cleanup rollback failed' run_gate --mode candidate-test \
  "${restore_order_args[@]}" --confirm "$confirm"
unset HARNESS_RESTORE_ORDER HARNESS_CANDIDATE_SERVICE_FAILURE
unset HARNESS_OLD_KORRID_ACTIVE HARNESS_OLD_KORRID_ENABLED

missing_controller_ledger="$TMP/missing-controller-ledger"
: >"$HARNESS_LOG"
assert_fails_with 'require an explicit expected controller identity and production profile' run_gate \
  --mode persistent-switch --candidate "$CANDIDATE" --rollback-generation "$ROLLBACK" \
  --runtime-user "$RUNTIME_USER" --ledger "$missing_controller_ledger" --confirm "$confirm"
assert_no_mutation
assert_fails_with 'unsupported production profile' run_gate --mode persistent-switch \
  --candidate "$CANDIDATE" --rollback-generation "$ROLLBACK" --runtime-user "$RUNTIME_USER" \
  --ledger "$TMP/unsupported-profile-ledger" --expected-controller-id "$CONTROLLER_ID" \
  --production-profile default.yaml --confirm "$confirm"
for controller_model in synthetic virtual unsupported stale generic-joystick two-matching; do
  controller_ledger="$TMP/controller-$controller_model-ledger"
  mapfile -d '' -t controller_args < <(common_for "$controller_ledger")
  export HARNESS_LEDGER="$controller_ledger" HARNESS_CONTROLLER_MODEL="$controller_model"
  : >"$HARNESS_LOG"
  assert_fails_with 'expected supported physical controller identity is not uniquely live' \
    run_gate --mode persistent-switch "${controller_args[@]}" --confirm "$confirm"
  assert_no_mutation
  unset HARNESS_CONTROLLER_MODEL
done

# An interruption immediately after activation leaves a durable failure state,
# performs rollback, and blocks retry until an exact baseline reconcile.
interrupt_ledger="$TMP/interruption-ledger"
mapfile -d '' -t interrupt_args < <(common_for "$interrupt_ledger")
export HARNESS_LEDGER="$interrupt_ledger" HARNESS_AUTOMATED_INTERRUPT=yes
: >"$HARNESS_LOG"
assert_fails_with 'mutation failed; fresh reconcile is required' run_gate --mode candidate-test \
  "${interrupt_args[@]}" --confirm "$confirm"
grep -Fx 'state=failed-needs-inspection' "$interrupt_ledger/state" >/dev/null
grep -F 'action=activate-test' "$HARNESS_LOG" >/dev/null
grep -F 'action=restore' "$HARNESS_LOG" >/dev/null
unset HARNESS_AUTOMATED_INTERRUPT
assert_fails_with 'fresh reconcile is required before retry' run_gate --mode candidate-test \
  "${interrupt_args[@]}" --confirm "$confirm"
export HARNESS_RAW_TOPOLOGY=changed-after-rollback
assert_fails_with 'rollback predicates differ' run_gate --mode reconcile "${interrupt_args[@]}"
unset HARNESS_RAW_TOPOLOGY
run_gate --mode reconcile "${interrupt_args[@]}" >/dev/null
[[ "$(awk -F= '$1 == "state" {print $2}' "$interrupt_ledger/state")" == '' ]]

# Every remote mutation has TERM/KILL timeout behavior and timeout failure uses
# the same rollback/reconcile path.
timeout_ledger="$TMP/timeout-ledger"
mapfile -d '' -t timeout_args < <(common_for "$timeout_ledger")
export HARNESS_LEDGER="$timeout_ledger" HARNESS_MUTATION_SLEEP=3 KORRI_DEVICE_GATE_REMOTE_TIMEOUT=1
export KORRI_DEVICE_GATE_LOCK_WAIT_TIMEOUT=2 KORRI_DEVICE_GATE_SSH_TIMEOUT=4
assert_fails_with 'mutation failed; fresh reconcile is required' run_gate --mode candidate-test \
  "${timeout_args[@]}" --confirm "$confirm"
grep -Fx 'state=failed-needs-inspection' "$timeout_ledger/state" >/dev/null
unset HARNESS_MUTATION_SLEEP KORRI_DEVICE_GATE_REMOTE_TIMEOUT
unset KORRI_DEVICE_GATE_LOCK_WAIT_TIMEOUT KORRI_DEVICE_GATE_SSH_TIMEOUT

# A remote mutation can survive a transport failure. Cleanup waits on the same
# root gate lock, so restore starts only after the remote mutation finishes.
outlive_ledger="$TMP/outlive-ledger"
mapfile -d '' -t outlive_args < <(common_for "$outlive_ledger")
export HARNESS_LEDGER="$outlive_ledger" HARNESS_MUTATION_OUTLIVES_TRANSPORT=yes
export HARNESS_OUTLIVE_MARKER="$TMP/outlive-marker" HARNESS_GATE_LOCK="$TMP/root-gate-lock"
: >"$HARNESS_LOG"
assert_fails_with 'mutation failed; fresh reconcile is required' run_gate --mode candidate-test \
  "${outlive_args[@]}" --confirm "$confirm"
mutation_finished_line="$(grep -n 'remote-mutation-finished' "$HARNESS_LOG" | cut -d: -f1)"
restore_line="$(grep -n 'action=restore' "$HARNESS_LOG" | cut -d: -f1 | tail -1)"
[[ -n "$mutation_finished_line" && -n "$restore_line" && "$mutation_finished_line" -lt "$restore_line" ]]
grep -F 'lock-wait action=restore' "$HARNESS_LOG" >/dev/null
unset HARNESS_MUTATION_OUTLIVES_TRANSPORT HARNESS_OUTLIVE_MARKER HARNESS_GATE_LOCK

# Run a mutation through the real /dev/tty path. The driver reads the private
# nonce from the 0600 ledger and sends the displayed one-time tokens over a PTY.
run_interactive() {
  local mode="$1" ledger_state="$2" ledger="$3" transcript="$4"
  shift 4
  local fifo="$TMP/tty-fifo-$RANDOM" command='' nonce boot token gate
  local -a argv=("$GATE" --host "$HOSTNAME" --expected-machine-id "$MACHINE_ID" --expected-hostname "$HOSTNAME" --mode "$mode" "$@")
  mkfifo "$fifo"
  printf -v command '%q ' "${argv[@]}"
  script -qefc "$command" /dev/null <"$fifo" >"$transcript" 2>&1 &
  local script_pid=$!
  exec 3>"$fifo"
  for _ in $(seq 1 200); do
    nonce="$(awk -F= '$1 == "attempt_nonce" {print $2}' "$ledger/state" 2>/dev/null || true)"
    boot="$(awk -F= '$1 == "boot_id" {print $2}' "$ledger/state" 2>/dev/null || true)"
    [[ "$nonce" =~ ^[0-9a-f]{64}$ && -n "$boot" ]] && break
    kill -0 "$script_pid" 2>/dev/null || break
    sleep 0.02
  done
  [[ "$nonce" =~ ^[0-9a-f]{64}$ && -n "$boot" ]]
  trap '' PIPE
  for gate in normalized-gameplay health-recovery-ambiguity dbus-spoof-and-exclusive-grab exact-stop-and-races direct-action-isolation sunshine-video-controller-recovery catalog-and-session; do
    token="PASS-$(printf '%s' "$MACHINE_ID|$HOSTNAME|$CANDIDATE|$nonce|$boot|$ledger_state|$gate" | sha256sum | cut -c1-16)"
    [[ "${HARNESS_MISTYPE_GATE:-}" != "$gate" ]] || token=MISTYPED-TOKEN
    printf '%s\n' "$token" >&3 || break
  done
  exec 3>&-
  trap - PIPE
  local status=0
  wait "$script_pid" || status=$?
  if grep -F "$nonce" "$transcript" >/dev/null; then
    return 1
  fi
  return "$status"
}

run_abrupt_kill() {
  local mode="$1" pause_action="$2" ledger_state="$3" ledger="$4"
  shift 4
  local pause_marker="$TMP/abrupt-$RANDOM.ready" pid
  export HARNESS_PAUSE_ACTION="$pause_action" HARNESS_PAUSE_MARKER="$pause_marker"
  setsid "$GATE" --host "$HOSTNAME" --expected-machine-id "$MACHINE_ID" --expected-hostname "$HOSTNAME" \
    --mode "$mode" "$@" >"$pause_marker.stdout" 2>"$pause_marker.stderr" &
  pid=$!
  for _ in $(seq 1 500); do
    if [[ -e "$pause_marker" && -e "$HARNESS_ATTEMPT_MARKER" && -e "$HARNESS_ATTEMPT_LEASE" ]] \
      && grep -Fx "state=$ledger_state" "$ledger/state" >/dev/null 2>&1; then
      break
    fi
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.01
  done
  [[ -e "$pause_marker" && -e "$HARNESS_ATTEMPT_MARKER" && -e "$HARNESS_ATTEMPT_LEASE" ]]
  grep -Fx "state=$ledger_state" "$ledger/state" >/dev/null
  kill -KILL -- "-$pid"
  wait "$pid" 2>/dev/null || true
  # The real flock is released by SIGKILL. The mkdir-based harness needs the
  # equivalent modeled release because its EXIT trap cannot run after SIGKILL.
  rmdir "$HARNESS_LOG.lock" 2>/dev/null || true
  unset HARNESS_PAUSE_ACTION HARNESS_PAUSE_MARKER
}

run_startup_abrupt_kill() {
  local mode="$1" window="$2" ledger_state="$3" ledger="$4"
  shift 4
  local pause_marker="$TMP/startup-abrupt-$RANDOM.ready" pid
  export HARNESS_ATTEMPT_START_PAUSE="$window" HARNESS_PAUSE_MARKER="$pause_marker"
  setsid "$GATE" --host "$HOSTNAME" --expected-machine-id "$MACHINE_ID" --expected-hostname "$HOSTNAME" \
    --mode "$mode" "$@" >"$pause_marker.stdout" 2>"$pause_marker.stderr" &
  pid=$!
  for _ in $(seq 1 500); do
    if [[ -e "$pause_marker" ]] && grep -Fx "state=$ledger_state" "$ledger/state" >/dev/null 2>&1; then
      break
    fi
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.01
  done
  [[ -e "$pause_marker" ]]
  grep -Fx "state=$ledger_state" "$ledger/state" >/dev/null
  nonce="$(awk -F= '$1 == "attempt_nonce" {print $2}' "$ledger/state")"
  [[ "$nonce" =~ ^[0-9a-f]{64}$ ]]
  if [[ "$window" == pre-marker ]]; then
    [[ ! -e "$HARNESS_ATTEMPT_MARKER" && ! -e "$HARNESS_ATTEMPT_LEASE" ]]
  else
    grep -Fx "nonce=$nonce" "$HARNESS_ATTEMPT_MARKER" >/dev/null
    grep -Fx "candidate=$CANDIDATE" "$HARNESS_ATTEMPT_MARKER" >/dev/null
    [[ -e "$HARNESS_ATTEMPT_LEASE" ]]
  fi
  kill -KILL -- "-$pid"
  wait "$pid" 2>/dev/null || true
  for _ in $(seq 1 100); do
    ! kill -0 -- "-$pid" 2>/dev/null && break
    sleep 0.01
  done
  if kill -0 -- "-$pid" 2>/dev/null; then
    printf 'modeled gate process group survived SIGKILL: %s\n' "$pid" >&2
    return 1
  fi
  # Production leases are systemd units and survive the local gate process.
  # Restore the proven lease only after all modeled process-group cleanup stops.
  [[ "$window" != post-marker ]] || : >"$HARNESS_ATTEMPT_LEASE"
  rmdir "$HARNESS_LOG.lock" 2>/dev/null || true
  unset HARNESS_ATTEMPT_START_PAUSE HARNESS_PAUSE_MARKER
}

run_stalled_interactive() {
  local ledger="$1" transcript="$2"
  shift 2
  local fifo="$TMP/stalled-tty-$RANDOM" command=''
  local -a argv=("$GATE" --host "$HOSTNAME" --expected-machine-id "$MACHINE_ID" --expected-hostname "$HOSTNAME" --mode candidate-test "$@")
  mkfifo "$fifo"
  printf -v command '%q ' "${argv[@]}"
  script -qefc "$command" /dev/null <"$fifo" >"$transcript" 2>&1 &
  local script_pid=$!
  exec 4>"$fifo"
  local status=0
  wait "$script_pid" || status=$?
  exec 4>&-
  [[ ! -e "$HARNESS_ATTEMPT_MARKER" && ! -e "$HARNESS_ATTEMPT_LEASE" ]]
  return "$status"
}

# A stalled controlling terminal reaches the bounded read timeout while
# rollback is armed. Cleanup restores the baseline and releases the marker.
stalled_ledger="$TMP/stalled-ledger"
mapfile -d '' -t stalled_args < <(common_for "$stalled_ledger")
export HARNESS_LEDGER="$stalled_ledger" KORRI_DEVICE_GATE_HITL_READ_TIMEOUT=1
export KORRI_DEVICE_GATE_HITL_OVERALL_TIMEOUT=2 KORRI_DEVICE_GATE_ATTEMPT_TIMEOUT=4
: >"$HARNESS_LOG"
if run_stalled_interactive "$stalled_ledger" "$TMP/stalled.tty" "${stalled_args[@]}" --confirm "$confirm"; then
  printf 'stalled HITL unexpectedly passed\n' >&2
  exit 1
fi
grep -F 'HITL stage timed out: normalized-gameplay' "$TMP/stalled.tty" >/dev/null
grep -Fx 'state=failed-needs-inspection' "$stalled_ledger/state" >/dev/null
grep -F 'action=restore' "$HARNESS_LOG" >/dev/null
unset KORRI_DEVICE_GATE_HITL_READ_TIMEOUT KORRI_DEVICE_GATE_HITL_OVERALL_TIMEOUT KORRI_DEVICE_GATE_ATTEMPT_TIMEOUT

# The durable remote marker rejects a second invocation throughout the first
# invocation's activation and HITL window.
first_ledger="$TMP/interleaved-first-ledger"
second_ledger="$TMP/interleaved-second-ledger"
mapfile -d '' -t first_args < <(common_for "$first_ledger")
mapfile -d '' -t second_args < <(common_for "$second_ledger")
interleaved_fifo="$TMP/interleaved-fifo"
mkfifo "$interleaved_fifo"
printf -v interleaved_command '%q ' "$GATE" --host "$HOSTNAME" --expected-machine-id "$MACHINE_ID" \
  --expected-hostname "$HOSTNAME" --mode candidate-test "${first_args[@]}" --confirm "$confirm"
HARNESS_LEDGER="$first_ledger" KORRI_DEVICE_GATE_HITL_READ_TIMEOUT=30 \
  KORRI_DEVICE_GATE_HITL_OVERALL_TIMEOUT=60 KORRI_DEVICE_GATE_ATTEMPT_TIMEOUT=90 \
  script -qefc "$interleaved_command" /dev/null <"$interleaved_fifo" >"$TMP/interleaved-first.tty" 2>&1 &
first_pid=$!
exec 5>"$interleaved_fifo"
for _ in $(seq 1 300); do
  [[ -e "$HARNESS_ATTEMPT_MARKER" && -e "$HARNESS_ATTEMPT_LEASE" ]] && break
  kill -0 "$first_pid" 2>/dev/null || break
  sleep 0.02
done
[[ -e "$HARNESS_ATTEMPT_MARKER" && -e "$HARNESS_ATTEMPT_LEASE" ]]

live_reconcile_ledger="$TMP/interleaved-live-reconcile-ledger"
cp -a "$first_ledger" "$live_reconcile_ledger"
sed -i 's/^state=pending-mutation$/state=failed-needs-inspection/' "$live_reconcile_ledger/state"
mapfile -d '' -t live_reconcile_args < <(common_for "$live_reconcile_ledger")
export HARNESS_LEDGER="$live_reconcile_ledger"
assert_fails_with 'attempt is still live; reconcile refuses to race it' run_gate --mode reconcile \
  "${live_reconcile_args[@]}"

export HARNESS_LEDGER="$second_ledger"
assert_fails_with 'another device-gate attempt marker already exists' run_gate --mode candidate-test \
  "${second_args[@]}" --confirm "$confirm"
[[ -e "$HARNESS_ATTEMPT_MARKER" && -e "$HARNESS_ATTEMPT_LEASE" ]]
kill -TERM "$first_pid" 2>/dev/null || true
wait "$first_pid" 2>/dev/null || true
exec 5>&-
for _ in $(seq 1 300); do
  [[ ! -e "$HARNESS_ATTEMPT_MARKER" && ! -e "$HARNESS_ATTEMPT_LEASE" ]] && break
  sleep 0.02
done
[[ ! -e "$HARNESS_ATTEMPT_MARKER" && ! -e "$HARNESS_ATTEMPT_LEASE" ]]

# Reconcile removes the same marker only after its lease is stale and still
# proves the exact rollback baseline before restoring the prior ledger state.
live_nonce="$(awk -F= '$1 == "attempt_nonce" {print $2}' "$live_reconcile_ledger/state")"
printf 'nonce=%s\ncandidate=%s\n' "$live_nonce" "$CANDIDATE" >"$HARNESS_ATTEMPT_MARKER"
export HARNESS_LEDGER="$live_reconcile_ledger"
run_gate --mode reconcile "${live_reconcile_args[@]}" >/dev/null
[[ ! -e "$HARNESS_ATTEMPT_MARKER" ]]

# Attempt startup is two-phase. SIGKILL before marker creation leaves a nonce-
# bearing starting state that reconciles without a marker. SIGKILL after marker
# creation but before the durable in-progress transition honors the exact lease,
# rejects a mismatch, and reconciles only after that exact lease is inactive.
for startup_window in pre-marker post-marker; do
  startup_pending_ledger="$TMP/startup-pending-$startup_window-ledger"
  mapfile -d '' -t startup_pending_args < <(common_for "$startup_pending_ledger")
  export HARNESS_LEDGER="$startup_pending_ledger"
  : >"$HARNESS_LOG"
  run_startup_abrupt_kill candidate-test "$startup_window" pending-mutation-starting \
    "$startup_pending_ledger" "${startup_pending_args[@]}" --confirm "$confirm"
  if [[ "$startup_window" == post-marker ]]; then
    assert_fails_with 'attempt is still live; reconcile refuses to race it' run_gate --mode reconcile \
      "${startup_pending_args[@]}"
    startup_nonce="$(awk -F= '$1 == "attempt_nonce" {print $2}' "$startup_pending_ledger/state")"
    rm -f "$HARNESS_ATTEMPT_LEASE"
    printf 'nonce=%s\ncandidate=%s\n' "f${startup_nonce:1}" "$CANDIDATE" >"$HARNESS_ATTEMPT_MARKER"
    assert_fails_with 'marker does not match this private attempt' run_gate --mode reconcile \
      "${startup_pending_args[@]}"
    printf 'nonce=%s\ncandidate=%s\n' "$startup_nonce" "$CANDIDATE" >"$HARNESS_ATTEMPT_MARKER"
  fi
  run_gate --mode reconcile "${startup_pending_args[@]}" >/dev/null
  grep -Fx 'state=' "$startup_pending_ledger/state" >/dev/null
  grep -E 'wrapper=deadline .*action=predicates' "$HARNESS_LOG" >/dev/null
  [[ ! -e "$HARNESS_ATTEMPT_MARKER" && ! -e "$HARNESS_ATTEMPT_LEASE" ]]
done

# SIGKILL bypasses every local cleanup trap. A stale pending mutation remains
# blocked while its matching remote lease is active. Once inactive, reconcile
# requires the exact marker nonce/candidate and the complete rollback baseline.
abrupt_pending_ledger="$TMP/abrupt-pending-ledger"
mapfile -d '' -t abrupt_pending_args < <(common_for "$abrupt_pending_ledger")
export HARNESS_LEDGER="$abrupt_pending_ledger"
: >"$HARNESS_LOG"
run_abrupt_kill candidate-test activate-test pending-mutation "$abrupt_pending_ledger" \
  "${abrupt_pending_args[@]}" --confirm "$confirm"
assert_fails_with 'attempt is still live; reconcile refuses to race it' run_gate --mode reconcile \
  "${abrupt_pending_args[@]}"
pending_nonce="$(awk -F= '$1 == "attempt_nonce" {print $2}' "$abrupt_pending_ledger/state")"
bad_pending_nonce="f${pending_nonce:1}"
[[ "$bad_pending_nonce" != "$pending_nonce" ]] || bad_pending_nonce="e${pending_nonce:1}"
printf 'nonce=%s\ncandidate=%s\n' "$bad_pending_nonce" "$CANDIDATE" >"$HARNESS_ATTEMPT_MARKER"
rm -f "$HARNESS_ATTEMPT_LEASE"
assert_fails_with 'marker does not match this private attempt' run_gate --mode reconcile \
  "${abrupt_pending_args[@]}"
printf 'nonce=%s\ncandidate=%s\n' "$pending_nonce" "$ROLLBACK" >"$HARNESS_ATTEMPT_MARKER"
assert_fails_with 'marker does not match this private attempt' run_gate --mode reconcile \
  "${abrupt_pending_args[@]}"
printf 'nonce=%s\ncandidate=%s\n' "$pending_nonce" "$CANDIDATE" >"$HARNESS_ATTEMPT_MARKER"
run_gate --mode reconcile "${abrupt_pending_args[@]}" >/dev/null
grep -Fx 'state=' "$abrupt_pending_ledger/state" >/dev/null
grep -E 'wrapper=deadline .*action=predicates' "$HARNESS_LOG" >/dev/null
[[ ! -e "$HARNESS_ATTEMPT_MARKER" && ! -e "$HARNESS_ATTEMPT_LEASE" ]]

flow_ledger="$TMP/flow-ledger"
mapfile -d '' -t flow_args < <(common_for "$flow_ledger")
export HARNESS_LEDGER="$flow_ledger" HARNESS_PAIRING_MODES=700:600
: >"$HARNESS_LOG"
run_interactive candidate-test pending-mutation "$flow_ledger" "$TMP/candidate.tty" \
  "${flow_args[@]}" --confirm "$confirm"
grep -Fx 'state=candidate-green' "$flow_ledger/state" >/dev/null
grep -Fx 'old-user.korrid.active=false' "$flow_ledger/baseline.predicates" >/dev/null
grep -Fx 'old-user.sunshine.active=true' "$flow_ledger/baseline.predicates" >/dev/null
grep -Fx 'old-user.sunshine.enabled=true' "$flow_ledger/baseline.predicates" >/dev/null
grep -Fx 'old-user.x11-headless.active=true' "$flow_ledger/baseline.predicates" >/dev/null
grep -Fx 'old-user.x11-headless.enabled=true' "$flow_ledger/baseline.predicates" >/dev/null
grep -Fx 'system.sunshine.active=inactive' "$flow_ledger/baseline.predicates" >/dev/null
grep -Fx 'system.x11-headless.active=inactive' "$flow_ledger/baseline.predicates" >/dev/null
grep -Fx 'sunshine.pairing-state-modes=700:600' "$flow_ledger/baseline.predicates" >/dev/null
grep -F 'pairing-modes=700:600' "$HARNESS_LOG" >/dev/null
grep -Fx 'sunshine.pairing-state-present=true' "$flow_ledger/baseline.predicates" >/dev/null
grep -F 'predicates-user=korri wrapper=attempt-command' "$HARNESS_LOG" >/dev/null
if grep -F 'predicates-user=root' "$HARNESS_LOG" >/dev/null; then
  printf 'rollback comparison read root user-unit state instead of runtime-user state\n' >&2
  exit 1
fi
[[ "$(stat -c %a "$flow_ledger")" == 700 ]]
[[ "$(stat -c %a "$flow_ledger/baseline.predicates")" == 600 ]]
[[ "$(wc -l <"$flow_ledger/consumed-gates")" -eq 7 ]]

# Retained baselines must still name the requested rollback and exactly match
# every current rollback predicate before any mutation is armed.
for stale_kind in generation service; do
  stale_ledger="$TMP/stale-$stale_kind-ledger"
  cp -a "$flow_ledger" "$stale_ledger"
  case "$stale_kind" in
    generation)
      sed "s|^generation.current=.*|generation.current=$CANDIDATE|" \
        "$stale_ledger/baseline.predicates" >"$stale_ledger/baseline.next"
      expected_stale='retained baseline current generation differs from requested rollback'
      ;;
    service)
      sed 's/^old-user.sunshine.active=true$/old-user.sunshine.active=false/' \
        "$stale_ledger/baseline.predicates" >"$stale_ledger/baseline.next"
      expected_stale='rollback predicates differ from the sanitized baseline'
      ;;
  esac
  chmod 0600 "$stale_ledger/baseline.next"
  mv "$stale_ledger/baseline.next" "$stale_ledger/baseline.predicates"
  mapfile -d '' -t stale_args < <(common_for "$stale_ledger")
  export HARNESS_LEDGER="$stale_ledger"
  : >"$HARNESS_LOG"
  assert_fails_with "$expected_stale" run_gate --mode inject-health-failure \
    "${stale_args[@]}" --confirm "$confirm"
  assert_no_mutation
done

# One session identity binds state to every later baseline proof operation.
identity_race_ledger="$TMP/identity-race-ledger"
identity_race_old="$TMP/identity-race-ledger-old"
cp -a "$flow_ledger" "$identity_race_ledger"
mapfile -d '' -t identity_race_args < <(common_for "$identity_race_ledger")
export HARNESS_LEDGER="$identity_race_ledger"
export KORRI_DEVICE_GATE_TEST_HOOK=after-state-read
export KORRI_DEVICE_GATE_TEST_HOOK_READY="$TMP/identity-race.ready"
export KORRI_DEVICE_GATE_TEST_HOOK_RELEASE="$TMP/identity-race.release"
: >"$HARNESS_LOG"
run_gate --mode inject-health-failure "${identity_race_args[@]}" --confirm "$confirm" \
  >"$TMP/identity-race.stdout" 2>"$TMP/identity-race.stderr" &
identity_race_pid=$!
for _ in $(seq 1 300); do
  [[ -e "$KORRI_DEVICE_GATE_TEST_HOOK_READY" ]] && break
  kill -0 "$identity_race_pid" 2>/dev/null || break
  sleep 0.01
done
[[ -e "$KORRI_DEVICE_GATE_TEST_HOOK_READY" ]]
mv "$identity_race_ledger" "$identity_race_old"
mkdir -m 0700 "$identity_race_ledger"
cp -a "$identity_race_old/state" "$identity_race_old/baseline.predicates" "$identity_race_ledger/"
printf '%s\n' outside >"$TMP/identity-race-outside"
ln -s "$TMP/identity-race-outside" "$identity_race_ledger/sunshine-private-state.accepted"
touch "$KORRI_DEVICE_GATE_TEST_HOOK_RELEASE"
if wait "$identity_race_pid"; then
  printf 'ledger directory replacement unexpectedly passed\n' >&2
  exit 1
fi
grep -E 'safe baseline proof|ledger proof' "$TMP/identity-race.stderr" >/dev/null
[[ "$(cat "$TMP/identity-race-outside")" == outside ]]
assert_no_mutation
unset KORRI_DEVICE_GATE_TEST_HOOK KORRI_DEVICE_GATE_TEST_HOOK_READY KORRI_DEVICE_GATE_TEST_HOOK_RELEASE

# Resume always reads the baseline proof through the descriptor-bound helper.
for proof_model in symlink hardlink wrong-mode; do
  proof_ledger="$TMP/baseline-proof-$proof_model-ledger"
  cp -a "$flow_ledger" "$proof_ledger"
  proof_outside="$TMP/baseline-proof-$proof_model-outside"
  cp "$proof_ledger/baseline.predicates" "$proof_outside"
  case "$proof_model" in
    symlink)
      rm "$proof_ledger/baseline.predicates"
      ln -s "$proof_outside" "$proof_ledger/baseline.predicates"
      ;;
    hardlink)
      rm "$proof_ledger/baseline.predicates"
      ln "$proof_outside" "$proof_ledger/baseline.predicates"
      ;;
    wrong-mode) chmod 0640 "$proof_ledger/baseline.predicates" ;;
  esac
  mapfile -d '' -t proof_args < <(common_for "$proof_ledger")
  export HARNESS_LEDGER="$proof_ledger"
  assert_fails_with 'ledger state exists without a safe baseline proof' run_gate \
    --mode inject-health-failure "${proof_args[@]}" --confirm "$confirm"
  cmp -s "$proof_outside" "$flow_ledger/baseline.predicates"
done
export HARNESS_LEDGER="$flow_ledger"

run_gate --mode inject-health-failure "${flow_args[@]}" --confirm "$confirm" >/dev/null
grep -Fx 'state=automatic-rollback-green' "$flow_ledger/state" >/dev/null
run_gate --mode rollback "${flow_args[@]}" --confirm "$confirm" >/dev/null
grep -Fx 'state=rollback-await-reboot' "$flow_ledger/state" >/dev/null
export HARNESS_BOOT_ID=boot-two HARNESS_CURRENT_GENERATION="$ROLLBACK"

# Both reboot verification states have the same two recoverable startup
# windows. Reconcile reruns the state-specific checks before it restores the
# explicit resume state.
for startup_window in pre-marker post-marker; do
  startup_rollback_ledger="$TMP/startup-rollback-$startup_window-ledger"
  cp -a "$flow_ledger" "$startup_rollback_ledger"
  mapfile -d '' -t startup_rollback_args < <(common_for "$startup_rollback_ledger")
  export HARNESS_LEDGER="$startup_rollback_ledger"
  : >"$HARNESS_LOG"
  run_startup_abrupt_kill rollback-reboot-verify "$startup_window" rollback-reboot-verifying-starting \
    "$startup_rollback_ledger" "${startup_rollback_args[@]}" --confirm "$confirm"
  if [[ "$startup_window" == post-marker ]]; then
    assert_fails_with 'attempt is still live; reconcile refuses to race it' run_gate --mode reconcile \
      "${startup_rollback_args[@]}"
    rm -f "$HARNESS_ATTEMPT_LEASE"
  fi
  run_gate --mode reconcile "${startup_rollback_args[@]}" >/dev/null
  grep -Fx 'state=rollback-await-reboot' "$startup_rollback_ledger/state" >/dev/null
  grep -E 'wrapper=attempt-command .*action=rollback-gates' "$HARNESS_LOG" >/dev/null
  grep -E 'wrapper=attempt-command .*action=predicates' "$HARNESS_LOG" >/dev/null
  [[ ! -e "$HARNESS_ATTEMPT_MARKER" && ! -e "$HARNESS_ATTEMPT_LEASE" ]]
done

# A SIGKILL during rollback reboot verification leaves its explicit resume
# state intact. Reconcile refuses the live lease, then reruns rollback gates
# and the complete baseline before restoring rollback-await-reboot.
abrupt_rollback_ledger="$TMP/abrupt-rollback-ledger"
cp -a "$flow_ledger" "$abrupt_rollback_ledger"
mapfile -d '' -t abrupt_rollback_args < <(common_for "$abrupt_rollback_ledger")
export HARNESS_LEDGER="$abrupt_rollback_ledger"
: >"$HARNESS_LOG"
run_abrupt_kill rollback-reboot-verify rollback-gates rollback-reboot-verifying \
  "$abrupt_rollback_ledger" "${abrupt_rollback_args[@]}" --confirm "$confirm"
assert_fails_with 'attempt is still live; reconcile refuses to race it' run_gate --mode reconcile \
  "${abrupt_rollback_args[@]}"
rm -f "$HARNESS_ATTEMPT_LEASE"
run_gate --mode reconcile "${abrupt_rollback_args[@]}" >/dev/null
grep -Fx 'state=rollback-await-reboot' "$abrupt_rollback_ledger/state" >/dev/null
grep -E 'wrapper=attempt-command .*action=rollback-gates' "$HARNESS_LOG" >/dev/null
grep -E 'wrapper=attempt-command .*action=predicates' "$HARNESS_LOG" >/dev/null
[[ ! -e "$HARNESS_ATTEMPT_MARKER" && ! -e "$HARNESS_ATTEMPT_LEASE" ]]

export HARNESS_LEDGER="$flow_ledger"
run_gate --mode rollback-reboot-verify "${flow_args[@]}" --confirm "$confirm" >/dev/null
grep -Fx 'state=rollback-reboot-green' "$flow_ledger/state" >/dev/null
grep -E 'wrapper=attempt-command .*action=rollback-gates' "$HARNESS_LOG" >/dev/null

# A failure after the accepted digest rename rolls back, reconciles, and retries
# through the idempotent exact-content write without deleting the proof.
accepted_retry_ledger="$TMP/accepted-retry-ledger"
cp -a "$flow_ledger" "$accepted_retry_ledger"
mapfile -d '' -t accepted_retry_args < <(common_for "$accepted_retry_ledger")
export HARNESS_LEDGER="$accepted_retry_ledger"
export KORRI_DEVICE_GATE_TEST_FAIL_AFTER_ACCEPTED_PROOF=true
if run_interactive persistent-switch pending-mutation "$accepted_retry_ledger" "$TMP/accepted-retry-fail.tty" \
  "${accepted_retry_args[@]}" --confirm "$confirm"; then
  printf 'post-accepted-proof failure unexpectedly passed\n' >&2
  exit 1
fi
grep -F 'modeled failure after accepted Sunshine proof commit' "$TMP/accepted-retry-fail.tty" >/dev/null
grep -Fx 'state=failed-needs-inspection' "$accepted_retry_ledger/state" >/dev/null
[[ -s "$accepted_retry_ledger/sunshine-private-state.accepted" ]]
unset KORRI_DEVICE_GATE_TEST_FAIL_AFTER_ACCEPTED_PROOF
run_gate --mode reconcile "${accepted_retry_args[@]}" >/dev/null
grep -Fx 'state=rollback-reboot-green' "$accepted_retry_ledger/state" >/dev/null
run_interactive persistent-switch pending-mutation "$accepted_retry_ledger" "$TMP/accepted-retry-pass.tty" \
  "${accepted_retry_args[@]}" --confirm "$confirm"
grep -Fx 'state=candidate-await-reboot' "$accepted_retry_ledger/state" >/dev/null

export HARNESS_LEDGER="$flow_ledger"
run_interactive persistent-switch pending-mutation "$flow_ledger" "$TMP/persistent.tty" \
  "${flow_args[@]}" --confirm "$confirm"
grep -Fx 'state=candidate-await-reboot' "$flow_ledger/state" >/dev/null
export HARNESS_BOOT_ID=boot-three HARNESS_CURRENT_GENERATION="$CANDIDATE"

# The accepted proof cannot be read from a replacement ledger directory after
# state was bound to the original directory identity.
accepted_identity_ledger="$TMP/accepted-identity-ledger"
accepted_identity_old="$TMP/accepted-identity-ledger-old"
cp -a "$flow_ledger" "$accepted_identity_ledger"
mapfile -d '' -t accepted_identity_args < <(common_for "$accepted_identity_ledger")
export HARNESS_LEDGER="$accepted_identity_ledger"
export KORRI_DEVICE_GATE_TEST_HOOK=before-accepted-proof-read
export KORRI_DEVICE_GATE_TEST_HOOK_READY="$TMP/accepted-identity.ready"
export KORRI_DEVICE_GATE_TEST_HOOK_RELEASE="$TMP/accepted-identity.release"
run_gate --mode candidate-reboot-verify "${accepted_identity_args[@]}" --confirm "$confirm" \
  >"$TMP/accepted-identity.stdout" 2>"$TMP/accepted-identity.stderr" &
accepted_identity_pid=$!
for _ in $(seq 1 300); do
  [[ -e "$KORRI_DEVICE_GATE_TEST_HOOK_READY" ]] && break
  kill -0 "$accepted_identity_pid" 2>/dev/null || break
  sleep 0.01
done
[[ -e "$KORRI_DEVICE_GATE_TEST_HOOK_READY" ]]
mv "$accepted_identity_ledger" "$accepted_identity_old"
mkdir -m 0700 "$accepted_identity_ledger"
cp -a "$accepted_identity_old/state" "$accepted_identity_old/baseline.predicates" \
  "$accepted_identity_old/sunshine-private-state.accepted" "$accepted_identity_ledger/"
touch "$KORRI_DEVICE_GATE_TEST_HOOK_RELEASE"
if wait "$accepted_identity_pid"; then
  printf 'accepted proof ledger replacement unexpectedly passed\n' >&2
  exit 1
fi
grep -F 'accepted Sunshine private-state proof is absent or unsafe' "$TMP/accepted-identity.stderr" >/dev/null
unset KORRI_DEVICE_GATE_TEST_HOOK KORRI_DEVICE_GATE_TEST_HOOK_READY KORRI_DEVICE_GATE_TEST_HOOK_RELEASE
export HARNESS_LEDGER="$flow_ledger"

# Candidate reboot proof rejects swapped or unsafe accepted-digest artifacts.
for proof_model in symlink hardlink wrong-mode; do
  proof_ledger="$TMP/accepted-proof-$proof_model-ledger"
  cp -a "$flow_ledger" "$proof_ledger"
  proof_outside="$TMP/accepted-proof-$proof_model-outside"
  cp "$proof_ledger/sunshine-private-state.accepted" "$proof_outside"
  case "$proof_model" in
    symlink)
      rm "$proof_ledger/sunshine-private-state.accepted"
      ln -s "$proof_outside" "$proof_ledger/sunshine-private-state.accepted"
      ;;
    hardlink)
      rm "$proof_ledger/sunshine-private-state.accepted"
      ln "$proof_outside" "$proof_ledger/sunshine-private-state.accepted"
      ;;
    wrong-mode) chmod 0640 "$proof_ledger/sunshine-private-state.accepted" ;;
  esac
  mapfile -d '' -t proof_args < <(common_for "$proof_ledger")
  export HARNESS_LEDGER="$proof_ledger"
  assert_fails_with 'accepted Sunshine private-state proof is absent or unsafe' run_gate \
    --mode candidate-reboot-verify "${proof_args[@]}" --confirm "$confirm"
  cmp -s "$proof_outside" "$flow_ledger/sunshine-private-state.accepted"
done
export HARNESS_LEDGER="$flow_ledger"

# The private Sunshine tree accepted after persistent HITL must survive reboot.
cross_reboot_private_ledger="$TMP/cross-reboot-private-ledger"
cp -a "$flow_ledger" "$cross_reboot_private_ledger"
mapfile -d '' -t cross_reboot_private_args < <(common_for "$cross_reboot_private_ledger")
export HARNESS_LEDGER="$cross_reboot_private_ledger"
export HARNESS_PRIVATE_STATE_DIGEST=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
: >"$HARNESS_LOG"
assert_fails_with 'Sunshine private state changed across candidate reboot' run_gate   --mode candidate-reboot-verify "${cross_reboot_private_args[@]}" --confirm "$confirm"
grep -Fx 'state=failed-needs-inspection' "$cross_reboot_private_ledger/state" >/dev/null
assert_fails_with 'Sunshine private state changed across candidate reboot' run_gate   --mode reconcile "${cross_reboot_private_args[@]}"
unset HARNESS_PRIVATE_STATE_DIGEST
rm -f "$HARNESS_ATTEMPT_MARKER" "$HARNESS_ATTEMPT_LEASE"

for startup_window in pre-marker post-marker; do
  startup_candidate_ledger="$TMP/startup-candidate-$startup_window-ledger"
  cp -a "$flow_ledger" "$startup_candidate_ledger"
  mapfile -d '' -t startup_candidate_args < <(common_for "$startup_candidate_ledger")
  export HARNESS_LEDGER="$startup_candidate_ledger"
  : >"$HARNESS_LOG"
  run_startup_abrupt_kill candidate-reboot-verify "$startup_window" candidate-reboot-verifying-starting \
    "$startup_candidate_ledger" "${startup_candidate_args[@]}" --confirm "$confirm"
  grep -Eq '^attempt_nonce=[0-9a-f]{64}$' "$startup_candidate_ledger/state"
  if [[ "$startup_window" == post-marker ]]; then
    assert_fails_with 'attempt is still live; reconcile refuses to race it' run_gate --mode reconcile \
      "${startup_candidate_args[@]}"
    rm -f "$HARNESS_ATTEMPT_LEASE"
  fi
  run_gate --mode reconcile "${startup_candidate_args[@]}" >/dev/null
  grep -Fx 'state=candidate-await-reboot' "$startup_candidate_ledger/state" >/dev/null
  grep -E 'wrapper=attempt-command .*action=automated-gates' "$HARNESS_LOG" >/dev/null
  grep -E 'wrapper=attempt-command .*action=acceptance-fingerprint' "$HARNESS_LOG" >/dev/null
  [[ ! -e "$HARNESS_ATTEMPT_MARKER" && ! -e "$HARNESS_ATTEMPT_LEASE" ]]
done

# A SIGKILL during candidate reboot verification preserves candidate-await-
# reboot. Reconcile refuses the live lease, then checks the candidate generation,
# exact controller/profile, automated topology, and acceptance fingerprint.
abrupt_candidate_ledger="$TMP/abrupt-candidate-ledger"
cp -a "$flow_ledger" "$abrupt_candidate_ledger"
mapfile -d '' -t abrupt_candidate_args < <(common_for "$abrupt_candidate_ledger")
export HARNESS_LEDGER="$abrupt_candidate_ledger"
: >"$HARNESS_LOG"
run_abrupt_kill candidate-reboot-verify automated-gates candidate-reboot-verifying \
  "$abrupt_candidate_ledger" "${abrupt_candidate_args[@]}" --confirm "$confirm"
assert_fails_with 'attempt is still live; reconcile refuses to race it' run_gate --mode reconcile \
  "${abrupt_candidate_args[@]}"
rm -f "$HARNESS_ATTEMPT_LEASE"
run_gate --mode reconcile "${abrupt_candidate_args[@]}" >/dev/null
grep -Fx 'state=candidate-await-reboot' "$abrupt_candidate_ledger/state" >/dev/null
grep -E 'wrapper=attempt-command .*action=automated-gates' "$HARNESS_LOG" >/dev/null
grep -E 'wrapper=attempt-command .*action=acceptance-fingerprint' "$HARNESS_LOG" >/dev/null
[[ ! -e "$HARNESS_ATTEMPT_MARKER" && ! -e "$HARNESS_ATTEMPT_LEASE" ]]

# A mistyped reboot HITL token is a resumable verification failure. Reconcile
# returns to candidate-await-reboot without rolling back the accepted candidate.
cp -a "$flow_ledger" "$TMP/mistyped-ledger"
mistyped_ledger="$TMP/mistyped-ledger"
export HARNESS_LEDGER="$mistyped_ledger" HARNESS_MISTYPE_GATE=normalized-gameplay
: >"$HARNESS_LOG"
if run_interactive candidate-reboot-verify candidate-reboot-verifying "$mistyped_ledger" "$TMP/mistyped.tty" \
  "${flow_args[@]/$flow_ledger/$mistyped_ledger}" --confirm "$confirm"; then
  printf 'mistyped candidate reboot token unexpectedly passed\n' >&2
  exit 1
fi
grep -Fx 'state=failed-needs-inspection' "$mistyped_ledger/state" >/dev/null
grep -Fx 'resume_state=candidate-await-reboot' "$mistyped_ledger/state" >/dev/null
if grep -F 'action=restore' "$HARNESS_LOG" >/dev/null; then
  printf 'candidate reboot verification failure attempted rollback\n' >&2
  exit 1
fi
unset HARNESS_MISTYPE_GATE
run_gate --mode reconcile "${flow_args[@]/$flow_ledger/$mistyped_ledger}" >/dev/null
grep -Fx 'state=candidate-await-reboot' "$mistyped_ledger/state" >/dev/null

# A transient SSH failure in an automated reboot gate follows the same durable
# failure/reconcile path and can then retry successfully.
cp -a "$flow_ledger" "$TMP/transient-ledger"
transient_ledger="$TMP/transient-ledger"
export HARNESS_LEDGER="$transient_ledger" HARNESS_FAIL_ACTION_ONCE=automated-gates
export HARNESS_FAIL_ACTION_MARKER="$TMP/transient-action-marker"
assert_fails_with 'candidate reboot verification failed; fresh reconcile is required' run_gate \
  --mode candidate-reboot-verify "${flow_args[@]/$flow_ledger/$transient_ledger}" --confirm "$confirm"
grep -Fx 'state=failed-needs-inspection' "$transient_ledger/state" >/dev/null
grep -Fx 'resume_state=candidate-await-reboot' "$transient_ledger/state" >/dev/null
unset HARNESS_FAIL_ACTION_ONCE HARNESS_FAIL_ACTION_MARKER
run_gate --mode reconcile "${flow_args[@]/$flow_ledger/$transient_ledger}" >/dev/null
grep -Fx 'state=candidate-await-reboot' "$transient_ledger/state" >/dev/null
run_interactive candidate-reboot-verify candidate-reboot-verifying "$transient_ledger" "$TMP/transient-retry.tty" \
  "${flow_args[@]/$flow_ledger/$transient_ledger}" --confirm "$confirm"
grep -Fx 'state=complete' "$transient_ledger/state" >/dev/null

export HARNESS_LEDGER="$flow_ledger"
run_interactive candidate-reboot-verify candidate-reboot-verifying "$flow_ledger" "$TMP/candidate-reboot.tty" \
  "${flow_args[@]}" --confirm "$confirm"
grep -Fx 'state=complete' "$flow_ledger/state" >/dev/null
grep -E 'wrapper=attempt-command .*action=automated-gates' "$HARNESS_LOG" >/dev/null
grep -E 'wrapper=attempt-command .*action=acceptance-fingerprint' "$HARNESS_LOG" >/dev/null
if grep -E 'wrapper=(direct|deadline) .*action=(automated-gates|acceptance-fingerprint|rollback-gates)' "$HARNESS_LOG" >/dev/null; then
  printf 'post-activation automated command ran without a remote deadline\n' >&2
  exit 1
fi
[[ "$(wc -l <"$flow_ledger/consumed-gates")" -eq 21 ]]
[[ "$(sort -u "$flow_ledger/consumed-gates" | wc -l)" -eq 21 ]]

# Replacement between automated proof and acceptance fails and rolls back.
replacement_ledger="$TMP/replacement-ledger"
mapfile -d '' -t replacement_args < <(common_for "$replacement_ledger")
export HARNESS_LEDGER="$replacement_ledger" HARNESS_REPLACE_TARGET=yes
if run_interactive candidate-test pending-mutation "$replacement_ledger" "$TMP/replacement.tty" \
  "${replacement_args[@]}" --confirm "$confirm"; then
  printf 'replacement acceptance unexpectedly passed\n' >&2
  exit 1
fi
grep -F 'normalized target, Sunshine provenance, or expected physical controller proof changed before acceptance' "$TMP/replacement.tty" >/dev/null
grep -Fx 'state=failed-needs-inspection' "$replacement_ledger/state" >/dev/null
unset HARNESS_REPLACE_TARGET

# Sunshine replacement after the initial automated proof also fails before acceptance.
sunshine_replacement_ledger="$TMP/sunshine-replacement-ledger"
mapfile -d '' -t sunshine_replacement_args < <(common_for "$sunshine_replacement_ledger")
export HARNESS_LEDGER="$sunshine_replacement_ledger" HARNESS_REPLACE_SUNSHINE=yes
if run_interactive candidate-test pending-mutation "$sunshine_replacement_ledger" "$TMP/sunshine-replacement.tty" \
  "${sunshine_replacement_args[@]}" --confirm "$confirm"; then
  printf 'Sunshine replacement acceptance unexpectedly passed
' >&2
  exit 1
fi
grep -F 'normalized target, Sunshine provenance, or expected physical controller proof changed before acceptance' \
  "$TMP/sunshine-replacement.tty" >/dev/null
grep -Fx 'state=failed-needs-inspection' "$sunshine_replacement_ledger/state" >/dev/null
unset HARNESS_REPLACE_SUNSHINE

# Private Sunshine replacement during HITL fails the exact acceptance proof.
private_replacement_ledger="$TMP/private-replacement-ledger"
mapfile -d '' -t private_replacement_args < <(common_for "$private_replacement_ledger")
export HARNESS_LEDGER="$private_replacement_ledger" HARNESS_REPLACE_PRIVATE_STATE=yes
if run_interactive candidate-test pending-mutation "$private_replacement_ledger" "$TMP/private-replacement.tty"   "${private_replacement_args[@]}" --confirm "$confirm"; then
  printf 'private Sunshine replacement acceptance unexpectedly passed
' >&2
  exit 1
fi
grep -F 'normalized target, Sunshine provenance, or expected physical controller proof changed before acceptance'   "$TMP/private-replacement.tty" >/dev/null
grep -Fx 'state=failed-needs-inspection' "$private_replacement_ledger/state" >/dev/null
unset HARNESS_REPLACE_PRIVATE_STATE

# A failed boot-ID fetch after persistent acceptance does not roll back. The
# accepted pending state resumes without repeating the switch or HITL gates.
cp -a "$flow_ledger" "$TMP/resume-ledger"
resume_ledger="$TMP/resume-ledger"
export HARNESS_LEDGER="$resume_ledger"
# Return the copied ledger to the prerequisite state.
awk 'BEGIN{done=0} /^state=/{print "state=rollback-reboot-green"; done=1; next} {print} END{if(!done) print "state=rollback-reboot-green"}' \
  "$resume_ledger/state" >"$resume_ledger/state.next"
mv "$resume_ledger/state.next" "$resume_ledger/state"
chmod 0600 "$resume_ledger/state"
rm -f "$resume_ledger/sunshine-private-state.accepted"
export HARNESS_BOOT_COUNT_FILE="$TMP/boot-count" HARNESS_FAIL_BOOT_ID_AT=2
: >"$HARNESS_BOOT_COUNT_FILE"
printf '0\n' >"$HARNESS_BOOT_COUNT_FILE"
if run_interactive persistent-switch pending-mutation "$resume_ledger" "$TMP/pending-accepted.tty" \
  "${flow_args[@]/$flow_ledger/$resume_ledger}" --confirm "$confirm"; then
  printf 'failed boot-ID fetch unexpectedly passed\n' >&2
  exit 1
fi
grep -Fx 'state=candidate-accepted-pending-boot' "$resume_ledger/state" >/dev/null
if grep -F 'mutation failed; fresh reconcile' "$TMP/pending-accepted.tty" >/dev/null; then
  printf 'accepted pending state incorrectly rolled back\n' >&2
  exit 1
fi
unset HARNESS_BOOT_COUNT_FILE HARNESS_FAIL_BOOT_ID_AT
: >"$HARNESS_LOG"
run_gate --mode persistent-switch "${flow_args[@]/$flow_ledger/$resume_ledger}" --confirm "$confirm" >/dev/null
grep -Fx 'state=candidate-await-reboot' "$resume_ledger/state" >/dev/null
if grep -E 'action=(persistent-switch|activate-test)' "$HARNESS_LOG" >/dev/null; then
  printf 'accepted pending resumption repeated mutation\n' >&2
  exit 1
fi

printf 'inputd device gate shell tests passed\n'
