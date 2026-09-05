#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REMOTE="$ROOT/zao-remote.sh"
PUSH="$ROOT/push-zao.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

function_source="$(awk '
  /^quiesce_and_cut_obsolete_session\(\) \{/ { found=1 }
  found { print }
  found && /^}$/ { exit }
' "$REMOTE")"
[[ "$function_source" == quiesce_and_cut_obsolete_session* ]]

mkdir -p "$TMP/bin"
cat >"$TMP/bin/systemctl" <<'EOF'
#!/usr/bin/env bash
set -eu
printf '%s\n' "$*" >>"$HARNESS_LOG"
case "$*" in
  '--system show korrid.service -p ActiveState --value')
    [[ "${HARNESS_SYSTEM_AUTHORITY:-inactive}" != unavailable ]] || exit 69
    printf '%s\n' "${HARNESS_SYSTEM_AUTHORITY-inactive}" ;;
  '--system show korrid-control.socket -p ActiveState --value')
    [[ "${HARNESS_SYSTEM_SOCKET:-inactive}" != unavailable ]] || exit 69
    printf '%s\n' "${HARNESS_SYSTEM_SOCKET-inactive}" ;;
  '--system list-units --type=service --state=activating,active,reloading,deactivating --no-legend --plain korri-game-*.service')
    [[ "${HARNESS_SYSTEM_GAME:-none}" != unavailable ]] || exit 69
    [[ "${HARNESS_SYSTEM_GAME:-none}" == none ]] \
      || printf 'korri-game-system.service loaded active %s\n' "$HARNESS_SYSTEM_GAME"
    ;;
  '--user show korrid.service -p ActiveState --value') cat "$HARNESS_STATE" ;;
  '--user start korrid.service') printf active >"$HARNESS_STATE" ;;
  '--user stop korrid.service')
    [[ "${HARNESS_STOP_FAIL:-no}" != yes ]] || exit 1
    printf inactive >"$HARNESS_STATE" ;;
  '--user is-active --quiet korrid.service') [[ "$(cat "$HARNESS_STATE")" == active ]] ;;
  '--user list-units --type=service --state=activating,active,reloading,deactivating --no-legend --plain korri-game-*.service')
    [[ "${HARNESS_LIVE_UNIT:-no}" != unavailable ]] || exit 69
    [[ "$(cat "$HARNESS_STATE")" == inactive ]] || {
      : >"$HARNESS_SESSION_ROOT/active.json"
      printf 'korri-game-raced.service loaded active running\n'
      exit 0
    }
    [[ "${HARNESS_LIVE_UNIT:-no}" != yes ]] \
      || printf 'korri-game-live.service loaded active running\n'
    ;;
  *) exit 2 ;;
esac
EOF
chmod +x "$TMP/bin/systemctl"

run_cut() (
  export HOME="$1"
  export PATH="$TMP/bin:$PATH"
  export HARNESS_LOG="$2"
  export HARNESS_STATE="$3"
  export HARNESS_SESSION_ROOT="$HOME/.local/state/korrid/private/host-session"
  export HARNESS_LIVE_UNIT="${4:-no}"
  eval "$function_source"
  quiesce_and_cut_obsolete_session
)

home="$TMP/home"
session_root="$home/.local/state/korrid/private/host-session"
mkdir -p "$session_root"
chmod 0700 "$session_root"
printf '%s' aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa >"$session_root/launch-id"
chmod 0600 "$session_root/launch-id"
printf active >"$TMP/state"
: >"$TMP/log"
run_cut "$home" "$TMP/log" "$TMP/state"
[[ ! -e "$session_root/launch-id" ]]
[[ ! -e "$session_root/active.json" ]]
mapfile -t calls <"$TMP/log"
[[ "${calls[0]}" == '--system show korrid.service -p ActiveState --value' ]]
[[ "${calls[1]}" == '--system show korrid-control.socket -p ActiveState --value' ]]
[[ "${calls[3]}" == '--user stop korrid.service' ]]
[[ "${calls[5]}" == '--system list-units --type=service --state=activating,active,reloading,deactivating --no-legend --plain korri-game-*.service' ]]
[[ "${calls[6]}" == '--user list-units --type=service --state=activating,active,reloading,deactivating --no-legend --plain korri-game-*.service' ]]
[[ "$(<"$TMP/state")" == inactive ]]

printf '%s' bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb >"$session_root/launch-id"
chmod 0600 "$session_root/launch-id"
printf active >"$TMP/state"
if run_cut "$home" "$TMP/log" "$TMP/state" yes; then
  echo 'standalone cut accepted a live game unit' >&2
  exit 1
fi
[[ -f "$session_root/launch-id" ]]
[[ "$(<"$TMP/state")" == active ]]

