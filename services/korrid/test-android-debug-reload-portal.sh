#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash coreutils curl gnugrep gnused jq
# shellcheck shell=bash
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
HELPER="$ROOT/services/korrid/android-debug-reload-portal.sh"
TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

cat >"$TMP/adb" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FAKE_ADB_LOG"
if [[ "$*" == *' shell pidof com.simonwjackson.korri.debug' ]]; then printf '4242\n'; fi
SH
cat >"$TMP/curl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
cat "$FAKE_TARGETS"
SH
cat >"$TMP/timeout" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
shift
exec "$@"
SH
cat >"$TMP/sleep" <<'SH'
#!/usr/bin/env bash
# Keep intentionally rejected fake-probe cases fast.
exit 0
SH
cat >"$TMP/websocat" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
request="$(cat)"
socket="${!#}"
id="$(jq -r '.id' <<<"$request")"
expression="$(jq -r '.params.expression' <<<"$request")"
jq -cn --arg socket "$socket" --arg expression "$expression" \
  '{socket:$socket,expression:$expression}' >>"$FAKE_EVAL_LOG"
if grep -Fq 'hasCapability:' <<<"$expression"; then
  case "$socket" in
    */main|*/shell-a|*/shell-b) value='{"exactPortal":true,"hasNative":true,"hasPort":true,"hasCapability":true}' ;;
    */overlay)
      [[ "${FAKE_OVERLAY_CLASSIFICATION:-ok}" == ok ]] || exit 12
      value='{"exactPortal":true,"hasNative":false,"hasPort":false,"hasCapability":false}' ;;
    *) exit 13 ;;
  esac
elif [[ "$socket" != */main ]]; then
  echo 'reload/probe evaluation reached non-shell target' >&2
  exit 14
elif grep -Fq 'setTimeout(() => location.reload(), 100)' <<<"$expression"; then
  value='{"requested":true,"href":"https://appassets.androidplatform.net/assets/portal/index.html","markerSet":true}'
elif grep -Fq 'gameMatches' <<<"$expression"; then
  if [[ "${FAKE_RELOAD_MARKER_PRESENT:-false}" == true ]]; then
    marker_present=true
  else
    marker_present=false
  fi
  value="{\"href\":\"https://appassets.androidplatform.net/assets/portal/index.html\",\"exactPortal\":true,\"readyState\":\"complete\",\"reloadMarkerPresent\":$marker_present,\"navigationType\":\"reload\",\"mounted\":true,\"homeMatches\":1,\"libraryMatches\":0,\"loadError\":false,\"gameMatches\":1}"
else
  value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","exactPortal":true,"readyState":"complete"}'
fi
jq -cn --argjson id "$id" --argjson value "$value" '{id:$id,result:{result:{value:$value}}}'
SH
chmod +x "$TMP/adb" "$TMP/curl" "$TMP/timeout" "$TMP/sleep" "$TMP/websocat"
export PATH="$TMP:$PATH"

export FAKE_ADB_LOG="$TMP/adb.log"
export FAKE_EVAL_LOG="$TMP/eval.log"
export FAKE_TARGETS="$TMP/targets.json"
export KORRI_ADB_BIN="$TMP/adb"
export KORRI_CURL_BIN="$TMP/curl"
export KORRI_WEBSOCAT_BIN="$TMP/websocat"
export KORRI_TIMEOUT_BIN="$TMP/timeout"
KORRI_JQ_BIN="$(command -v jq)"
export KORRI_JQ_BIN
trusted='https://appassets.androidplatform.net/assets/portal/index.html'

same_url_shell_and_overlay() {
  jq -cn --arg url "$trusted" \
    '[{type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43121/devtools/page/main"},
      {type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43121/devtools/page/overlay"},
      {type:"page",url:"https://example.invalid/",webSocketDebuggerUrl:"ws://127.0.0.1:43121/devtools/page/external"}]' \
    >"$FAKE_TARGETS"
}
same_url_shell_and_overlay

output="$($HELPER fake-device com.simonwjackson.korri.debug --expect-game 01K4J6K8Y00000000000000002 'Wario Land 4')"
jq -e --arg url "$trusted" '
  .url == $url and .mode == "--expect-game" and .gameId == "01K4J6K8Y00000000000000002"
  and .title == "Wario Land 4" and .reloaded == true
