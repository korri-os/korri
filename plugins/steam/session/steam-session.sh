# Invoked once under dbus-run-session by the public steam-session command.
# Steam's own artifact paths and the Proton tool name are substituted by
# packages/default.nix. The FEX and Proton runtime files come from the
# required plugins through the KORRI_* environment checked by that command.
set -Eeuo pipefail
umask 077

say() { printf 'steam-session: %s\n' "$*" >&2; }
fail() { say "$2"; exit "$1"; }
trap 'fail 126 "Steam failed: session error at line $LINENO (status $?)"' ERR

appid="$1"
export STEAM_GAMES_ROOT="$STEAM_HOME"
export STEAM_DOT="$HOME/.steam"
export STEAM_BETA=steamdeck_stable
export FEX_APP_CONFIG_LOCATION="$KORRI_FEX_CONFIG/"
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
login_deadline=0
fex_rootfs=""
proton_dir=""
forwarded_at=0
resends=0
package_dir="$STEAM_HOME/package"
client_manifest="$package_dir/steam_client_${STEAM_BETA}_linuxarm64.manifest"
saved_manifest="$STEAM_HOME/.korri/steam_client_${STEAM_BETA}_linuxarm64.manifest"

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

# The bootstrapper reads its channel from package/beta and the installed
# client version from that channel's .manifest. When an update installs, it
# renames the downloaded manifest to .manifest. The client then switches its
# own channel to '' and deletes .manifest during the session, so the next start
# reports installed version 0 and Steam cannot launch games until a second
# start reinstalls the same client. Keep a copy of the manifest Steam itself
# installed and put it back before every start.
save_client_manifest() {
  [[ -f "$client_manifest" ]] || return 0
  cmp -s "$client_manifest" "$saved_manifest" && return 0
  mkdir -p "${saved_manifest%/*}"
  cp -f "$client_manifest" "$saved_manifest.tmp.$$"
  mv -f "$saved_manifest.tmp.$$" "$saved_manifest"
}
restore_client_channel() {
  mkdir -p "$package_dir"
  if [[ "$(cat "$package_dir/beta" 2>/dev/null)" != "$STEAM_BETA" ]]; then
    printf '%s\n' "$STEAM_BETA" >"$package_dir/beta.tmp.$$"
    mv -f "$package_dir/beta.tmp.$$" "$package_dir/beta"
  fi
  save_client_manifest
  if [[ ! -f "$client_manifest" && -f "$saved_manifest" &&
    -f "$package_dir/steam_client_${STEAM_BETA}_linuxarm64.installed" ]]; then
    say 'start: restoring the installed Steam client manifest'
    cp -f "$saved_manifest" "$client_manifest.tmp.$$"
    mv -f "$client_manifest.tmp.$$" "$client_manifest"
  fi
}
start_steam() {
  say 'start: Steam desktop client'
  restore_client_channel
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
    save_client_manifest
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
retry_appid() {
  (( reforwards < 3 )) || fail 126 'Steam failed: restart re-forward limit (3) exceeded'
  reforwards=$((reforwards + 1))
  say "restart: re-forwarding AppID ($reforwards/3)"
  wait_for_steam_ready
}
forward_appid() {
  local forward_generation status
  while true; do
    forward_generation="$generation"
    status=0
    say "launch: forwarding AppID $appid"
    timeout 15 @fhs@/bin/steam-arm64-fhs "${flags[@]}" -applaunch "$appid" \
      >>"$STEAM_HOME/logs/session_log.txt" 2>&1 || status=$?
    # A forwarding stub can itself return 42, or report another IPC failure
    # after the managed client exited 42. Both need the same bounded recovery
    # as a restart first noticed by the observation loop.
    check_client
    if (( status == 0 && generation == forward_generation )); then
      launch_generation="$generation"
      launch_deadline=$((SECONDS + 180))
      forwarded_at=$SECONDS
      return 0
    fi
    if (( status == 42 || generation != forward_generation )); then
      retry_appid
    else
      fail 126 'Steam failed: AppID forward failed or timed out'
    fi
  done
}
app_installed() {
  grep -aqE '"StateFlags"[[:space:]]+"4"' \
    "$STEAM_HOME/steamapps/appmanifest_$appid.acf" 2>/dev/null
}
app_process_present() {
  local environ
  for environ in /proc/[0-9]*/environ; do
    [[ -r "$environ" ]] || continue
    if grep -azqxF "SteamAppId=$appid" "$environ" 2>/dev/null; then return 0; fi
  done
  return 1
}

# A prepare program prints its ready directory as the last stdout line. Its
# diagnostics go to this session's stderr.
run_prepare() {
  local label="$1" program="$2" result="$3" output status=0
  output=$("$program" "$KORRI_CACHE_ROOT" 9>&-) || status=$?
  output="${output##*$'\n'}"
  if (( status != 0 )) || [[ "$output" != /* || ! -d "$output" ]]; then
    fail 126 "runtime download failed: $label (prepare exited with status $status)"
  fi
  printf -v "$result" '%s' "$output"
}

# The account root is setgid (game-library-access.nix), so every directory
# created below it inherits the bit. The game unit's RestrictSUIDSGID then
# refuses an extractor's chmod of such a directory, and Steam's own unpacking
# fails. Clear the bit on every directory this user owns in the Steam home.
find "$STEAM_HOME" -xdev -type d -perm -2000 -user "$(id -u)" -exec chmod g-s {} +
mkdir -p "$STEAM_HOME/logs" "$STEAM_HOME/fex-data"
# A lock spans settings and the client lifetime. Never edit Steam-owned VDF
# concurrently with another launch of this session program.
exec 9>"$STEAM_HOME/.korri-session.lock"
flock -n 9 || fail 126 'Steam failed: another Steam session owns STEAM_HOME'
if pgrep -x steam >/dev/null; then fail 126 'Steam failed: client is already running before settings merge'; fi
# bwrap enters its caller's cwd before the guest helper can change directory.
cd "$STEAM_HOME"
say 'runtime: preparing the FEX rootfs and Proton-CachyOS (first run downloads them)'
run_prepare 'FEX rootfs' "$KORRI_FEX_PREPARE" fex_rootfs
run_prepare 'Proton-CachyOS' "$KORRI_PROTON_CACHYOS_PREPARE" proton_dir
export FEX_ROOTFS="$fex_rootfs"
say 'seed: preparing ARM64 runtime and client'
if [[ ! -x "$STEAM_HOME/steamrtarm64/steam" ]]; then
  @helpers@/bin/steam-arm64-seed --apply >>"$STEAM_HOME/logs/session_log.txt" 2>&1 \
    || fail 126 'Steam failed: ARM64 seed failed (see logs/session_log.txt)'
fi
# Link on every launch: the seed runs only once per Steam home, and a new
# Proton pin brings a new tool name and prepared directory.
mkdir -p "$STEAM_HOME/compatibilitytools.d"
ln -sfn "$proton_dir" "$STEAM_HOME/compatibilitytools.d/@protonToolName@"
say 'settings: merging Proton, Steam Input and first-launch seeds'
@settings@/bin/steam-vdf-settings "$STEAM_HOME" "$appid"
if ! login_present; then say 'login: Steam sign-in required; waiting up to 600 s for manual sign-in'; fi
# Runtime preparation and client/runtime downloads must not consume the
# manual sign-in window.
login_deadline=$((SECONDS + 600))
start_steam
wait_for_steam_ready
# Login can first happen during the warm gate. Apply userdata seeds only on
# the next launch: Steam is now running, and its files must not be edited.
mark_logs
reforwards=0
forward_appid
saw_added=0
saw_removed=0
acted=0
say "watch: waiting for AppID $appid"
while true; do
  check_login
  check_client
  save_client_manifest
  read_logs
  if grep -aqE "GameAction \[AppID $appid," <<<"$new_console"; then
    if (( acted == 0 )) && ! app_installed; then
      # Steam asks the person to confirm an install, then downloads. Both
      # can take much longer than a launch.
      say "watch: AppID $appid is not installed; waiting up to 3600 s for the install"
      launch_deadline=$((SECONDS + 3600))
    fi
    acted=1
  fi
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
    retry_appid
    forward_appid
  fi
  # A forward that reaches Steam before sign-in completes is dropped without
  # any GameAction. Send it again, a bounded number of times.
  if (( saw_added == 0 && acted == 0 && resends < 3 && SECONDS - forwarded_at >= 20 )); then
    resends=$((resends + 1))
    say "launch: Steam did not act on AppID $appid; forwarding again ($resends/3)"
    forward_appid
  fi
  if (( saw_added == 0 && SECONDS >= launch_deadline )); then
    fail 124 "no game process for AppID $appid within 180 s"
  fi
  sleep 1
done
