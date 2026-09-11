#!/bin/sh
# Bring up SSH on an R36-class handheld's stock firmware.
#
# EmulationStation runs this as root when the user launches one of the
# `Korri SSH` entries. It is invoked by `launcher.sh`, which passes the card
# root as $1 because the mount point differs between stock images.
#
# Two independent goals, in this order:
#
#   1. Start a shell *now*, from the card, even if nothing can be persisted.
#      A live session is what lets us learn why persistence failed.
#   2. Persist across reboot by installing onto internal storage.
#
# Everything is logged to the card, because on a failed run the log is the
# only thing that comes back.

card_root=${1:-}
if [ -z "$card_root" ]; then
  exit 1
fi

log="$card_root/korri-ssh-install.log"
exec >>"$log" 2>&1
echo "=============================================================="
echo "korri ssh install: $(date 2>/dev/null)"
set -x

# --- facts first, so a failed run still teaches us something ------------
uname -a
cat /etc/os-release
cat /proc/device-tree/model 2>/dev/null | tr -d '\0'; echo
cat /proc/device-tree/compatible 2>/dev/null | tr -d '\0'; echo
cat /proc/cmdline
free -m 2>/dev/null || cat /proc/meminfo
cat /proc/mounts
ls -la /storage/ 2>/dev/null
# Dropbear rejects an authorized_keys directory whose ancestors are writable
# by others, so the modes of these two matter as much as the files' own.
ls -ld / /storage /run /tmp 2>/dev/null
ip addr 2>/dev/null || ifconfig -a 2>/dev/null
command -v systemctl && systemctl show --property=UnitPath 2>/dev/null

# --- stop anything a previous run left behind ---------------------------
# Otherwise the old server keeps the port and the new one exits, so a fixed
# payload would appear not to have taken effect at all.
if command -v systemctl >/dev/null 2>&1; then
  systemctl stop korri-ssh.service 2>/dev/null
fi
for stale in /storage/.korri-ssh/dropbear.pid /run/korri-ssh/dropbear.pid /tmp/korri-ssh/dropbear.pid; do
  if [ -f "$stale" ]; then
    kill "$(cat "$stale")" 2>/dev/null
    rm -f "$stale"
  fi
done
sleep 1

# --- goal 1: a shell this session, straight off the card ----------------
# The card is usually FAT, which cannot hold the executable bit, so the
# binary is copied to a real filesystem before it is run.
#
# Not /tmp. Dropbear checks every ancestor of its authorized_keys directory
# and refuses one that is writable by others, so a world-writable /tmp makes
# every public key fail with nothing but "Permission denied" on the client:
#   "/tmp must be owned by user or root, and not writable by group or others"
# /storage is the writable partition on internal storage; /run is a
# root-owned tmpfs. Both have clean ancestors.
if [ -d /storage ]; then
  run_dir=/storage/.korri-ssh
else
  run_dir=/run/korri-ssh
fi
mkdir -p "$run_dir"
cp "$card_root/.korri/dropbear" "$run_dir/dropbear"
cp "$card_root/.korri/dropbear_ed25519_host_key" "$run_dir/host_key"
cp "$card_root/.korri/authorized_keys" "$run_dir/authorized_keys"
chmod 755 "$run_dir/dropbear"
chmod 700 "$run_dir"
chmod 600 "$run_dir/host_key" "$run_dir/authorized_keys"

# -D names the directory holding authorized_keys, so no home-directory
# rewriting is needed. Key-only: -s disables password logins.
# No -F here, so this copy forks and the launcher can return to the menu.
#
# Port 2223, not 2222. The persistent service below listens on 2222, and two
# servers cannot share a port: whichever loses would exit, and with
# Restart=always the service would loop instead of failing visibly. Keeping
# them apart also makes the reachable port say which path succeeded.
"$run_dir/dropbear" -E -s -p 2223 \
  -r "$run_dir/host_key" \
  -D "$run_dir" \
  -P "$run_dir/dropbear.pid"
sleep 2
cat "$run_dir/dropbear.pid" 2>/dev/null

# --- goal 2: survive a reboot -------------------------------------------
# /storage is the writable partition on internal storage in LibreELEC-derived
# stock images, and /storage/.config/system.d is the writable systemd unit
# path. Persisting only makes sense if both exist.
persist_dir=/storage/.ssh
unit_dir=/storage/.config/system.d

if [ -d /storage ] && mkdir -p "$persist_dir" 2>/dev/null; then
  cp "$run_dir/dropbear" "$persist_dir/dropbear"
  cp "$run_dir/host_key" "$persist_dir/dropbear_ed25519_host_key"
  cp "$run_dir/authorized_keys" "$persist_dir/authorized_keys"
  chmod 755 "$persist_dir/dropbear"
  chmod 700 "$persist_dir"
  chmod 600 "$persist_dir/dropbear_ed25519_host_key" \
    "$persist_dir/authorized_keys"

  if command -v systemctl >/dev/null 2>&1; then
    mkdir -p "$unit_dir"
    cp "$card_root/.korri/korri-ssh.service" "$unit_dir/korri-ssh.service"

    # The stock frontend suspends the device on idle, which drops the
    # network a minute or two after every boot. Masking the sleep chain in
    # the writable unit path is reversible: delete the symlinks.
    for unit in suspend.target sleep.target hybrid-sleep.target hibernate.target; do
      ln -sf /dev/null "$unit_dir/$unit"
    done

    systemctl daemon-reload
    systemctl enable korri-ssh.service
    systemctl restart korri-ssh.service
    sleep 3
    systemctl status korri-ssh.service
  fi
fi

# --- report what we ended up with ---------------------------------------
# 2223 means the card-launched server is up; 2222 means persistence worked
# too and SSH will survive a reboot and the card being removed.
(netstat -tlnp 2>/dev/null || ss -tlnp 2>/dev/null) | grep -E '2222|2223|:22 '
ip addr 2>/dev/null | grep -E 'inet |state '
echo "korri ssh install: done"
sync
