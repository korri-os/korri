#!/usr/bin/env bash
# Sent over SSH by run.py. No build, activation, block write or bootloader install.
set -euo pipefail

fail() { echo "Mini V2 update refused: $*" >&2; return 1; }

check_mount_values() {
  local target=$1 source=$2 kind=$3 label=$4 options=$5
  case "$target:$source:$kind:$label" in
    /boot:/dev/sda21:vfat:RPMINIV2|/:/dev/sda22:ext4:NIXOS_RPMINIV2) ;;
    *) fail "unexpected mount: $target $source $kind $label"; return 1 ;;
  esac
  [[ ,$options, == *,rw,* ]] || fail "$target is not writable"
}

check_mounts() {
  local target source kind label options
  [[ $(tr -d '\0' </proc/device-tree/model) == 'Retroid Pocket Mini V2' ]] || fail 'wrong board'
  for target in /boot /; do
    source=$(findmnt --mountpoint "$target" -n -o SOURCE)
    kind=$(findmnt --mountpoint "$target" -n -o FSTYPE)
    options=$(findmnt --mountpoint "$target" -n -o OPTIONS)
    label=$(blkid -p -s LABEL -o value "$source")
    check_mount_values "$target" "$source" "$kind" "$label" "$options"
  done
  [[ $(findmnt -R --mountpoint /boot -n -o TARGET | wc -l) == 1 ]] || fail 'nested mount below /boot'
  local path
  for path in /boot/EFI /boot/EFI/nixos /boot/EFI/BOOT /boot/boot /boot/boot/grub; do
    [[ -d $path && ! -L $path ]] || fail "missing or symlink directory: $path"
  done
  [[ -f /boot/boot/grub/grub.cfg && ! -L /boot/boot/grub/grub.cfg ]] || fail 'not a regular grub.cfg'
  local policy
  policy=$(nix config show)
  for setting in 'require-sigs = true' 'max-jobs = 0' 'builders = ' 'fallback = false'; do
    grep -Fx -- "$setting" <<<"$policy" >/dev/null || fail "device policy changed: $setting"
  done
}

check_space() {
  local available
  available=$(df --output=avail --block-size=1 "$1" | tail -n 1)
  [[ $available -ge $2 ]] || fail "ESP needs $2 bytes; only $available available"
}

# Each rename stays on one filesystem. sync also covers FAT metadata before
# the profile changes. An interruption cannot truncate the active grub.cfg.
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
  [[ -f $cfg.previous && ! -L $cfg.previous ]] || fail 'no previous grub.cfg'
  previous=$(readlink -f "$profile-rpminiv2-previous")
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
  nix-env -p "$profile-rpminiv2-previous" --set "$previous"
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

status() {
  printf 'current=%s\n' "$(readlink -f /run/current-system)"
  printf 'profile=%s\n' "$(readlink -f /nix/var/nix/profiles/system)"
  printf 'grub_sha256=%s\n' "$(sha256sum /boot/boot/grub/grub.cfg | cut -d ' ' -f1)"
  printf 'grub_base64=%s\n' "$(base64 -w0 /boot/boot/grub/grub.cfg)"
  printf 'previous_profile=%s\n' "$(readlink -f /nix/var/nix/profiles/system-rpminiv2-previous || true)"
  df -h /boot /
  findmnt --mountpoint /boot
  findmnt --mountpoint /
}

main() {
  check_mounts
  local action=${1:?action} cfg=/boot/boot/grub/grub.cfg profile=/nix/var/nix/profiles/system
  if [[ $action == status ]]; then status; return; fi
  # Lock an existing ext4 directory; status never creates even a lock file.
  exec {lock}</nix/var/nix/profiles
  flock -n "$lock" || fail 'another Mini V2 update holds the profile lock'
  local expected=${2:?expected system} expected_hash=${3:?expected grub hash}
  [[ $(readlink -f /run/current-system) == "$expected" ]] || fail 'booted system changed'
  [[ $(sha256sum "$cfg" | cut -d ' ' -f1) == "$expected_hash" ]] || fail 'grub.cfg changed since inspection'
  if [[ $action == rollback ]]; then rollback "$cfg" "$profile"; status; return; fi
  [[ $action == update ]] || fail 'unknown action'
  [[ $(readlink -f "$profile") == "$expected" ]] || fail 'profile differs from booted system; reboot or recover first'
  local system=${4:?new system} needed=${5:?ESP required bytes} stage name
  [[ -x $system/init ]] || fail 'new system closure is missing'
  check_space /boot "$needed"
  stage=$(mktemp -d /var/tmp/rpminiv2-update.XXXXXX)
  trap 'rm -rf -- "$stage"' EXIT
  tar -xf - -C "$stage"
  (cd "$stage" && sha256sum --check SHA256SUMS)
  # Never replace the loader or font during updates. Compare the shared assets
  # against the installed, hardware-proven handoff before modifying boot files.
  cmp "$stage/BOOTAA64.EFI" /boot/EFI/BOOT/BOOTAA64.EFI
  cmp "$stage/dejavu-mono.pf2" /boot/boot/grub/dejavu-mono.pf2
  check_mounts
  check_space /boot "$needed"
  for name in "$stage"/payload/*; do
    copy_payload "$name" "/boot/EFI/nixos/$(basename "$name")"
  done
  commit_update "$cfg" "$profile" "$stage/grub.cfg" "$system"
  rm -rf -- "$stage"
  trap - EXIT
  echo 'Update staged. Reboot separately, then verify the booted system.'
  status
}

if [[ ${BASH_SOURCE[0]:-$0} == "$0" ]]; then main "$@"; fi
