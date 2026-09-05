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
  '--user stop korrid.service') printf inactive >"$HARNESS_STATE" ;;
  '--user is-active --quiet korrid.service') [[ "$(cat "$HARNESS_STATE")" == active ]] ;;
  '--user list-units --type=service --state=activating,active,reloading,deactivating --no-legend --plain korri-game-*.service')
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
[[ "${calls[0]}" == '--user stop korrid.service' ]]
[[ "${calls[1]}" == '--user is-active --quiet korrid.service' ]]
[[ "${calls[2]}" == '--user list-units --type=service --state=activating,active,reloading,deactivating --no-legend --plain korri-game-*.service' ]]

printf '%s' bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb >"$session_root/launch-id"
chmod 0600 "$session_root/launch-id"
printf active >"$TMP/state"
if run_cut "$home" "$TMP/log" "$TMP/state" yes; then
  echo 'standalone cut accepted a live game unit' >&2
  exit 1
fi
[[ -f "$session_root/launch-id" ]]

trap_line="$(grep -n '^[[:space:]]*trap rollback_install ERR$' "$REMOTE" | cut -d: -f1)"
cut_line="$(grep -n '^[[:space:]]*quiesce_and_cut_obsolete_session$' "$REMOTE" | cut -d: -f1)"
mutation_line="$(grep -n '^[[:space:]]*ln -sfn "\$profile"' "$REMOTE" | cut -d: -f1)"
[[ "$trap_line" =~ ^[0-9]+$ && "$cut_line" =~ ^[0-9]+$ && "$mutation_line" =~ ^[0-9]+$ ]]
[[ "$trap_line" -lt "$cut_line" && "$cut_line" -lt "$mutation_line" ]]
grep -F 'systemctl --user is-active --quiet korrid.service && previous_service_active=true' "$REMOTE" >/dev/null
grep -F 'if [[ "$previous_service_active" == true ]]; then' "$REMOTE" >/dev/null
grep -F 'systemctl --user restart korrid.service || true' "$REMOTE" >/dev/null
# The documented caller must ship and execute the reviewed remote installer.
grep -F '"$root/services/korrid/deploy/zao-remote.sh"' "$PUSH" >/dev/null
grep -F '"zao:$remote_tmp/zao-remote.sh"' "$PUSH" >/dev/null
grep -F '"$remote_tmp/zao-remote.sh" install "$package" "$remote_tmp"' "$PUSH" >/dev/null

echo 'standalone Zao deployment cut tests passed'
