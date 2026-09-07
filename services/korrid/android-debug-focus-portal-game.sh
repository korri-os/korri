#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash android-tools coreutils curl gnugrep gnused jq websocat
# shellcheck shell=bash
# Debug-device helper for focusing exact elements in the bundled Shift portal.
# Same-URL overlay pages receive only inert Shell-interface classification;
# focus/probe expressions run only in the uniquely classified main Shell. It
# never clicks, dispatches input, invokes native functions, or reaches RPC.
# Release WebViews expose no inspector.
# shellcheck disable=SC2016 # jq/JavaScript variables are intentionally quoted.
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  android-debug-focus-portal-game.sh <adb-serial> <package> --library [devtools-port]
  android-debug-focus-portal-game.sh <adb-serial> <package> --verify-library [devtools-port]
  android-debug-focus-portal-game.sh <adb-serial> <package> --game <game-id> <exact-title> [devtools-port]
  android-debug-focus-portal-game.sh <adb-serial> <package> --detail-play [devtools-port]
  android-debug-focus-portal-game.sh <adb-serial> <package> --launch-location <location-id> <exact-game-title> [devtools-port]
USAGE
  exit 2
}

[[ $# -ge 3 && $# -le 6 ]] || usage
serial="$1"
package="$2"
mode="$3"
shift 3
game_id=''
location_id=''
title=''
case "$mode" in
  --library | --verify-library)
    [[ $# -le 1 ]] || usage
    ;;
  --game)
    [[ $# -ge 2 && $# -le 3 ]] || usage
    game_id="$1"
    title="$2"
    shift 2
    [[ "$game_id" =~ ^[A-Za-z0-9._:@/-]+$ && -n "$title" ]] || usage
    ;;
  --detail-play)
    [[ $# -le 1 ]] || usage
    game_id='local-game:01K4J6K8Y00000000000000002'
    title='Wario Land 4'
    ;;
  --launch-location)
    [[ $# -ge 2 && $# -le 3 ]] || usage
    location_id="$1"
    title="$2"
    shift 2
    [[ -n "$location_id" && -n "$title" ]] || usage
    ;;
  *) usage ;;
esac
devtools_port="${1:-43123}"
[[ "$serial" =~ ^[A-Za-z0-9._:-]+$ ]] || usage
[[ "$package" =~ ^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$ ]] || usage
[[ "$devtools_port" =~ ^[0-9]+$ && "$devtools_port" -ge 1024 && "$devtools_port" -le 65535 ]] || usage

readonly TRUSTED_PORTAL_URL='https://appassets.androidplatform.net/assets/portal/index.html'
ADB_BIN="${KORRI_ADB_BIN:-$(command -v adb)}"
CURL_BIN="${KORRI_CURL_BIN:-$(command -v curl)}"
JQ_BIN="${KORRI_JQ_BIN:-$(command -v jq)}"
# The sourced portal-target transport reads these configured executables.
# shellcheck disable=SC2034
WEBSOCAT_BIN="${KORRI_WEBSOCAT_BIN:-$(command -v websocat)}"
# shellcheck disable=SC2034
TIMEOUT_BIN="${KORRI_TIMEOUT_BIN:-$(command -v timeout)}"
ADB=("$ADB_BIN" -s "$serial")
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=services/korrid/android-debug-portal-target.sh
source "$SCRIPT_DIR/android-debug-portal-target.sh"

pid="$("${ADB[@]}" shell pidof "$package" | tr -d '\r\n')"
[[ "$pid" =~ ^[0-9]+$ ]] || {
  echo 'Korri process is missing or ambiguous' >&2
  exit 1
}
"${ADB[@]}" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
"${ADB[@]}" forward "tcp:$devtools_port" "localabstract:webview_devtools_remote_$pid" >/dev/null
cleanup() {
  "${ADB[@]}" forward --remove "tcp:$devtools_port" >/dev/null 2>&1 || true
}
trap cleanup EXIT

json_targets() {
  "$CURL_BIN" --fail --silent --show-error --connect-timeout 2 --max-time 5 \
    "http://127.0.0.1:$devtools_port/json"
}

url_js="$("$JQ_BIN" -Rn --arg value "$TRUSTED_PORTAL_URL" '$value')"
game_id_js="$("$JQ_BIN" -Rn --arg value "$game_id" '$value')"
location_id_js="$("$JQ_BIN" -Rn --arg value "$location_id" '$value')"
title_js="$("$JQ_BIN" -Rn --arg value "$title" '$value')"
common_js="
  const expectedUrl = $url_js;
  if (location.href !== expectedUrl) throw new Error('not the trusted main portal');
  const renderedFocusable = (element) => {
    const style = getComputedStyle(element);
    return element instanceof HTMLElement && !element.hasAttribute('disabled')
      && element.tabIndex >= 0 && element.getClientRects().length > 0
      && style.display !== 'none' && style.visibility !== 'hidden';
  };"
case "$mode" in
  --library)
    expression="(() => {$common_js
      const selector = 'button[data-korri-part=\"shift.cine-library-tile\"][aria-label=\"Library\"]';
      const matches = Array.from(document.querySelectorAll(selector)).filter(renderedFocusable);
      if (matches.length !== 1) throw new Error('expected exactly one visible focusable Library tile');
      const target = matches[0];
      target.focus();
      const rect = target.getBoundingClientRect();
      const rectValues = [rect.left, rect.top, rect.width, rect.height, rect.right, rect.bottom];
      const rectFinitePositive = rectValues.every(Number.isFinite)
        && rect.width > 0 && rect.height > 0;
      const fullyOnScreen = rectFinitePositive
        && rect.left >= 0 && rect.top >= 0
        && rect.right <= window.innerWidth && rect.bottom <= window.innerHeight;
      return {
        href: location.href,
        view: 'home',
        part: target.getAttribute('data-korri-part'),
        title: target.getAttribute('aria-label'),
        visibleFocusableMatches: matches.length,
        activeExact: document.activeElement === target
          && document.activeElement.matches(selector),
        bounds: {left: rect.left, top: rect.top, width: rect.width, height: rect.height},
        viewport: {width: window.innerWidth, height: window.innerHeight},
        rectFinitePositive,
        fullyOnScreen
      };
    })()"
    predicate='.href == $url and .view == "home"
      and .part == "shift.cine-library-tile" and .title == "Library"
      and .visibleFocusableMatches == 1 and .activeExact == true
      and .rectFinitePositive == true and .fullyOnScreen == true
      and (.bounds | type == "object") and (.viewport | type == "object")
      and (.bounds.left | type == "number") and (.bounds.top | type == "number")
      and (.bounds.width | type == "number") and .bounds.width > 0
      and (.bounds.height | type == "number") and .bounds.height > 0
      and (.viewport.width | type == "number") and .viewport.width > 0
      and (.viewport.height | type == "number") and .viewport.height > 0
      and .bounds.left >= 0 and .bounds.top >= 0
      and (.bounds.left + .bounds.width) <= .viewport.width
      and (.bounds.top + .bounds.height) <= .viewport.height'
    failure='trusted main portal did not expose exactly one visible focusable Library tile'
    ;;
  --verify-library)
    expression="(() => {$common_js
      const matches = Array.from(document.querySelectorAll('[data-shift-library]'))
        .filter((element) => element instanceof HTMLElement
          && element.getClientRects().length > 0
          && getComputedStyle(element).display !== 'none'
          && getComputedStyle(element).visibility !== 'hidden');
      return {href: location.href, view: 'library', visibleLibraryRoots: matches.length};
    })()"
    predicate='.href == $url and .view == "library" and .visibleLibraryRoots == 1'
    failure='trusted main portal did not expose exactly one visible Shift Library view'
    ;;
  --game)
    expression="(() => {$common_js
      const gameId = $game_id_js;
      const title = $title_js;
      if (document.querySelectorAll('[data-shift-library]').length !== 1) {
        throw new Error('Shift Library is not the current installed view');
      }
      const renderedIdMatches = Array.from(document.querySelectorAll('[data-shift-game-id]'))
        .filter(renderedFocusable)
        .filter((element) => element.getAttribute('data-shift-game-id') === gameId);
      const exactMatches = renderedIdMatches
        .filter((element) => element.getAttribute('aria-label') === title);
      if (renderedIdMatches.length !== 1 || exactMatches.length !== 1) {
        throw new Error('expected exactly one rendered focusable game with exact identity');
      }
      const target = exactMatches[0];
      target.focus();
      const rect = target.getBoundingClientRect();
      const rectValues = [rect.left, rect.top, rect.width, rect.height, rect.right, rect.bottom];
      const rectFinitePositive = rectValues.every(Number.isFinite)
        && rect.width > 0 && rect.height > 0;
      const fullyOnScreen = rectFinitePositive
        && rect.left >= 0 && rect.top >= 0
        && rect.right <= window.innerWidth && rect.bottom <= window.innerHeight;
      return {
        href: location.href,
        view: 'library',
        gameId: target.getAttribute('data-shift-game-id'),
        title: target.getAttribute('aria-label'),
        renderedIdMatches: renderedIdMatches.length,
        exactMatches: exactMatches.length,
        activeExact: document.activeElement === target
          && document.activeElement.getAttribute('data-shift-game-id') === gameId
          && document.activeElement.getAttribute('aria-label') === title,
        bounds: {left: rect.left, top: rect.top, width: rect.width, height: rect.height},
        viewport: {width: window.innerWidth, height: window.innerHeight},
        rectFinitePositive,
        fullyOnScreen
      };
    })()"
    predicate='.href == $url and .view == "library" and .gameId == $gameId and .title == $title
      and .renderedIdMatches == 1 and .exactMatches == 1 and .activeExact == true
      and .rectFinitePositive == true and .fullyOnScreen == true
      and (.bounds | type == "object") and (.viewport | type == "object")
      and (.bounds.left | type == "number") and (.bounds.top | type == "number")
      and (.bounds.width | type == "number") and .bounds.width > 0
      and (.bounds.height | type == "number") and .bounds.height > 0
      and (.viewport.width | type == "number") and .viewport.width > 0
      and (.viewport.height | type == "number") and .viewport.height > 0
      and .bounds.left >= 0 and .bounds.top >= 0
      and (.bounds.left + .bounds.width) <= .viewport.width
      and (.bounds.top + .bounds.height) <= .viewport.height'
    failure='trusted main portal Library did not expose exactly one focusable game with the expected identity'
    ;;
  --detail-play)
    expression="(() => {$common_js
      const gameId = $game_id_js;
      const title = $title_js;
      const rendered = (element) => {
        const style = getComputedStyle(element);
        return element instanceof HTMLElement && element.getClientRects().length > 0
          && style.display !== 'none' && style.visibility !== 'hidden';
      };
      const detailRoots = Array.from(document.querySelectorAll('[data-shift-detail]'))
        .filter(rendered);
      const exactRoots = detailRoots.filter((element) =>
        element.getAttribute('data-shift-detail-game-id') === gameId);
      if (detailRoots.length !== 1 || exactRoots.length !== 1) {
        throw new Error('expected one visible detail with exact game identity');
      }
      const root = exactRoots[0];
      const visibleTitles = Array.from(root.querySelectorAll('[data-korri-part=\"shift.detail-title\"]'))
        .filter(rendered);
      const exactTitles = visibleTitles.filter((element) => element.textContent?.trim() === title);
      if (visibleTitles.length !== 1 || exactTitles.length !== 1) {
        throw new Error('expected one exact visible detail title');
      }
      const candidates = Array.from(root.querySelectorAll('button.shift-detail-btn.primary'))
        .filter(renderedFocusable);
      const labelled = candidates.map((element) => {
        const accessible = element.getAttribute('aria-label')?.trim()
          || element.textContent?.trim() || '';
        const label = accessible.replace(/^[▶▷►]\s*/, '');
        return {element, label};
      }).filter(({label}) => label === 'Play' || label === 'Continue');
      if (candidates.length !== 1 || labelled.length !== 1) {
        throw new Error('expected one visible focusable primary Play or Continue action');
      }
      const target = labelled[0].element;
      const label = labelled[0].label;
      target.focus();
      const rect = target.getBoundingClientRect();
      const rectValues = [rect.left, rect.top, rect.width, rect.height, rect.right, rect.bottom];
      const rectFinitePositive = rectValues.every(Number.isFinite)
        && rect.width > 0 && rect.height > 0;
      const fullyOnScreen = rectFinitePositive
        && rect.left >= 0 && rect.top >= 0
        && rect.right <= window.innerWidth && rect.bottom <= window.innerHeight;
      return {
        href: location.href,
        view: 'detail',
        gameId: root.getAttribute('data-shift-detail-game-id'),
        title: exactTitles[0].textContent?.trim(),
        label,
        visibleDetailRoots: detailRoots.length,
        exactDetailRoots: exactRoots.length,
        visibleTitles: visibleTitles.length,
        exactTitles: exactTitles.length,
        primaryCandidates: candidates.length,
        exactActions: labelled.length,
        activeExact: document.activeElement === target
          && target.matches('button.shift-detail-btn.primary'),
        bounds: {left: rect.left, top: rect.top, width: rect.width, height: rect.height},
        viewport: {width: window.innerWidth, height: window.innerHeight},
        rectFinitePositive,
        fullyOnScreen
      };
    })()"
    predicate='.href == $url and .view == "detail" and .gameId == $gameId and .title == $title
      and (.label == "Play" or .label == "Continue")
      and .visibleDetailRoots == 1 and .exactDetailRoots == 1
      and .visibleTitles == 1 and .exactTitles == 1
      and .primaryCandidates == 1 and .exactActions == 1 and .activeExact == true
      and .rectFinitePositive == true and .fullyOnScreen == true
      and (.bounds | type == "object") and (.viewport | type == "object")
      and (.bounds.left | type == "number") and (.bounds.top | type == "number")
      and (.bounds.width | type == "number") and .bounds.width > 0
      and (.bounds.height | type == "number") and .bounds.height > 0
      and (.viewport.width | type == "number") and .viewport.width > 0
      and (.viewport.height | type == "number") and .viewport.height > 0
      and .bounds.left >= 0 and .bounds.top >= 0
      and (.bounds.left + .bounds.width) <= .viewport.width
      and (.bounds.top + .bounds.height) <= .viewport.height'
    failure='trusted main portal did not expose exact Wario detail with one focusable Play action'
    ;;
  --launch-location)
    expression="(() => {$common_js
      const locationId = $location_id_js;
      const title = $title_js;
      const rendered = (element) => {
        const style = getComputedStyle(element);
        return element instanceof HTMLElement && element.getClientRects().length > 0
          && style.display !== 'none' && style.visibility !== 'hidden';
      };
      const expectedDialogLabel = 'Choose where to play ' + title;
      const dialogs = Array.from(document.querySelectorAll('[role=\"dialog\"]'))
        .filter(rendered);
      const exactDialogs = dialogs.filter((element) =>
        element.getAttribute('aria-label') === expectedDialogLabel);
      if (dialogs.length !== 1 || exactDialogs.length !== 1) {
        throw new Error('expected one exact visible launch-location chooser');
      }
      const dialog = exactDialogs[0];
      const visibleTitles = Array.from(dialog.querySelectorAll('.shift-sheet-title'))
        .filter(rendered);
      const exactTitles = visibleTitles.filter((element) => element.textContent?.trim() === title);
      if (visibleTitles.length !== 1 || exactTitles.length !== 1) {
        throw new Error('expected one exact launch-location chooser title');
      }
      const rows = Array.from(dialog.querySelectorAll('button[data-launch-location-id]'))
        .filter(renderedFocusable)
        .filter((element) => element.getAttribute('aria-disabled') !== 'true');
      const identityMatches = rows.filter((element) =>
        element.getAttribute('data-launch-location-id') === locationId);
      if (identityMatches.length !== 1) {
        throw new Error('expected exactly one enabled focusable launch-location identity');
      }
      const target = identityMatches[0];
      const label = target.getAttribute('aria-label')?.trim() || '';
      if (label.length === 0) throw new Error('launch-location row has no accessible label');
      target.focus();
      const rect = target.getBoundingClientRect();
      const rectValues = [rect.left, rect.top, rect.width, rect.height, rect.right, rect.bottom];
      const rectFinitePositive = rectValues.every(Number.isFinite)
        && rect.width > 0 && rect.height > 0;
      const fullyOnScreen = rectFinitePositive
        && rect.left >= 0 && rect.top >= 0
        && rect.right <= window.innerWidth && rect.bottom <= window.innerHeight;
      return {
        href: location.href,
        view: 'launch-location',
        dialogLabel: dialog.getAttribute('aria-label'),
        title: exactTitles[0].textContent?.trim(),
        locationId: target.getAttribute('data-launch-location-id'),
        label,
        visibleDialogs: dialogs.length,
        exactDialogs: exactDialogs.length,
        visibleTitles: visibleTitles.length,
        exactTitles: exactTitles.length,
        enabledFocusableRows: rows.length,
        identityMatches: identityMatches.length,
        activeExact: document.activeElement === target
          && target.getAttribute('data-launch-location-id') === locationId,
        bounds: {left: rect.left, top: rect.top, width: rect.width, height: rect.height},
        viewport: {width: window.innerWidth, height: window.innerHeight},
        rectFinitePositive,
        fullyOnScreen
      };
    })()"
    predicate='.href == $url and .view == "launch-location"
      and .dialogLabel == ("Choose where to play " + $title) and .title == $title
      and .locationId == $locationId and (.label | type == "string") and (.label | length > 0)
      and .visibleDialogs == 1 and .exactDialogs == 1
      and .visibleTitles == 1 and .exactTitles == 1
      and .enabledFocusableRows >= 1 and .identityMatches == 1 and .activeExact == true
      and .rectFinitePositive == true and .fullyOnScreen == true
      and (.bounds | type == "object") and (.viewport | type == "object")
      and (.bounds.left | type == "number") and (.bounds.top | type == "number")
      and (.bounds.width | type == "number") and .bounds.width > 0
      and (.bounds.height | type == "number") and .bounds.height > 0
      and (.viewport.width | type == "number") and .viewport.width > 0
      and (.viewport.height | type == "number") and .viewport.height > 0
      and .bounds.left >= 0 and .bounds.top >= 0
      and (.bounds.left + .bounds.width) <= .viewport.width
      and (.bounds.top + .bounds.height) <= .viewport.height'
    failure='trusted main portal did not expose exact Wario chooser with one focusable local identity'
    ;;
