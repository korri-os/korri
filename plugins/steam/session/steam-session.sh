# Invoked once under dbus-run-session by the public steam-session command.
# All native artifact paths are substituted by packages/default.nix.
set -Eeuo pipefail
umask 077

say() { printf 'steam-session: %s\n' "$*" >&2; }
fail() { say "$2"; exit "$1"; }
trap 'fail 126 "Steam failed: session error at line $LINENO (status $?)"' ERR

appid="$1"
export STEAM_GAMES_ROOT="$STEAM_HOME"
export STEAM_DOT="$HOME/.steam"
export STEAM_BETA=steamdeck_stable
export FEX_ROOTFS=@rootfs@
export FEX_APP_CONFIG_LOCATION="$STEAM_HOME/fex-config/"
export FEX_APP_DATA_LOCATION="$STEAM_HOME/fex-data/"
export XDG_CURRENT_DESKTOP=sway
export LANG=C.UTF-8
# Do not reintroduce WAYLAND_DISPLAY, SWAYSOCK or the host's XDG_RUNTIME_DIR.
# Steam uses the game unit's Xwayland DISPLAY and its private D-Bus session.
flags=(-clientbeta steamdeck_stable -silent -nobigpicture -nochatui -nofriendsui -forcedesktopscaling 1.5)
steam_pid=""
console_log="$STEAM_HOME/logs/console_log.txt"
bootstrap_log="$STEAM_HOME/logs/bootstrap_log.txt"
console_mark=0
bootstrap_mark=0
new_console=""
new_bootstrap=""
generation=0
login_deadline=$((SECONDS + 600))

login_present() {
  local user
  for user in "$STEAM_HOME"/userdata/[0-9]*; do
    [[ -d "$user" && "${user##*/}" =~ ^[1-9][0-9]*$ ]] && return 0
  done
  return 1
}
check_login() {
  if ! login_present && (( SECONDS >= login_deadline )); then
    fail 126 'Steam sign-in required (no login after 600 s)'
  fi
}

# Read only new bytes. A truncated/replaced log starts a fresh cursor. Keeping
# independent cursors avoids accepting readiness or updates from a prior run.
read_log() {
  local path="$1" mark_name="$2" output_name="$3" size=0 text=""
  if [[ -f "$path" ]]; then
    size=$(stat -c %s "$path")
    if (( size < ${!mark_name} )); then printf -v "$mark_name" '%s' 0; fi
    text=$(tail -c +$(( ${!mark_name} + 1 )) "$path" 2>/dev/null || true)
  fi
  printf -v "$mark_name" '%s' "$size"
  printf -v "$output_name" '%s' "$text"
}
read_logs() {
  read_log "$console_log" console_mark new_console
  read_log "$bootstrap_log" bootstrap_mark new_bootstrap
}
mark_logs() {
  console_mark=0; bootstrap_mark=0
  # These cursors are passed by name to read_log above.
  # shellcheck disable=SC2034
  [[ ! -f "$console_log" ]] || console_mark=$(stat -c %s "$console_log")
  # shellcheck disable=SC2034
  [[ ! -f "$bootstrap_log" ]] || bootstrap_mark=$(stat -c %s "$bootstrap_log")
}
shutdown() {
  local deadline=$((SECONDS + 15))
  [[ -n "$steam_pid" ]] || return 0
  say 'end: stopping Steam'
  timeout 15 @fhs@/bin/steam-arm64-fhs -shutdown >/dev/null 2>&1 || true
  while kill -0 "$steam_pid" 2>/dev/null && (( SECONDS < deadline )); do sleep 1; done
  # The owning game unit has KillMode=control-group. Do not leave it waiting
  # indefinitely on a client that ignored -shutdown.
  kill "$steam_pid" 2>/dev/null || true
}
trap shutdown EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

