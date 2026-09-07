#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash coreutils curl gnugrep gnused jq
# shellcheck shell=bash
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
HELPER="$ROOT/services/korrid/android-debug-focus-portal-game.sh"
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
cat >"$TMP/websocat" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
request="$(cat)"
printf '%s\n' "$request" >>"$FAKE_EVAL_LOG"
socket="${!#}"
id="$(jq -r '.id' <<<"$request")"
expression="$(jq -r '.params.expression' <<<"$request")"
jq -cn --arg socket "$socket" --arg expression "$expression" \
  '{socket:$socket,expression:$expression}' >>"$FAKE_TARGET_EVAL_LOG"
if grep -Fq 'hasCapability:' <<<"$expression"; then
  case "$socket" in
    */main|*/shell-a|*/shell-b) value='{"exactPortal":true,"hasNative":true,"hasPort":true,"hasCapability":true}' ;;
    */overlay)
      [[ "${FAKE_OVERLAY_CLASSIFICATION:-ok}" == ok ]] || exit 12
      value='{"exactPortal":true,"hasNative":false,"hasPort":false,"hasCapability":false}' ;;
    *) exit 13 ;;
  esac
elif [[ "$socket" != */main ]]; then
  echo 'focus evaluation reached non-shell target' >&2
  exit 14
elif grep -Fq 'shift.cine-library-tile' <<<"$expression"; then
  case "${FAKE_LIBRARY_RESULT:-ok}" in
    ok) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"home","part":"shift.cine-library-tile","title":"Library","visibleFocusableMatches":1,"activeExact":true,"bounds":{"left":407.25,"top":352.5,"width":56,"height":80},"viewport":{"width":640,"height":480},"rectFinitePositive":true,"fullyOnScreen":true}' ;;
    ambiguous) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"home","part":"shift.cine-library-tile","title":"Library","visibleFocusableMatches":2,"activeExact":false,"bounds":{"left":0,"top":0,"width":0,"height":0},"viewport":{"width":640,"height":480},"rectFinitePositive":false,"fullyOnScreen":false}' ;;
    offscreen) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"home","part":"shift.cine-library-tile","title":"Library","visibleFocusableMatches":1,"activeExact":true,"bounds":{"left":620,"top":352,"width":56,"height":80},"viewport":{"width":640,"height":480},"rectFinitePositive":true,"fullyOnScreen":false}' ;;
    *) exit 2 ;;
  esac
elif grep -Fq 'visibleLibraryRoots' <<<"$expression"; then
  value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"library","visibleLibraryRoots":1}'
elif grep -Fq 'enabledFocusableRows' <<<"$expression"; then
  case "${FAKE_LOCATION_RESULT:-ok}" in
    ok) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"launch-location","dialogLabel":"Choose where to play Wario Land 4","title":"Wario Land 4","locationId":"[\"local\",null,\"01K4J6K8Y00000000000000002\"]","label":"This device","visibleDialogs":1,"exactDialogs":1,"visibleTitles":1,"exactTitles":1,"enabledFocusableRows":2,"identityMatches":1,"activeExact":true,"bounds":{"left":329.5,"top":191.25,"width":280,"height":52},"viewport":{"width":640,"height":480},"rectFinitePositive":true,"fullyOnScreen":true}' ;;
    ambiguous) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"launch-location","dialogLabel":"Choose where to play Wario Land 4","title":"Wario Land 4","locationId":"[\"local\",null,\"01K4J6K8Y00000000000000002\"]","label":"This device","visibleDialogs":1,"exactDialogs":1,"visibleTitles":1,"exactTitles":1,"enabledFocusableRows":3,"identityMatches":2,"activeExact":false,"bounds":{"left":0,"top":0,"width":0,"height":0},"viewport":{"width":640,"height":480},"rectFinitePositive":false,"fullyOnScreen":false}' ;;
    wrong-dialog) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"launch-location","dialogLabel":"Choose where to play Other Game","title":"Other Game","locationId":"[\"local\",null,\"01K4J6K8Y00000000000000002\"]","label":"This device","visibleDialogs":1,"exactDialogs":0,"visibleTitles":1,"exactTitles":0,"enabledFocusableRows":2,"identityMatches":1,"activeExact":true,"bounds":{"left":329,"top":191,"width":280,"height":52},"viewport":{"width":640,"height":480},"rectFinitePositive":true,"fullyOnScreen":true}' ;;
    disabled) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"launch-location","dialogLabel":"Choose where to play Wario Land 4","title":"Wario Land 4","locationId":"[\"local\",null,\"01K4J6K8Y00000000000000002\"]","label":"This device","visibleDialogs":1,"exactDialogs":1,"visibleTitles":1,"exactTitles":1,"enabledFocusableRows":1,"identityMatches":0,"activeExact":false,"bounds":{"left":0,"top":0,"width":0,"height":0},"viewport":{"width":640,"height":480},"rectFinitePositive":false,"fullyOnScreen":false}' ;;
    *) exit 2 ;;
  esac