esac

verified=''
for _ in $(seq 1 80); do
  if targets="$(json_targets 2>/dev/null)" \
    && socket="$(korri_debug_select_main_portal_socket "$targets" 2>/dev/null)" \
    && candidate="$(korri_debug_evaluate "$socket" "$expression" 2>/dev/null)" \
    && "$JQ_BIN" -e --arg url "$TRUSTED_PORTAL_URL" --arg gameId "$game_id" \
      --arg locationId "$location_id" --arg title "$title" \
      "$predicate" <<<"$candidate" >/dev/null; then
    verified="$candidate"
    break
  fi
  sleep 0.25
done
[[ -n "$verified" ]] || {
  echo "$failure" >&2
  exit 1
}

case "$mode" in
  --library)
    "$JQ_BIN" -c \
      '{url:.href,view,part,title,focused:.activeExact,bounds,viewport,
        rectFinitePositive,fullyOnScreen}' <<<"$verified"
    ;;
  --verify-library)
    "$JQ_BIN" -cn --arg url "$TRUSTED_PORTAL_URL" \
      '{url:$url,view:"library",verified:true}'
    ;;
  --game)
    "$JQ_BIN" -c \
      '{url:.href,view,gameId,title,focused:.activeExact,bounds,viewport,
        rectFinitePositive,fullyOnScreen}' <<<"$verified"
    ;;
  --detail-play)
    "$JQ_BIN" -c \
      '{url:.href,view,gameId,title,label,focused:.activeExact,bounds,viewport,
        rectFinitePositive,fullyOnScreen}' <<<"$verified"
    ;;
  --launch-location)
    "$JQ_BIN" -c \
      '{url:.href,view,dialogLabel,title,locationId,label,focused:.activeExact,
        enabledFocusableRows,bounds,viewport,rectFinitePositive,fullyOnScreen}' \
      <<<"$verified"
    ;;
esac