start_steam() {
  say 'start: Steam desktop client'
  mark_logs
  @fhs@/bin/steam-arm64-fhs "${flags[@]}" >>"$STEAM_HOME/logs/session_log.txt" 2>&1 &
  steam_pid=$!
  generation=$((generation + 1))
}
check_client() {
  local status=0
  # PrivatePIDs scopes these probes to the owning game unit. Bracket spelling
  # keeps pgrep from matching its own command line.
  if pgrep -f '[s]teamwebhelper.* -uimode=4([[:space:]]|$)' >/dev/null; then
    fail 126 'Steam failed: gamepad UI (-uimode=4) is not allowed'
  fi
  if ! kill -0 "$steam_pid" 2>/dev/null; then
    wait "$steam_pid" || status=$?
    if (( status == 42 )); then
      say 'restart: Steam requested update restart (42)'
      start_steam
    else
      fail 126 "Steam failed: client exited with status $status"
    fi
  fi
}
wait_for_steam_ready() {
  local deadline=$((SECONDS + 600)) desktop_since=-1 desktop_pid="" pid=""
  local updating=0 ready_log=0 old_generation="$generation" update_seen=0
  say 'warm: waiting for Steam readiness'
  while (( SECONDS < deadline )); do
    check_login
    check_client
    if (( old_generation != generation )); then
      desktop_since=-1; desktop_pid=""; ready_log=0; updating=0
      old_generation="$generation"
    fi
    read_logs
    if grep -aqE 'Found pending update|Installing update\.\.\.|Extracting package' <<<"$new_console$new_bootstrap"; then
      updating=1
      desktop_since=-1; ready_log=0
      if (( update_seen == 0 )); then
        say 'warm: startup update observed; extending readiness deadline to 900 s'
        deadline=$((SECONDS + 900)); update_seen=1
      fi
    fi
    # A quiet extractor is still an update. Only completion or a new client
    # generation clears the disruptive stage, not an arbitrary quiet interval.
    if grep -aiq 'Update complete' <<<"$new_console$new_bootstrap"; then updating=0; fi
    if grep -aqE 'Waiting for compat in post-logon|Loaded Config for Local Selection Path for App ID 769' <<<"$new_console"; then
      ready_log=1
    fi
    pid=$(pgrep -f '[s]teamwebhelper.* -uimode=7([[:space:]]|$)' | head -n 1 || true)
    if [[ -n "$pid" && "$pid" == "$desktop_pid" ]] && (( updating == 0 )); then
      (( desktop_since >= 0 )) || desktop_since=$SECONDS
    else
      desktop_since=-1
    fi
    desktop_pid="$pid"
    if (( updating == 0 )) && login_present &&
      { (( ready_log == 1 )) || (( desktop_since >= 0 && SECONDS - desktop_since >= 5 )); }; then
      return 0
    fi
    sleep 1
  done
  check_login
  fail 126 'Steam failed: readiness timeout'
}
forward_appid() {
  say "launch: forwarding AppID $appid"
  timeout 15 @fhs@/bin/steam-arm64-fhs "${flags[@]}" -applaunch "$appid" \
    >>"$STEAM_HOME/logs/session_log.txt" 2>&1 || fail 126 'Steam failed: AppID forward failed or timed out'
}
app_process_present() {
  local environ
  for environ in /proc/[0-9]*/environ; do
    [[ -r "$environ" ]] || continue
    if grep -azqxF "SteamAppId=$appid" "$environ" 2>/dev/null; then return 0; fi
  done
  return 1
}

say 'seed: preparing ARM64 runtime and client'
mkdir -p "$STEAM_HOME/logs" "$STEAM_HOME/fex-config/AppConfig" "$STEAM_HOME/fex-data"
# A lock spans settings and the client lifetime. Never edit Steam-owned VDF
# concurrently with another launch of this session program.
exec 9>"$STEAM_HOME/.korri-session.lock"
flock -n 9 || fail 126 'Steam failed: another Steam session owns STEAM_HOME'
if pgrep -x steam >/dev/null; then fail 126 'Steam failed: client is already running before settings merge'; fi
# bwrap enters its caller's cwd before the guest helper can change directory.
cd "$STEAM_HOME"
if [[ ! -x "$STEAM_HOME/steamrtarm64/steam" ]]; then
  @helpers@/bin/steam-arm64-seed --apply >>"$STEAM_HOME/logs/session_log.txt" 2>&1 \
    || fail 126 'Steam failed: ARM64 seed failed (see logs/session_log.txt)'
fi
mkdir -p "$STEAM_HOME/compatibilitytools.d"
ln -sfn @proton@/share/korri/proton-cachyos-arm64/dist "$STEAM_HOME/compatibilitytools.d/proton-cachyos-11.0-20260601-slr-arm64"
cp @resources@/share/compatibilitytool.vdf "$STEAM_HOME/compatibilitytools.d/compatibilitytool.vdf"
for resource in Config.json AppConfig/steamwebhelper.json; do
  [[ -f "$STEAM_HOME/fex-config/$resource" ]] || cp "@resources@/share/fex-emu/$resource" "$STEAM_HOME/fex-config/$resource"
done
say 'settings: merging Proton, Steam Input and first-launch seeds'
@settings@/bin/steam-vdf-settings "$STEAM_HOME" "$appid"
if ! login_present; then say 'login: Steam sign-in required; waiting up to 600 s for manual sign-in'; fi
start_steam
wait_for_steam_ready
# Login can first happen during the warm gate. Apply userdata seeds only on
# the next launch: Steam is now running, and its files must not be edited.
mark_logs
forward_appid
launch_deadline=$((SECONDS + 180))
launch_generation="$generation"
reforwards=0
saw_added=0
saw_removed=0
say "watch: waiting for AppID $appid"
while true; do
  check_login
  check_client
  read_logs
  if grep -aqE "Game process added[[:space:]]*: AppID $appid([^0-9]|$)" <<<"$new_console"; then
    if (( saw_added == 0 )); then say "watch: AppID $appid started"; fi
    saw_added=1
  fi
  if grep -aqE "Game process removed[[:space:]]*: AppID $appid([^0-9]|$)" <<<"$new_console"; then
    saw_removed=1
  fi
  if (( saw_added == 1 && saw_removed == 1 )) && ! app_process_present; then
    say "end: AppID $appid exited"
    exit 0
  fi
  if (( saw_added == 0 && launch_generation != generation )); then
    (( reforwards < 3 )) || fail 126 'Steam failed: restart re-forward limit (3) exceeded'
    reforwards=$((reforwards + 1))
    say "restart: re-forwarding AppID ($reforwards/3)"
    wait_for_steam_ready
    forward_appid
    launch_generation="$generation"
    launch_deadline=$((SECONDS + 180))
  fi
  if (( saw_added == 0 && SECONDS >= launch_deadline )); then
    fail 124 "no game process for AppID $appid within 180 s"
  fi
  sleep 1
done