elif grep -Fq 'visibleDetailRoots' <<<"$expression"; then
  case "${FAKE_DETAIL_RESULT:-ok}" in
    ok) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"detail","gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","label":"Play","visibleDetailRoots":1,"exactDetailRoots":1,"visibleTitles":1,"exactTitles":1,"primaryCandidates":1,"exactActions":1,"activeExact":true,"bounds":{"left":342.5,"top":287.25,"width":130,"height":52},"viewport":{"width":640,"height":480},"rectFinitePositive":true,"fullyOnScreen":true}' ;;
    ambiguous) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"detail","gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","label":"Play","visibleDetailRoots":1,"exactDetailRoots":1,"visibleTitles":1,"exactTitles":1,"primaryCandidates":2,"exactActions":2,"activeExact":false,"bounds":{"left":0,"top":0,"width":0,"height":0},"viewport":{"width":640,"height":480},"rectFinitePositive":false,"fullyOnScreen":false}' ;;
    wrong-game) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"detail","gameId":"game:aka:wario","title":"Wario Land 4","label":"Play","visibleDetailRoots":1,"exactDetailRoots":0,"visibleTitles":1,"exactTitles":1,"primaryCandidates":1,"exactActions":1,"activeExact":true,"bounds":{"left":342,"top":287,"width":130,"height":52},"viewport":{"width":640,"height":480},"rectFinitePositive":true,"fullyOnScreen":true}' ;;
    offscreen) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"detail","gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","label":"Continue","visibleDetailRoots":1,"exactDetailRoots":1,"visibleTitles":1,"exactTitles":1,"primaryCandidates":1,"exactActions":1,"activeExact":true,"bounds":{"left":600,"top":287,"width":130,"height":52},"viewport":{"width":640,"height":480},"rectFinitePositive":true,"fullyOnScreen":false}' ;;
    *) exit 2 ;;
  esac
else
  case "${FAKE_GAME_RESULT:-ok}" in
    ok) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"library","gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","renderedIdMatches":1,"exactMatches":1,"activeExact":true,"bounds":{"left":548.5,"top":230.25,"width":82,"height":120},"viewport":{"width":640,"height":480},"rectFinitePositive":true,"fullyOnScreen":true}' ;;
    ambiguous) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"library","gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","renderedIdMatches":2,"exactMatches":2,"activeExact":false,"bounds":{"left":0,"top":0,"width":0,"height":0},"viewport":{"width":640,"height":480},"rectFinitePositive":false,"fullyOnScreen":false}' ;;
    offscreen) value='{"href":"https://appassets.androidplatform.net/assets/portal/index.html","view":"library","gameId":"local-game:01K4J6K8Y00000000000000002","title":"Wario Land 4","renderedIdMatches":1,"exactMatches":1,"activeExact":true,"bounds":{"left":620,"top":230,"width":82,"height":120},"viewport":{"width":640,"height":480},"rectFinitePositive":true,"fullyOnScreen":false}' ;;
    *) exit 2 ;;
  esac
fi
jq -cn --argjson id "$id" --argjson value "$value" '{id:$id,result:{result:{value:$value}}}'
SH
chmod +x "$TMP/adb" "$TMP/curl" "$TMP/timeout" "$TMP/websocat"

export FAKE_ADB_LOG="$TMP/adb.log"
export FAKE_EVAL_LOG="$TMP/eval.log"
export FAKE_TARGET_EVAL_LOG="$TMP/target-eval.log"
export FAKE_TARGETS="$TMP/targets.json"
export KORRI_ADB_BIN="$TMP/adb"
export KORRI_CURL_BIN="$TMP/curl"
export KORRI_WEBSOCAT_BIN="$TMP/websocat"
export KORRI_TIMEOUT_BIN="$TMP/timeout"
KORRI_JQ_BIN="$(command -v jq)"
export KORRI_JQ_BIN

trusted='https://appassets.androidplatform.net/assets/portal/index.html'
trusted_targets() {
  jq -cn --arg url "$trusted" \
    '[{type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43123/devtools/page/main"},
      {type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43123/devtools/page/overlay"},
      {type:"page",url:"https://example.invalid/",webSocketDebuggerUrl:"ws://127.0.0.1:43123/devtools/page/external"}]' \
    >"$FAKE_TARGETS"
}
trusted_targets

