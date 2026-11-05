#!/usr/bin/env -S nix shell nixpkgs#bash nixpkgs#coreutils nixpkgs#findutils nixpkgs#gnused nixpkgs#shadow --command bash
# One-off operational migration: the shell's system user `korri-portal`
# becomes `korri-shell` (2026-11). Run it as root on the device, once, after
# the device runs the system generation whose korri-shell.service has
# User=korri-shell. It is idempotent; a second run finds nothing to do.
#
# It moves /var/lib/korri-portal to /var/lib/korri-shell, gives every file
# the old account owned on the root filesystem to korri-shell, removes
# /run/korri-portal, and deletes the old user and group if they still exist.
# Korri carries no fallback for the old name: this script is the cut.
set -euo pipefail

old=korri-portal
new=korri-shell
unit=korri-shell.service
systemctl=/run/current-system/sw/bin/systemctl

note() { printf 'migrate-shell-user: %s\n' "$*"; }
fail() { note "$*" >&2; exit 1; }

[[ $(id -u) -eq 0 ]] || fail "run as root"
getent passwd "$new" >/dev/null || fail "user $new is missing: switch to the new system generation first"
[[ $("$systemctl" show -P User "$unit") == "$new" ]] ||
  fail "$unit does not run as $new: switch to the new system generation first"

# The old IDs: from the account if it still exists, otherwise from the IDs
# NixOS keeps reserved in /var/lib/nixos after it removes a declared user.
id_of() { # id_of passwd|group NAME MAPFILE
  local entry
  if entry=$(getent "$1" "$2"); then
    cut -d: -f3 <<<"$entry"
  elif [[ -f $3 ]]; then
    sed -n "s/.*\"$2\": *\([0-9][0-9]*\).*/\1/p" "$3"
  fi
}
old_uid=$(id_of passwd "$old" /var/lib/nixos/uid-map)
old_gid=$(id_of group "$old" /var/lib/nixos/gid-map)
new_uid=$(id -u "$new")
new_gid=$(id -g "$new")
note "old uid=${old_uid:-unknown} gid=${old_gid:-unknown}; new uid=$new_uid gid=$new_gid"
[[ -z $old_uid || $old_uid != "$new_uid" ]] || fail "old and new uid are equal; nothing to migrate"

state_old=/var/lib/$old
state_new=/var/lib/$new
[[ ! -L $state_old ]] || fail "$state_old is a symlink; refusing"
aside=$state_new.before-migration
[[ ! -d $state_old || ! -e $state_new || ! -e $aside ]] ||
  fail "$aside already exists; inspect it, then remove it"

# Any other file the old account owned, on the root filesystem only.
strays=()
if [[ -n $old_uid || -n $old_gid ]]; then
  match=()
  [[ -z $old_uid ]] || match+=(-uid "$old_uid")
  [[ -z $old_uid || -z $old_gid ]] || match+=(-o)
  [[ -z $old_gid ]] || match+=(-gid "$old_gid")
  while IFS= read -r -d '' path; do
    [[ $path == "$state_old" || $path == "$state_old"/* || $path == /run/$old || $path == /run/$old/* ]] ||
      strays+=("$path")
  done < <(find / -xdev -path /nix -prune -o \( "${match[@]}" \) -print0)
fi

if [[ ! -e $state_old && ! -e /run/$old && ${#strays[@]} -eq 0 ]] &&
  ! getent passwd "$old" >/dev/null && ! getent group "$old" >/dev/null; then
  note "nothing to do"
  exit 0
fi

was_active=no
if "$systemctl" is-active -q "$unit"; then
  was_active=yes
  note "stopping $unit"
  "$systemctl" stop "$unit"
fi

if [[ -d $state_old ]]; then
  if [[ -e $state_new ]]; then
    # The new unit created an empty home on its first start. Keep it aside
    # instead of deleting it.
    note "moving the new unit's first-start $state_new to $aside"
    mv -T "$state_new" "$aside"
  fi
  note "moving $state_old to $state_new"
  mv -T "$state_old" "$state_new"
fi
if [[ -d $state_new ]]; then
  chown -R "$new:$new" "$state_new"
  chmod 0700 "$state_new"
fi

# Runtime state is per boot. systemd removed it when the old unit stopped;
# remove what an unclean stop may have left.
if [[ -d /run/$old && ! -L /run/$old ]]; then
  note "removing /run/$old"
  rm -rf --one-file-system "/run/$old"
fi

for path in "${strays[@]}"; do
  note "giving $path to $new"
  chown -h "$new:$new" "$path"
done

if getent passwd "$old" >/dev/null; then
  note "deleting user $old"
  userdel "$old"
fi
if getent group "$old" >/dev/null; then
  note "deleting group $old"
  groupdel "$old"
fi

if [[ $was_active == yes ]]; then
  note "starting $unit"
  "$systemctl" start "$unit"
fi
note "done"
