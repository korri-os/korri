#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash coreutils util-linux
# shellcheck shell=bash
set -euo pipefail
ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
RUN="$(mktemp -d)"
trap 'rm -rf "$RUN"' EXIT
export KORRI_FEDERATION_UNIT_ROOT="$RUN"
# The same production configuration seam as federation-acceptance-check.sh.
export KORRID_PRIVATE_STATE_ROOT="$RUN/host/private"
export KORRID_SUNSHINE_PRIVATE_STATE_ROOT="$RUN/sunshine"
export KORRID_CONTROL_DIRECTORY="$RUN/control"
export KORRID_CONTROL_SOCKET="$RUN/control/control.sock"
export KORRID_COMPOSITOR_CONTROL_DIRECTORY="$RUN/compositor"
export KORRID_CERTIFICATE_CONTROL_DIRECTORY="$RUN/certificate"
KORRID_RUNTIME_UID="$(id -u)"
KORRID_RUNTIME_GID="$(id -g)"
export KORRID_RUNTIME_UID KORRID_RUNTIME_GID
helper="$ROOT/clients/android/test/federation-systemd-fixture.sh"
unit=korri-game-0123456789abcdef0123456789abcdef.service
# SystemdLaunchUnitBackend::launch_arguments, with federation_fixture.rs's
# configured command and empty environment. Keep each argument distinct.
launch=(
  --system --no-ask-password --quiet --collect --service-type=exec
  "--unit=$unit" "--uid=$KORRID_RUNTIME_UID" "--gid=$KORRID_RUNTIME_GID"
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
  "--property=InaccessiblePaths=$KORRID_PRIVATE_STATE_ROOT /run/korrid $KORRID_CONTROL_SOCKET $KORRID_CONTROL_DIRECTORY $KORRID_SUNSHINE_PRIVATE_STATE_ROOT $KORRID_COMPOSITOR_CONTROL_DIRECTORY $KORRID_CERTIFICATE_CONTROL_DIRECTORY /run/user/$KORRID_RUNTIME_UID -/run/korri-input-seat /dev/uinput /dev/inputplumber/sources"
  --property=RestrictSUIDSGID=yes
)
failures=0
reject_launch() {
  local name="$1"
  shift
  if bash "$helper" "$@" >"$RUN/rejected.out" 2>&1; then
    printf 'FAIL: accepted %s\n' "$name" >&2
    failures=$((failures + 1))
  fi
  if [[ -e "$RUN/$unit" ]]; then
    printf 'FAIL: %s recorded a running unit\n' "$name" >&2
    failures=$((failures + 1))
    rm "$RUN/$unit"
  fi
}
reject_launch 'missing separator and command' "${launch[@]}"
reject_launch 'missing command' "${launch[@]}" --
reject_launch 'missing separator' "${launch[@]}" /bin/true
reject_launch 'wrong command' "${launch[@]}" -- /bin/false
reject_launch 'extra command argument' "${launch[@]}" -- /bin/true extra
reject_launch 'unknown option' "${launch[@]}" --unknown -- /bin/true
reject_launch 'unknown property' "${launch[@]}" --property=ExecStart=/bin/false -- /bin/true
reject_launch 'unexpected environment' "${launch[@]}" --setenv=PATH=/tmp -- /bin/true
reject_launch 'unit-only launch' --system --no-ask-password "--unit=$unit" -- /bin/true
reject_launch 'missing required option' "${launch[@]:0:3}" "${launch[@]:4}" -- /bin/true
reject_launch 'duplicate unit' "${launch[@]}" "--unit=$unit" -- /bin/true
reject_launch 'malformed unit option' "${launch[@]:0:5}" --unit "$unit" "${launch[@]:6}" -- /bin/true
reject_launch 'wrong uid' "${launch[@]:0:6}" --uid=invalid "${launch[@]:7}" -- /bin/true
reject_launch 'changed security property' "${launch[@]:0:9}" --property=NoNewPrivileges=no "${launch[@]:10}" -- /bin/true
reject_launch 'missing global options' "${launch[@]:2}" -- /bin/true
reject_launch 'literal launch verb' --system --no-ask-password launch "$unit"
reject_launch 'empty invocation'
reject_launch 'truncated global options' --system --no-ask-password
[[ "$failures" == 0 ]] || exit 1
bash "$helper" --system --no-ask-password list-units 'korri-game-*.service' --state=activating,active,reloading,deactivating --plain --no-legend --no-pager
bash "$helper" "${launch[@]}" -- /bin/true
show() { bash "$helper" --system --no-ask-password show "$unit" --property=LoadState --property=ActiveState --property=FreezerState; }
[[ "$(show)" == $'LoadState=loaded\nActiveState=active\nFreezerState=running' ]]
bash "$helper" --system --no-ask-password freeze "$unit"
[[ "$(show)" == *'FreezerState=frozen' ]]
if bash "$helper" --system --no-ask-password stop "$unit"; then exit 1; fi
bash "$helper" --system --no-ask-password thaw "$unit"
bash "$helper" --system --no-ask-password stop "$unit"
[[ "$(show)" == $'LoadState=not-found\nActiveState=inactive\nFreezerState=running' ]]
if bash "$helper" --system --no-ask-password stop sunshine.service; then exit 1; fi
if bash "$helper" --system --no-ask-password restart "$unit"; then exit 1; fi
echo 'federation systemd fixture checks passed'