library="$($HELPER fake-device com.simonwjackson.korri.debug --library)"
jq -e --arg url "$trusted" '
  .url == $url and .view == "home" and .part == "shift.cine-library-tile"
  and .title == "Library" and .focused == true
  and .bounds == {left:407.25,top:352.5,width:56,height:80}
  and .viewport == {width:640,height:480}
  and .rectFinitePositive == true and .fullyOnScreen == true
' <<<"$library" >/dev/null
view="$($HELPER fake-device com.simonwjackson.korri.debug --verify-library)"
jq -e --arg url "$trusted" '.url == $url and .view == "library" and .verified == true' <<<"$view" >/dev/null
game="$($HELPER fake-device com.simonwjackson.korri.debug --game local-game:01K4J6K8Y00000000000000002 'Wario Land 4')"
jq -e --arg url "$trusted" '
  .url == $url and .view == "library" and .gameId == "local-game:01K4J6K8Y00000000000000002"
  and .title == "Wario Land 4" and .focused == true
  and .bounds == {left:548.5,top:230.25,width:82,height:120}
  and .viewport == {width:640,height:480}
  and .rectFinitePositive == true and .fullyOnScreen == true
' <<<"$game" >/dev/null
detail="$($HELPER fake-device com.simonwjackson.korri.debug --detail-play)"
jq -e --arg url "$trusted" '
  .url == $url and .view == "detail" and .gameId == "local-game:01K4J6K8Y00000000000000002"
  and .title == "Wario Land 4" and .label == "Play" and .focused == true
  and .bounds == {left:342.5,top:287.25,width:130,height:52}
  and .viewport == {width:640,height:480}
  and .rectFinitePositive == true and .fullyOnScreen == true
' <<<"$detail" >/dev/null
location_id='["local",null,"01K4J6K8Y00000000000000002"]'
location="$($HELPER fake-device com.simonwjackson.korri.debug \
  --launch-location "$location_id" 'Wario Land 4')"
jq -e --arg url "$trusted" --arg locationId "$location_id" '
  .url == $url and .view == "launch-location"
  and .dialogLabel == "Choose where to play Wario Land 4"
  and .title == "Wario Land 4" and .locationId == $locationId
  and .label == "This device" and .focused == true
  and .enabledFocusableRows == 2
  and .bounds == {left:329.5,top:191.25,width:280,height:52}
  and .viewport == {width:640,height:480}
  and .rectFinitePositive == true and .fullyOnScreen == true
' <<<"$location" >/dev/null

grep -F 'forward tcp:43123 localabstract:webview_devtools_remote_4242' "$FAKE_ADB_LOG" >/dev/null
grep -F 'forward --remove tcp:43123' "$FAKE_ADB_LOG" >/dev/null
grep -F 'button[data-korri-part=\"shift.cine-library-tile\"][aria-label=\"Library\"]' "$FAKE_EVAL_LOG" >/dev/null
grep -F "document.querySelectorAll('[data-shift-library]').length !== 1" "$FAKE_EVAL_LOG" >/dev/null
grep -F 'target.focus()' "$FAKE_EVAL_LOG" >/dev/null
grep -F 'target.getBoundingClientRect()' "$FAKE_EVAL_LOG" >/dev/null
grep -F 'rectFinitePositive' "$FAKE_EVAL_LOG" >/dev/null
grep -F 'fullyOnScreen' "$FAKE_EVAL_LOG" >/dev/null
grep -F "element.getAttribute('data-shift-detail-game-id') === gameId" "$FAKE_EVAL_LOG" >/dev/null
grep -F "root.querySelectorAll('button.shift-detail-btn.primary')" "$FAKE_EVAL_LOG" >/dev/null
grep -F "label === 'Play' || label === 'Continue'" "$FAKE_EVAL_LOG" >/dev/null
grep -F "button[data-launch-location-id]" "$FAKE_EVAL_LOG" >/dev/null
grep -F "element.getAttribute('data-launch-location-id') === locationId" "$FAKE_EVAL_LOG" >/dev/null
grep -F "element.getAttribute('aria-label') === expectedDialogLabel" "$FAKE_EVAL_LOG" >/dev/null
for forbidden in '.click(' 'dispatchEvent(' 'fetch(' XMLHttpRequest example.invalid; do
  if grep -F "$forbidden" "$FAKE_EVAL_LOG" >/dev/null; then
    echo "debug focus evaluated forbidden content: $forbidden" >&2
    exit 1
  fi
done
if jq -e 'select((.socket | endswith("/overlay")) and (.expression | contains("hasCapability:") | not))' \
  "$FAKE_TARGET_EVAL_LOG" >/dev/null; then
  echo 'focus evaluation reached same-URL overlay beyond inert classification' >&2
  exit 1
fi

