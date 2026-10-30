#!/usr/bin/env bash
# THROWAWAY SPIKE check for the SDL build (`--no-default-features --features
# webos`). Run through `nix run .#slint-spike-sdl-check`, which sets KORRI_ROOT,
# CARGO_TARGET_DIR, the toolchain, SDL_COMPAT_LIB, SDL_204_LIB and
# SDL_204_SOURCE.
set -euo pipefail
crate="$KORRI_ROOT/clients/slint-spike"
manifest="$crate/Cargo.toml"
features=(--no-default-features --features webos)
spike="$CARGO_TARGET_DIR/debug/korri-slint-spike"

scratch_root="$(mktemp -d)"
audit="$(mktemp -d)"

# X servers for the SDL runs. Not xvfb-run: it sleeps 3 s and then starts the
# command whether or not its Xvfb came up (seen in its source), which turns a
# lost display-number race into SDL's "No available video device". This
# script starts Xvfb with -displayfd, which reports the display only when the
# server accepts connections, and stops the app before the server.
#
# -noreset: SDL 2.0.4's X11_Available opens and closes a display, then
# X11_CreateDevice opens another. Closing the only client makes a default
# Xvfb reset (dix CloseDownClient -> DE_RESET), and the reset's
# KillAllClients drops a connection that arrived in the same dispatch turn,
# so SDL_Init fails with "x11 not available". Seen in 30 of 30 failed
# fixture runs (one completed connection, two disconnects in the -audit
# log); 0 of 60 failed with -noreset under the same interleaved load.
# -audit 4 logs each client connect and disconnect.
#
# A failed run keeps its logs under $KORRI_ROOT/.cache/slint-spike-sdl-check-failures/.
xvfb_pid=""
xvfb_count=0
app_pid=""
stop_app() {
  [[ -n "$app_pid" ]] || return 0
  kill -TERM "$app_pid" 2>/dev/null || true
  for _ in $(seq 300); do kill -0 "$app_pid" 2>/dev/null || break; sleep 0.05; done
  wait "$app_pid" 2>/dev/null || true
  app_pid=""
}
stop_xvfb() {
  [[ -n "$xvfb_pid" ]] || return 0
  kill "$xvfb_pid" 2>/dev/null || true
  wait "$xvfb_pid" 2>/dev/null || true
  xvfb_pid=""
}
cleanup() {
  local status=$?
  stop_app
  stop_xvfb
  if [[ $status -ne 0 ]]; then
    local kept
    kept="$KORRI_ROOT/.cache/slint-spike-sdl-check-failures/$(date -u +%Y%m%dT%H%M%SZ)-$$"
    mkdir -p "$kept"
    cp -R "$audit"/. "$kept"/
    echo "failed run: logs kept in $kept" >&2
  fi
  rm -rf "$scratch_root" "$audit"
}
trap cleanup EXIT

# start_xvfb WIDTHxHEIGHT: own Xvfb; exports DISPLAY once it accepts clients.
start_xvfb() {
  xvfb_count=$((xvfb_count + 1))
  local number_file="$audit/display-number"
  xvfb_log="$audit/xvfb-$xvfb_count.log"
  : >"$number_file"
  exec 3<>"$number_file"
  Xvfb -displayfd 3 -screen 0 "${1}x24" -nolisten tcp -noreset -audit 4 >"$xvfb_log" 2>&1 &
  xvfb_pid=$!
  exec 3>&-
  local display=""
  for _ in $(seq 200); do
    display="$(head -n1 "$number_file" 2>/dev/null || true)"
    [[ -n "$display" ]] && break
    if ! kill -0 "$xvfb_pid" 2>/dev/null; then
      echo "FAIL: Xvfb exited before it reported a display" >&2
      cat "$xvfb_log" >&2
      exit 1
    fi
    sleep 0.05
  done
  [[ -n "$display" ]] || { echo "FAIL: Xvfb reported no display in 10 s" >&2; exit 1; }
  export DISPLAY=":$display"
}

echo "== fmt"
cargo fmt --manifest-path "$manifest" --check
echo "== clippy (webos feature)"
cargo clippy --locked --manifest-path "$manifest" "${features[@]}" --all-targets -- -D warnings
echo "== unit tests (webos feature: state mapping, layout plan, key translation)"
cargo test --locked --manifest-path "$manifest" "${features[@]}"
echo "== build (webos feature)"
cargo build --locked --manifest-path "$manifest" "${features[@]}"

echo "== SDL 2.0.4 ABI: every bound function is in 2.0.4's export list"
tar -xzOf "$SDL_204_SOURCE" SDL2-2.0.4/src/dynapi/SDL_dynapi_procs.h \
  | grep -o 'SDL_DYNAPI_PROC([^,]*,SDL_[A-Za-z0-9_]*' | sed 's/.*,//' | sort -u >"$audit/exports"
sed -n '/^sdl_functions! {/,/^}/p' "$crate/src/sdl/ffi.rs" \
  | grep -o '^    SDL_[A-Za-z0-9_]*' | tr -d ' ' | sort -u >"$audit/bound"
missing="$(comm -23 "$audit/bound" "$audit/exports")"
if [[ -n "$missing" ]]; then
  echo "FAIL: not exported by SDL 2.0.4: $missing" >&2
  exit 1
fi
echo "  ok   $(wc -l <"$audit/bound") functions, all in SDL 2.0.4"

