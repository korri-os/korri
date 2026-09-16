#!/usr/bin/env bash
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
HELPER="$ROOT/services/korrid/android-debug-launch-local.sh"
TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

cat >"$TMP/adb" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FAKE_ADB_LOG"
case "$*" in
  *' shell pidof com.simonwjackson.korri.debug')
    case "${FAKE_PIDOF_BEHAVIOR:-ok}" in
      ok) printf '4242\n' ;;
      missing) echo 'no pid for package' >&2; exit 1 ;;
      ambiguous) printf '4242\n4243\n' ;;
      *) exit 31 ;;
    esac
    ;;
  *' forward tcp:43120 localabstract:webview_devtools_remote_4242')
    if [[ "${FAKE_FORWARD_BEHAVIOR:-ok}" == fail ]]; then
      echo 'cannot bind requested forward' >&2
      exit 1
    fi
    ;;
  *' forward --remove tcp:43120')
    if [[ "${FAKE_REMOVE_BEHAVIOR:-ok}" == fail ]]; then
      echo 'cannot remove requested forward' >&2
      exit 1
    fi
    ;;
esac
SH
cat >"$TMP/curl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
cat "$FAKE_TARGETS"
SH
cat >"$TMP/timeout" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FAKE_TIMEOUT_LOG"
seconds="$1"
shift
case "${FAKE_TIMEOUT_BEHAVIOR:-ok}" in
  pidof)
    if [[ "$*" == *' shell pidof com.simonwjackson.korri.debug' ]]; then exit 124; fi
    ;;
  forward)
    if [[ "$*" == *' forward tcp:43120 localabstract:webview_devtools_remote_4242' ]]; then exit 124; fi
    ;;
  remove)
    if [[ "$*" == *' forward --remove tcp:43120' ]]; then exit 124; fi
    ;;
esac
exec "$@"
SH
cat >"$TMP/websocat" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
request="$(cat)"
socket="${!#}"
expression="$(jq -r '.params.expression' <<<"$request")"
jq -cn --arg socket "$socket" --arg expression "$expression" \
  '{socket:$socket,expression:$expression}' >>"$FAKE_EVAL_LOG"
if grep -Fq 'hasCapability:' <<<"$expression"; then
  case "$socket" in
    */main|*/shell-a|*/shell-b)
      value='{"exactPortal":true,"hasNative":true,"hasPort":true,"hasCapability":true}' ;;
    */overlay)
      value='{"exactPortal":true,"hasNative":false,"hasPort":false,"hasCapability":false}' ;;
    *) exit 13 ;;
  esac
elif grep -Fq 'hasLaunchLocal:' <<<"$expression"; then
  [[ "$socket" == */main ]] || exit 14
  value="$(jq -cn \
    --argjson port "${FAKE_SIGNER_PORT:-43211}" \
    --arg capability "${FAKE_SIGNER_CAPABILITY:-$FAKE_EXPECTED_CAPABILITY}" \
    '{href:"https://appassets.androidplatform.net/assets/portal/index.html",exactPortal:true,readyState:"complete",hasNative:true,hasPort:true,hasCapability:true,hasLaunchLocal:true,port:$port,capability:$capability}')"
elif grep -Fq 'setTimeout(() =>' <<<"$expression" && grep -Fq 'launchLocal(specJson)' <<<"$expression"; then
  encoded="$(awk -F'"' '/const encodedLaunchSpec = /{print $2; exit}' <<<"$expression")"
  printf '%s\n' "$socket" >>"$FAKE_LAUNCH_LOG"
  printf '%s' "$encoded" >>"$FAKE_LAUNCH_BASE64_LOG"
  printf '%s' "$encoded" | base64 -d >"$FAKE_LAUNCH_SPEC_LOG"
  case "${FAKE_LAUNCH_ACK:-success}" in
    success) value='{"_tag":"LaunchScheduled"}' ;;
    bad) value='{"_tag":"Launched"}' ;;
    empty) exit 0 ;;
    *) exit 15 ;;
  esac
else
  exit 16
fi
jq -cn --argjson value "$value" '{id:1,result:{result:{value:$value}}}'
SH
chmod +x "$TMP/adb" "$TMP/curl" "$TMP/timeout" "$TMP/websocat"

export FAKE_ADB_LOG="$TMP/adb.log"
export FAKE_TIMEOUT_LOG="$TMP/timeout.log"
export FAKE_EVAL_LOG="$TMP/eval.log"
export FAKE_LAUNCH_LOG="$TMP/launch.log"
export FAKE_LAUNCH_BASE64_LOG="$TMP/launch.base64.log"
export FAKE_LAUNCH_SPEC_LOG="$TMP/launch-spec.json"
export FAKE_TARGETS="$TMP/targets.json"
export KORRI_ADB_BIN="$TMP/adb"
export KORRI_CURL_BIN="$TMP/curl"
export KORRI_WEBSOCAT_BIN="$TMP/websocat"
export KORRI_TIMEOUT_BIN="$TMP/timeout"
KORRI_JQ_BIN="$(command -v jq)"
export KORRI_JQ_BIN
trusted='https://appassets.androidplatform.net/assets/portal/index.html'
secret_capability='bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'
secret_spec='{"launchId":"launch-1","runnerId":"retroarch","integrity":"transport-secret","component":{"packageName":"com.korri.retroarch"}}'
export FAKE_EXPECTED_CAPABILITY="$secret_capability"