' <<<"$output" >/dev/null
if jq -e 'select((.socket | endswith("/overlay")) and (.expression | contains("hasCapability:") | not))' \
  "$FAKE_EVAL_LOG" >/dev/null; then
  echo 'reload/probe evaluation reached same-URL overlay target' >&2
  exit 1
fi
main_expressions="$(jq -r 'select(.socket | endswith("/main")) | .expression' "$FAKE_EVAL_LOG")"
grep -F 'setTimeout(() => location.reload(), 100)' <<<"$main_expressions" >/dev/null
grep -F 'Object.defineProperty(globalThis, markerKey' <<<"$main_expressions" >/dev/null
grep -F 'reloadMarkerPresent: Object.prototype.hasOwnProperty.call(globalThis, markerKey)' \
  <<<"$main_expressions" >/dev/null
marker_one="$(grep -Eo '__korriReloadProbe_[0-9a-f]{32}' <<<"$main_expressions" | sort -u)"
[[ "$marker_one" =~ ^__korriReloadProbe_[0-9a-f]{32}$ ]] || {
  echo 'reload helper did not use exactly one host-random marker key' >&2
  exit 1
}
if grep -Fq 'timeOrigin' "$HELPER"; then
  echo 'reload helper still relies on performance.timeOrigin' >&2
  exit 1
fi
grep -F 'forward --remove tcp:43121' "$FAKE_ADB_LOG" >/dev/null

: >"$FAKE_EVAL_LOG"
output="$($HELPER fake-device com.simonwjackson.korri.debug --expect-portal)"
jq -e '.mode == "--expect-portal" and .gameId == "" and .reloaded == true' <<<"$output" >/dev/null
marker_two="$(jq -r 'select(.socket | endswith("/main")) | .expression' "$FAKE_EVAL_LOG" \
  | grep -Eo '__korriReloadProbe_[0-9a-f]{32}' | sort -u)"
[[ "$marker_two" =~ ^__korriReloadProbe_[0-9a-f]{32}$ && "$marker_two" != "$marker_one" ]] || {
  echo 'reload helper reused its invocation marker key' >&2
  exit 1
}
if jq -e 'select((.socket | endswith("/overlay")) and (.expression | contains("hasCapability:") | not))' \
  "$FAKE_EVAL_LOG" >/dev/null; then
  echo 'generic reload evaluated overlay target beyond classification' >&2
  exit 1
fi

export FAKE_RELOAD_MARKER_PRESENT=true
if "$HELPER" fake-device com.simonwjackson.korri.debug --expect-portal \
  >"$TMP/marker-present.out" 2>"$TMP/marker-present.err"; then
  echo 'reload helper accepted a document that retained its old Window marker' >&2
  exit 1
fi
unset FAKE_RELOAD_MARKER_PRESENT

jq -cn --arg url "$trusted" \
  '[{type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43121/devtools/page/shell-a"},
    {type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43121/devtools/page/shell-b"},
    {type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43121/devtools/page/overlay"}]' \
  >"$FAKE_TARGETS"
if "$HELPER" fake-device com.simonwjackson.korri.debug --expect-portal >"$TMP/duplicate.out" 2>"$TMP/duplicate.err"; then
  echo 'reload helper accepted two classified Shell targets' >&2
  exit 1
fi

jq -cn --arg url "$trusted" \
  '[{type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43121/devtools/page/overlay"}]' \
  >"$FAKE_TARGETS"
if "$HELPER" fake-device com.simonwjackson.korri.debug --expect-portal >"$TMP/overlay.out" 2>"$TMP/overlay.err"; then
  echo 'reload helper accepted overlay-only targets' >&2
  exit 1
fi

jq -cn '[{type:"page",url:"https://example.invalid/",webSocketDebuggerUrl:"ws://127.0.0.1:43121/devtools/page/main"}]' \
  >"$FAKE_TARGETS"
if "$HELPER" fake-device com.simonwjackson.korri.debug --expect-portal >"$TMP/url.out" 2>"$TMP/url.err"; then
  echo 'reload helper accepted a wrong-URL target' >&2
  exit 1
fi

same_url_shell_and_overlay
export FAKE_OVERLAY_CLASSIFICATION=error
if "$HELPER" fake-device com.simonwjackson.korri.debug --expect-portal >"$TMP/error.out" 2>"$TMP/error.err"; then
  echo 'reload helper ignored overlay classification failure' >&2
  exit 1
fi
unset FAKE_OVERLAY_CLASSIFICATION

printf 'Android debug portal reload contract passed\n'
