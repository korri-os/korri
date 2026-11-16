#!/usr/bin/env bash
# Tests of d12-binding.sh make (#434 D12) with no phone and no real signer:
# nak's own NIP-46 bunker, holding a throwaway owner key, on nak's own relay
# on a free loopback port, in a temporary folder that the test removes. Run
# it on zao through its Nix app, which gives it the binding's tools:
#
#   nix run .#android-app-d12-binding-check
#
# Each case prints PASS or FAIL; the test exits non-zero when one fails.
set -uo pipefail

: "${KORRID:?run through nix run .#android-app-d12-binding-check}"
: "${KORRI_APP_BUILD_TOOLS:?run through nix run .#android-app-d12-binding-check}"
binding="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/d12-binding.sh"
scratch="$(mktemp -d)"
pids=()
cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2> /dev/null; done
  rm -rf -- "$scratch"
}
trap cleanup EXIT

failures=0
pass() { printf 'PASS %s\n' "$1"; }
failed() {
  printf 'FAIL %s\n' "$1"
  failures=$((failures + 1))
}

# The relay and the bunker.
port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
nak serve --hostname 127.0.0.1 --port "$port" > "$scratch/relay.log" 2>&1 &
pids+=($!)
for _ in $(seq 50); do (: > "/dev/tcp/127.0.0.1/$port") 2> /dev/null && break; sleep 0.1; done
owner_key="$(nak key generate)"
owner="$(nak key public "$owner_key")"
secret="d12-binding-test-$RANDOM$RANDOM"
nak bunker --sec "$owner_key" --authorized-secrets "$secret" "ws://127.0.0.1:$port" > "$scratch/bunker.log" 2>&1 &
pids+=($!)
for _ in $(seq 50); do grep -q 'bunker://' "$scratch/bunker.log" && break; sleep 0.1; done
grep -q 'bunker://' "$scratch/bunker.log" || { echo "nak's bunker did not start" >&2; exit 1; }
relay="ws%3A%2F%2F127.0.0.1%3A$port"
umask 077
echo "bunker://$owner?relay=$relay&secret=$secret" > "$scratch/bunker.uri"
echo "bunker://$owner?relay=$relay&secret=wrong-$secret" > "$scratch/wrong.uri"
chmod 600 "$scratch/bunker.uri" "$scratch/wrong.uri"

# The folders that make must never touch.
tree="$scratch/tree"
mkdir -p "$tree/keep" "$tree/sub"
echo keep > "$tree/keep/marker"
echo sub > "$tree/sub/marker"
ln -s keep "$tree/link"
ln -s missing "$tree/dangling"
# Names and kinds of every entry, and the markers' content. Folder sizes
# change when entries come and go, so they are left out.
snapshot() {
  (cd "$tree" && find . -printf '%p %y\n' | sort && cat keep/marker sub/marker)
}
before="$(snapshot)"
unchanged() { [[ "$(snapshot | grep -v -E "^\./($1)( |/)")" == "$before" ]]; }

refused() {
  local name="$1" directory="$2" work="$3" output
  if output="$(cd "$directory" && bash "$binding" make "$work" --owner "$owner" --bunker-file "$scratch/bunker.uri" 2>&1)"; then
    failed "$name: make succeeded: $(tail -n 1 <<< "$output")"
  elif [[ "$(snapshot)" != "$before" ]]; then
    failed "$name: refused, but the folders changed"
  else
    pass "$name: refused and deleted nothing ($(tail -n 1 <<< "$output"))"
  fi
}
refused "nope/../keep" "$tree" nope/../keep
refused "x/.., run in sub" "$tree/sub" x/..
refused "., run in sub" "$tree/sub" .
refused "an existing folder" "$tree" keep
refused "a link to an existing folder" "$tree" link
refused "a link with a trailing slash" "$tree" link/
refused "a link that points nowhere" "$tree" dangling
refused "a folder whose parent is missing" "$tree" nope/new
refused "an absolute nope/../keep" / "$tree/nope/../keep"

# A new folder binds, through the bunker.
if output="$(cd "$tree" && bash "$binding" make fresh --owner "$owner" --bunker-file "$scratch/bunker.uri" 2>&1)" &&
  [[ "$(stat -c %a "$tree/fresh/state/identity/device.key")" == 600 ]] &&
  [[ -s "$tree/fresh/state/identity/owner.event.json" ]] && unchanged fresh; then
  pass "a new folder: bound over NIP-46, and nothing else changed"
else
  failed "a new folder: $(tail -n 1 <<< "$output")"
fi
if output="$(cd "$tree" && bash "$binding" make fresh2 --owner "$owner" --bunker-file "$scratch/bunker.uri" 2>&1)"; then
  pass "a second new folder: bound"
else
  failed "a second new folder: $(tail -n 1 <<< "$output")"
fi

# A failure after make made the folder removes that folder only.
if output="$(cd "$tree" && bash "$binding" make failing --bunker-file "$scratch/bunker.uri" 2>&1)"; then
  failed "a signer of another key than --owner: make succeeded"
elif [[ -e "$tree/failing" ]] || ! unchanged 'fresh|fresh2'; then
  failed "a signer of another key than --owner: refused, but the folders are wrong"
else
  pass "a signer of another key than --owner: refused; its new folder removed, nothing else"
fi

# A connection that the bunker refuses: nak's message, without the secret.
if output="$(cd "$tree" && bash "$binding" make refused --owner "$owner" --bunker-file "$scratch/wrong.uri" 2>&1)"; then
  failed "a refused connection: make succeeded"
elif grep -q -F "wrong-$secret" <<< "$output" || ! grep -q 'secret=REDACTED' <<< "$output"; then
  failed "a refused connection: the output shows the secret, or no nak message"
elif [[ -e "$tree/refused" ]]; then
  failed "a refused connection: its folder is still there"
else
  pass "a refused connection: refused, secret=REDACTED in nak's message, folder removed"
fi

# Each run reaches the bunker with a client key of its own, never nak's
# default for this machine.
default="$(nak key public "$(nak key default)")"
mapfile -t clients < <(python3 - "$scratch/bunker.log" <<'PY' | sort -u
import json, re, sys
log = open(sys.argv[1]).read()
for author, body in re.findall(r"request from '([0-9a-f]{64})': (\{.*?\n\})", log, re.DOTALL):
    if json.loads(body).get("method") == "sign_event":
        print(author)
PY
)
if [[ "${#clients[@]}" -ge 2 ]] && ! printf '%s\n' "${clients[@]}" | grep -q "$default"; then
  pass "client keys: ${#clients[@]} runs signed with ${#clients[@]} different keys, none nak's default"
else
  failed "client keys: ${clients[*]:-none} (nak's default is $default)"
fi

echo "failures: $failures"
[[ "$failures" -eq 0 ]]
