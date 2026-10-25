#!/usr/bin/env bash
# Shared file/profile transaction. Device adapters own mount and boot contracts.
check_space() {
  local available
  available=$(df --output=avail --block-size=1 "$1" | tail -n 1)
  [[ $available -ge $2 ]] || fail "${UPDATE_SPACE_NAME} needs $2 bytes; only $available available"
}

# GNU sync FILE calls fsync(2). Each rename stays on one filesystem; syncing
# the containing directory then persists the rename before the profile changes.
atomic_copy() {
  local source=$1 dest=$2 temp
  [[ ! -L $dest ]] || fail "symlink destination: $dest"
  temp=$(mktemp "${dest}.XXXXXX")
  cp -- "$source" "$temp"
  sync "$temp"
  mv -T -- "$temp" "$dest"
  sync "$(dirname "$dest")"
}

copy_payload() {
  local source=$1 dest=$2
  if [[ -e $dest || -L $dest ]]; then
    if [[ -f $dest && ! -L $dest ]] && cmp -s -- "$source" "$dest"; then return; fi
    fail "existing payload differs: $dest"
  else
    atomic_copy "$source" "$dest"
  fi
}

rollback() {
  local cfg=$1 profile=$2 previous
  [[ -f $cfg.previous && ! -L $cfg.previous ]] || fail "no previous ${UPDATE_CONFIG_NAME}"
  previous=$(readlink -f "$profile-${UPDATE_PROFILE_SUFFIX}-previous")
  [[ -d $previous ]] || fail 'previous system is missing'
  # Backup and its GC-rooted profile must describe the same first entry.
  [[ $(sed -n '/ init=/{s/.* init=\([^ ]*\)\/init.*/\1/;p;q;}' "$cfg.previous") == "$previous" ]] || fail 'backup/profile mismatch'
  atomic_copy "$cfg.previous" "$cfg"
  nix-env -p "$profile" --set "$previous"
  sync
}

commit_update() {
  local cfg=$1 profile=$2 candidate=$3 system=$4 previous
  previous=$(readlink -f "$profile")
  # A Nix profile roots the fallback closure even when old generations are GC'd.
  nix-env -p "$profile-${UPDATE_PROFILE_SUFFIX}-previous" --set "$previous"
  atomic_copy "$cfg" "$cfg.previous"
  sync
  # If setting the profile fails after the boot-file commit, repair both sides.
  atomic_copy "$candidate" "$cfg"
  if ! nix-env -p "$profile" --set "$system"; then
    rollback "$cfg" "$profile"
    fail 'profile update failed; restored previous selection'
  fi
  sync
}