export FAKE_LIBRARY_RESULT=ambiguous
if "$HELPER" fake-device com.simonwjackson.korri.debug --library >"$TMP/library.out" 2>"$TMP/library.err"; then
  echo 'debug focus helper accepted ambiguous Library tiles' >&2
  exit 1
fi
grep -F 'exactly one visible focusable Library tile' "$TMP/library.err" >/dev/null
export FAKE_LIBRARY_RESULT=offscreen
if "$HELPER" fake-device com.simonwjackson.korri.debug --library \
  >"$TMP/library-offscreen.out" 2>"$TMP/library-offscreen.err"; then
  echo 'debug focus helper accepted an off-screen Library tile' >&2
  exit 1
fi
grep -F 'did not expose exactly one visible focusable Library tile' \
  "$TMP/library-offscreen.err" >/dev/null
unset FAKE_LIBRARY_RESULT

export FAKE_GAME_RESULT=ambiguous
if "$HELPER" fake-device com.simonwjackson.korri.debug --game local-game:01K4J6K8Y00000000000000002 'Wario Land 4' \
  >"$TMP/ambiguous.out" 2>"$TMP/ambiguous.err"; then
  echo 'debug focus helper accepted an ambiguous rendered identity' >&2
  exit 1
fi
grep -F 'did not expose exactly one focusable game' "$TMP/ambiguous.err" >/dev/null
unset FAKE_GAME_RESULT

export FAKE_GAME_RESULT=offscreen
if "$HELPER" fake-device com.simonwjackson.korri.debug --game local-game:01K4J6K8Y00000000000000002 'Wario Land 4' \
  >"$TMP/offscreen.out" 2>"$TMP/offscreen.err"; then
  echo 'debug focus helper accepted an off-screen focused game' >&2
  exit 1
fi
grep -F 'did not expose exactly one focusable game' "$TMP/offscreen.err" >/dev/null
unset FAKE_GAME_RESULT

for detail_failure in ambiguous wrong-game offscreen; do
  export FAKE_DETAIL_RESULT="$detail_failure"
  if "$HELPER" fake-device com.simonwjackson.korri.debug --detail-play \
    >"$TMP/detail-$detail_failure.out" 2>"$TMP/detail-$detail_failure.err"; then
    echo "debug focus helper accepted invalid detail Play state: $detail_failure" >&2
    exit 1
  fi
  grep -F 'exact Wario detail with one focusable Play action' \
    "$TMP/detail-$detail_failure.err" >/dev/null
done
unset FAKE_DETAIL_RESULT

for location_failure in ambiguous wrong-dialog disabled; do
  export FAKE_LOCATION_RESULT="$location_failure"
  if "$HELPER" fake-device com.simonwjackson.korri.debug \
    --launch-location "$location_id" 'Wario Land 4' \
    >"$TMP/location-$location_failure.out" 2>"$TMP/location-$location_failure.err"; then
    echo "debug focus helper accepted invalid launch-location state: $location_failure" >&2
    exit 1
  fi
  grep -F 'exact Wario chooser with one focusable local identity' \
    "$TMP/location-$location_failure.err" >/dev/null
done
unset FAKE_LOCATION_RESULT

if "$HELPER" fake-device com.simonwjackson.korri.debug \
  --detail-play game:other 'Other Game' >"$TMP/detail-broad.out" 2>"$TMP/detail-broad.err"; then
  echo 'debug focus helper allowed detail Play identity broadening' >&2
  exit 1
fi
grep -F 'usage:' "$TMP/detail-broad.err" >/dev/null

jq -cn --arg url "$trusted" \
  '[{type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43123/devtools/page/overlay"}]' \
  >"$FAKE_TARGETS"
if "$HELPER" fake-device com.simonwjackson.korri.debug --library >"$TMP/overlay.out" 2>"$TMP/overlay.err"; then
  echo 'debug focus helper accepted an overlay-only target set' >&2
  exit 1
fi
grep -F 'did not expose exactly one visible focusable Library tile' "$TMP/overlay.err" >/dev/null

jq -cn --arg url "$trusted" \
  '[{type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43123/devtools/page/a"},
    {type:"page",url:$url,webSocketDebuggerUrl:"ws://127.0.0.1:43123/devtools/page/b"}]' \
  >"$FAKE_TARGETS"
if "$HELPER" fake-device com.simonwjackson.korri.debug --game local-game:01K4J6K8Y00000000000000002 'Wario Land 4' \
  >"$TMP/duplicate.out" 2>"$TMP/duplicate.err"; then
  echo 'debug focus helper accepted duplicate trusted portal targets' >&2
  exit 1
fi
grep -F 'did not expose exactly one focusable game' "$TMP/duplicate.err" >/dev/null

printf 'Android debug portal focus contract passed\n'