set_targets() {
  case "$1" in
    shell-overlay)
      jq -cn --arg url "$trusted" \
        '[{type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43120/devtools/page/main"},
          {type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43120/devtools/page/overlay"}]' \
        >"$FAKE_TARGETS"
      ;;
    ambiguous)
      jq -cn --arg url "$trusted" \
        '[{type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43120/devtools/page/shell-a"},
          {type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43120/devtools/page/shell-b"},
          {type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43120/devtools/page/overlay"}]' \
        >"$FAKE_TARGETS"
      ;;
  esac
}

reset_logs() {
  : >"$FAKE_ADB_LOG"
  : >"$FAKE_TIMEOUT_LOG"
  : >"$FAKE_EVAL_LOG"
  : >"$FAKE_LAUNCH_LOG"
  : >"$FAKE_LAUNCH_BASE64_LOG"
  : >"$FAKE_LAUNCH_SPEC_LOG"
  unset FAKE_PIDOF_BEHAVIOR FAKE_FORWARD_BEHAVIOR FAKE_REMOVE_BEHAVIOR FAKE_TIMEOUT_BEHAVIOR \
    FAKE_SIGNER_PORT FAKE_SIGNER_CAPABILITY FAKE_LAUNCH_ACK || true
}

launch_count() {
  if [[ -s "$FAKE_LAUNCH_LOG" ]]; then
    wc -l <"$FAKE_LAUNCH_LOG" | tr -d ' '
  else
    printf '0'
  fi
}

assert_one_main_launch() {
  [[ "$(launch_count)" == 1 ]]
  grep -Fx 'ws://127.0.0.1:43120/devtools/page/main' "$FAKE_LAUNCH_LOG" >/dev/null
  [[ "$(grep -o 'launchLocal(specJson)' "$FAKE_EVAL_LOG" | wc -l | tr -d ' ')" == 1 ]]
  jq -r '.expression' "$FAKE_EVAL_LOG" | grep -F 'const expectedPort = 43211;' >/dev/null
  jq -r '.expression' "$FAKE_EVAL_LOG" | grep -F "const expectedCapability = \"$secret_capability\";" >/dev/null
  [[ "$(jq -r '.expression' "$FAKE_EVAL_LOG" | grep -Fo 'signerMatches()' | wc -l | tr -d ' ')" -ge 2 ]]
}

envelope() {
  {
    printf '%s\n' "$secret_spec"
    printf '%s\n' '43211'
    printf '%s' "$secret_capability"
  } | jq -Rsc '
    split("\n") as $parts
    | {expectedSigner:{port:($parts[1] | tonumber),capability:$parts[2]},spec:($parts[0] | fromjson)}
  '
}

run_helper() {
  envelope | "$HELPER" fake-device com.simonwjackson.korri.debug >"$1" 2>"$2"
}

set_targets shell-overlay
reset_logs
run_helper "$TMP/success.out" "$TMP/success.err"
jq -e '. == {"_tag":"LaunchScheduled"}' "$TMP/success.out" >/dev/null
assert_one_main_launch
jq -e --argjson expected "$secret_spec" '. == $expected' "$FAKE_LAUNCH_SPEC_LOG" >/dev/null
if jq -e 'select((.socket | endswith("/overlay")) and (.expression | contains("launchLocal") or contains("hasLaunchLocal:")))' \
  "$FAKE_EVAL_LOG" >/dev/null; then
  echo 'launchLocal helper evaluated readiness or mutation on the same-URL overlay target' >&2
  exit 1
fi
if grep -F 'transport-secret' "$FAKE_ADB_LOG" "$FAKE_TIMEOUT_LOG" "$TMP/success.out" "$TMP/success.err" >/dev/null; then
  echo 'launchLocal helper leaked the stdin launch spec through argv or output' >&2
  exit 1
fi
if grep -F "$secret_capability" "$FAKE_ADB_LOG" "$FAKE_TIMEOUT_LOG" "$TMP/success.out" "$TMP/success.err" >/dev/null; then
  echo 'launchLocal helper leaked expected signer capability through argv or output' >&2
  exit 1
fi
if grep -F "$(cat "$FAKE_LAUNCH_BASE64_LOG")" "$FAKE_ADB_LOG" "$FAKE_TIMEOUT_LOG" "$TMP/success.out" "$TMP/success.err" >/dev/null; then
  echo 'launchLocal helper leaked base64 transport through argv or output' >&2
  exit 1
fi

set_targets shell-overlay
reset_logs
export FAKE_SIGNER_CAPABILITY=cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc
if run_helper "$TMP/mismatch.out" "$TMP/mismatch.err"; then
  echo 'launchLocal helper accepted a signer capability mismatch' >&2
  exit 1
fi
[[ "$(launch_count)" == 0 ]]
grep -F 'expected signer authority' "$TMP/mismatch.err" >/dev/null

set_targets ambiguous
reset_logs
if run_helper "$TMP/ambiguous.out" "$TMP/ambiguous.err"; then
  echo 'launchLocal helper accepted ambiguous trusted Shell targets' >&2
  exit 1
fi
[[ "$(launch_count)" == 0 ]]

set_targets shell-overlay
reset_logs
export FAKE_LAUNCH_ACK=empty
if run_helper "$TMP/empty.out" "$TMP/empty.err"; then
  echo 'launchLocal helper accepted a lost one-shot schedule ack' >&2
  exit 1
fi
assert_one_main_launch
grep -F 'not retrying' "$TMP/empty.err" >/dev/null
unset FAKE_LAUNCH_ACK

set_targets shell-overlay
reset_logs
export FAKE_LAUNCH_ACK=bad
if run_helper "$TMP/bad-ack.out" "$TMP/bad-ack.err"; then
  echo 'launchLocal helper accepted an invalid schedule ack' >&2
  exit 1
fi
assert_one_main_launch
grep -F 'invalid LaunchScheduled ack' "$TMP/bad-ack.err" >/dev/null
unset FAKE_LAUNCH_ACK

set_targets shell-overlay
reset_logs
export FAKE_PIDOF_BEHAVIOR=missing
if run_helper "$TMP/pidof-missing.out" "$TMP/pidof-missing.err"; then
  echo 'launchLocal helper accepted a missing Korri process' >&2
  exit 1
fi
grep -F 'Korri process is missing or ambiguous' "$TMP/pidof-missing.err" >/dev/null
grep -F 'pidof stderr: no pid for package' "$TMP/pidof-missing.err" >/dev/null
[[ "$(launch_count)" == 0 ]]
unset FAKE_PIDOF_BEHAVIOR

set_targets shell-overlay
reset_logs
export FAKE_PIDOF_BEHAVIOR=ambiguous
if run_helper "$TMP/pidof-ambiguous.out" "$TMP/pidof-ambiguous.err"; then
  echo 'launchLocal helper accepted multiple Korri processes' >&2
  exit 1
fi
grep -F 'Korri process is missing or ambiguous' "$TMP/pidof-ambiguous.err" >/dev/null
[[ "$(launch_count)" == 0 ]]
unset FAKE_PIDOF_BEHAVIOR

set_targets shell-overlay
reset_logs
export FAKE_TIMEOUT_BEHAVIOR=pidof
if run_helper "$TMP/pidof-timeout.out" "$TMP/pidof-timeout.err"; then
  echo 'launchLocal helper accepted a hanging pidof' >&2
  exit 1
fi
grep -F '10 ' "$FAKE_TIMEOUT_LOG" >/dev/null
grep -F 'Korri process is missing or ambiguous' "$TMP/pidof-timeout.err" >/dev/null
[[ "$(launch_count)" == 0 ]]
unset FAKE_TIMEOUT_BEHAVIOR

set_targets shell-overlay
reset_logs
export FAKE_FORWARD_BEHAVIOR=fail
if run_helper "$TMP/forward-fail.out" "$TMP/forward-fail.err"; then
  echo 'launchLocal helper accepted a failed DevTools forward' >&2
  exit 1
fi
grep -F 'failed to acquire trusted portal DevTools forward' "$TMP/forward-fail.err" >/dev/null
grep -F 'forward stderr: cannot bind requested forward' "$TMP/forward-fail.err" >/dev/null
[[ "$(launch_count)" == 0 ]]
unset FAKE_FORWARD_BEHAVIOR

set_targets shell-overlay
reset_logs
export FAKE_TIMEOUT_BEHAVIOR=forward
if run_helper "$TMP/forward-timeout.out" "$TMP/forward-timeout.err"; then
  echo 'launchLocal helper accepted a hanging DevTools forward' >&2
  exit 1
fi
grep -F 'failed to acquire trusted portal DevTools forward' "$TMP/forward-timeout.err" >/dev/null
[[ "$(launch_count)" == 0 ]]
unset FAKE_TIMEOUT_BEHAVIOR

set_targets shell-overlay
reset_logs
export FAKE_REMOVE_BEHAVIOR=fail
if run_helper "$TMP/remove-fail.out" "$TMP/remove-fail.err"; then
  echo 'launchLocal helper ignored cleanup-forward failure after success' >&2
  exit 1
fi
assert_one_main_launch
grep -F 'failed to remove trusted portal DevTools forward during cleanup' "$TMP/remove-fail.err" >/dev/null
unset FAKE_REMOVE_BEHAVIOR

printf 'Android debug launchLocal helper contract passed\n'
