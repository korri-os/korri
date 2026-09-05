#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash coreutils util-linux
# shellcheck shell=bash
# Executable systemd contract implementation. Never calls systemctl or runs games.
set -euo pipefail
: "${KORRI_FEDERATION_UNIT_ROOT:?isolated unit directory required}"
[[ -d "$KORRI_FEDERATION_UNIT_ROOT" && ! -L "$KORRI_FEDERATION_UNIT_ROOT" ]]
exec 8>"$KORRI_FEDERATION_UNIT_ROOT/lock"
flock -w 3 8
printf '%q ' "$@" >>"$KORRI_FEDERATION_UNIT_ROOT/calls"
printf '\n' >>"$KORRI_FEDERATION_UNIT_ROOT/calls"
[[ "$#" -ge 3 && "$1" == --system && "$2" == --no-ask-password ]]
shift 2
verb="$1"
unit=""
if [[ "$verb" == --* ]]; then
  # Match SystemdLaunchUnitBackend::launch_arguments exactly. This fixture's
  # federation_fixture.rs config has no environment and only ["/bin/true"].
  # Deliberately reject new flags until the acceptance contract is reviewed.
  [[ "${4:-}" == --unit=* ]] || { echo 'invalid fixture launch' >&2; exit 2; }
  unit="${4#--unit=}"
  runtime_uid="${KORRID_RUNTIME_UID:-$(id -u)}"
  runtime_gid="${KORRID_RUNTIME_GID:-$(id -g)}"
  expected=(
    --quiet --collect --service-type=exec
    "--unit=$unit" "--uid=$runtime_uid" "--gid=$runtime_gid"
    --property=KillMode=control-group
    --property=NoNewPrivileges=yes
    --property=CapabilityBoundingSet=
    --property=AmbientCapabilities=
    --property=PrivateTmp=yes
    --property=PrivatePIDs=yes
    --property=BindReadOnlyPaths=/tmp/.X11-unix/X0
    --property=ProtectKernelTunables=yes
    --property=ProtectKernelModules=yes
    --property=ProtectControlGroups=yes
    --property=ProtectProc=invisible
    --property=ProcSubset=pid
    "--property=InaccessiblePaths=${KORRID_PRIVATE_STATE_ROOT:?} /run/korrid ${KORRID_CONTROL_SOCKET:?} ${KORRID_CONTROL_DIRECTORY:?} ${KORRID_SUNSHINE_PRIVATE_STATE_ROOT:?} ${KORRID_COMPOSITOR_CONTROL_DIRECTORY:?} ${KORRID_CERTIFICATE_CONTROL_DIRECTORY:?} /run/user/$runtime_uid -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources"
    --property=RestrictSUIDSGID=yes
    -- /bin/true
  )
  [[ "$#" == "${#expected[@]}" ]] || { echo 'invalid fixture launch argument count' >&2; exit 2; }
  for arg in "${expected[@]}"; do
    [[ "$1" == "$arg" ]] || { echo 'invalid fixture launch argument' >&2; exit 2; }
    shift
  done
  verb=launch
else
  # `launch` is internal dispatch, not a systemctl verb.
  [[ "$verb" != launch && "$#" -ge 2 ]]
  unit="$2"
  shift 2
fi
if [[ "$verb" == list-units ]]; then
  [[ "$unit" == 'korri-game-*.service' && "$*" == '--state=activating,active,reloading,deactivating --plain --no-legend --no-pager' ]]
  shopt -s nullglob
  for file in "$KORRI_FEDERATION_UNIT_ROOT"/korri-game-*.service; do
    printf '%s loaded active running\n' "${file##*/}"
  done
  exit 0
fi
[[ "$unit" =~ ^korri-game-[a-f0-9]{32}\.service$ ]] || { echo 'invalid fixture unit' >&2; exit 2; }
file="$KORRI_FEDERATION_UNIT_ROOT/$unit"
state=absent
if [[ -f "$file" ]]; then read -r state <"$file"; fi
case "$verb" in
  launch)
    [[ "$state" == absent ]]
    printf 'running\n' >"$file"
    ;;
  show)
    [[ "$*" == '--property=LoadState --property=ActiveState --property=FreezerState' ]]
    if [[ "$state" == absent ]]; then
      printf 'LoadState=not-found\nActiveState=inactive\nFreezerState=running\n'
    else
      printf 'LoadState=loaded\nActiveState=active\nFreezerState=%s\n' "$state"
    fi
    ;;
  freeze|thaw)
    [[ "$#" == 0 && "$state" != absent ]]
    if [[ "$verb" == freeze ]]; then state=frozen; else state=running; fi
    printf '%s\n' "$state" >"$file"
    ;;
  stop)
    [[ "$#" == 0 && "$state" == running ]]
    rm "$file"
    ;;
  *) echo "unsupported fixture verb: $verb" >&2; exit 2 ;;
esac