echo "== links no SDL, X11, fontconfig or Wayland library"
needed="$(readelf -d "$spike" | grep NEEDED | sed 's/.*\[\(.*\)\]/\1/' | sort | tr '\n' ' ')"
echo "  NEEDED: $needed"
if grep -Eq 'SDL|X11|xcb|fontconfig|wayland|freetype' <<<"$needed"; then
  echo "FAIL: the SDL build links a window-system or font library" >&2
  exit 1
fi

echo "== refuses an effective root user (user namespace, uid 0 mapped)"
set +e
refusal="$(TMPDIR="$scratch_root" unshare --user --map-root-user "$spike" --windowed=640x360 2>&1)"
status=$?
set -e
if [[ $status -ne 2 || "$refusal" != *"effective user is root"* ]]; then
  echo "FAIL: root was not refused ($status): $refusal" >&2
  exit 1
fi
echo "  ok   euid 0 refused before korrid or SDL starts"

echo "== launch parameter handling (never echoed)"
for bad in '[1,2]' '{"broken"' "{\"x\":\"$(head -c 17000 /dev/zero | tr '\0' a)\"}"; do
  set +e
  out="$(TMPDIR="$scratch_root" "$spike" "$bad" 2>&1)"
  status=$?
  set -e
  if [[ $status -ne 2 || "$out" == *broken* || "$out" == *aaaa* ]]; then
    echo "FAIL: bad launch parameter accepted or echoed ($status)" >&2
    exit 1
  fi
done
for bad in --windowed=10x10 --windowed=640 '--self-test --self-test' --unknown; do
  set +e
  # shellcheck disable=SC2086
  TMPDIR="$scratch_root" "$spike" $bad >/dev/null 2>&1
  status=$?
  set -e
  [[ $status -eq 2 ]] || { echo "FAIL: '$bad' was not a usage error" >&2; exit 1; }
done
echo "  ok   non-object, unterminated and oversized JSON refused; bad flags refused"

summarize='
  { family = "other" }
  /sa_family=AF_INET6/ { family = ($0 ~ /"::1"/) ? "AF_INET6 loopback" : "AF_INET6 OUTSIDE" }
  /sa_family=AF_INET[,}]/ { family = ($0 ~ /inet_addr\("127\.0\.0\.1"\)/) ? "AF_INET loopback" : "AF_INET OUTSIDE" }
  /sa_family=AF_UNIX/ { family = "AF_UNIX" }
  /sa_family=AF_NETLINK/ { family = "AF_NETLINK" }
  /connect\(/ { count[family]++ }
  END { for (f in count) print f, count[f] }'

self_test() {
  local label="$1" library_dir="$2"
  echo "== Xvfb self-test over SDL ($label): real korrid, Refresh callback, auth probes"
  rm -f "$audit/families"
  start_xvfb 1920x1080
  local run_log="$audit/self-test-$xvfb_count.log"
  # Foreground and bounded: the app exits on its own before the server stops.
  SDL_VIDEODRIVER=x11 TMPDIR="$scratch_root" \
    LD_LIBRARY_PATH="$library_dir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    timeout -k 10 180 \
    strace -f -qq -s 0 -e trace=connect -o "|awk '$summarize' > '$audit/families'" \
    "$spike" --self-test --windowed=1920x1080 | tee "$run_log"
  stop_xvfb
  grep -q '^SELF-TEST PASS$' "$run_log"
  grep -q '^sdl: runtime ' "$run_log"
  grep -q 'video driver x11;' "$run_log"
  grep -q 'screen saver allowed: true' "$run_log" \
    || { echo "FAIL: SDL suppressed the screen saver" >&2; exit 1; }
  sort "$audit/families" | sed 's/^/  /'
  if grep -q OUTSIDE "$audit/families"; then
    echo "FAIL: connect(2) to an IP address outside loopback" >&2
    exit 1
  fi
  echo "  ok   every IP connect(2) went to 127.0.0.1 or ::1"
}
self_test "upstream SDL 2.0.4" "$SDL_204_LIB"
self_test "nixpkgs sdl2-compat" "$SDL_COMPAT_LIB"

echo "== launch JSON object starts the normal window; SIGTERM shuts down cleanly"
log="$audit/launch.log"
start_xvfb 1280x720
SDL_VIDEODRIVER=x11 TMPDIR="$scratch_root" \
  LD_LIBRARY_PATH="$SDL_204_LIB${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
  "$spike" '{"secretish":"never-printed"}' >"$log" 2>&1 &
app_pid=$!
for _ in $(seq 400); do grep -q 'refresh 1 complete' "$log" && break; sleep 0.05; done
grep -q 'refresh 1 complete' "$log" || { cat "$log" >&2; echo "FAIL: no first refresh" >&2; exit 1; }
stop_app
stop_xvfb
grep -q 'sdl: display 0 desktop mode 1280x720' "$log"
grep -q 'launch parameter: JSON object with 1 field(s), ignored' "$log"
grep -q 'signal 15: closing' "$log"
grep -q 'korrid stopped; scratch storage removed' "$log"
if grep -q never-printed "$log"; then echo "FAIL: launch parameter echoed" >&2; exit 1; fi
echo "  ok   fullscreen window, first refresh, SIGTERM, scratch removed, value not printed"

echo "== scratch storage removed"
if compgen -G "$scratch_root/korri-slint-spike-*" >/dev/null; then
  echo "FAIL: this run's scratch directories remain" >&2
  exit 1
fi
echo "  ok   no korri-slint-spike-* directory remains"
echo "SLINT SPIKE SDL CHECK PASS"