# The shipped launcher uses the system manager. An empty user manager is not
# evidence that its live or frozen game has ended.
for system_game in running frozen unavailable; do
  printf active >"$TMP/state"
  if HARNESS_SYSTEM_GAME="$system_game" run_cut "$home" "$TMP/log" "$TMP/state"; then
    echo "standalone cut accepted system game state: $system_game" >&2
    exit 1
  fi
  [[ -f "$session_root/launch-id" ]]
  [[ "$(<"$TMP/state")" == active ]]
done
for authority_state in active activating reloading deactivating unavailable ''; do
  if HARNESS_SYSTEM_AUTHORITY="$authority_state" run_cut "$home" "$TMP/log" "$TMP/state"; then
    echo "standalone cut accepted system authority: $authority_state" >&2
    exit 1
  fi
  [[ -f "$session_root/launch-id" ]]
  if HARNESS_SYSTEM_SOCKET="$authority_state" run_cut "$home" "$TMP/log" "$TMP/state"; then
    echo "standalone cut accepted system socket: $authority_state" >&2
    exit 1
  fi
  [[ -f "$session_root/launch-id" ]]
done

# The standalone path may query, but must never stop or start system services
# or signal any game unit. Unknown commands in the fixture also fail closed.
if grep -E '^--system (stop|start|restart)|^--(system|user) (stop|start|restart|kill|freeze|thaw) korri-game-' "$TMP/log"; then
  echo 'standalone cut attempted a system authority or game mutation' >&2
  exit 1
fi

# Refusal restores activity, not a running/frozen game or recovery record.
for previous_activity in active inactive; do
  for rejection in user-game user-unavailable record stop-failure; do
    printf '%s' "$previous_activity" >"$TMP/state"
    # Deployment must retain this opaque record without parsing or rewriting it.
    printf '%s' 'retained recovery bytes' >"$session_root/active.json"
    cp -r "$session_root" "$TMP/session-before"
    case "$rejection" in
      user-game) live=yes; stop_fail=no ;;
      user-unavailable) live=unavailable; stop_fail=no ;;
      record) live=no; stop_fail=no ;;
      stop-failure) live=yes; stop_fail=yes ;;
    esac
    if HARNESS_STOP_FAIL="$stop_fail" run_cut "$home" "$TMP/log" "$TMP/state" "$live"; then
      echo "standalone cut accepted refusal fixture: $rejection" >&2
      exit 1
    fi
    [[ "$(<"$TMP/state")" == "$previous_activity" ]]
    diff -r "$TMP/session-before" "$session_root"
    rm -r "$TMP/session-before"
    rm "$session_root/active.json"
  done
done

# With no retained identity, success must leave the user service quiesced for
# installation, not run the refusal handler on an early successful return.
rm "$session_root/launch-id"
for session_directory in empty absent; do
  [[ "$session_directory" != absent ]] || rmdir "$session_root"
  printf active >"$TMP/state"
  run_cut "$home" "$TMP/log" "$TMP/state"
  [[ "$(<"$TMP/state")" == inactive ]]
done

trap_line="$(grep -n '^[[:space:]]*trap rollback_install ERR$' "$REMOTE" | cut -d: -f1)"
cut_line="$(grep -n '^[[:space:]]*if ! quiesce_and_cut_obsolete_session; then$' "$REMOTE" | cut -d: -f1)"
# shellcheck disable=SC2016 # Literal production source invariant.
mutation_line="$(grep -n '^[[:space:]]*ln -sfn "\$profile"' "$REMOTE" | cut -d: -f1)"
[[ "$trap_line" =~ ^[0-9]+$ && "$cut_line" =~ ^[0-9]+$ && "$mutation_line" =~ ^[0-9]+$ ]]
[[ "$trap_line" -lt "$cut_line" && "$cut_line" -lt "$mutation_line" ]]
grep -F 'systemctl --user is-active --quiet korrid.service && previous_service_active=true' "$REMOTE" >/dev/null
# shellcheck disable=SC2016 # Literal production source invariant.
grep -F 'if [[ "$previous_service_active" == true ]]; then' "$REMOTE" >/dev/null
grep -F 'systemctl --user restart korrid.service || true' "$REMOTE" >/dev/null
# The documented caller must ship and execute the reviewed remote installer.
# shellcheck disable=SC2016 # Literal caller source invariants.
grep -F '"$root/services/korrid/deploy/zao-remote.sh"' "$PUSH" >/dev/null
# shellcheck disable=SC2016
grep -F '"zao:$remote_tmp/zao-remote.sh"' "$PUSH" >/dev/null
# shellcheck disable=SC2016
grep -F '"$remote_tmp/zao-remote.sh" install "$package" "$remote_tmp"' "$PUSH" >/dev/null

echo 'standalone Zao deployment cut tests passed'
