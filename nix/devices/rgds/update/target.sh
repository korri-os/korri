#!/usr/bin/env bash
# Root-filesystem files only. Never write the FAT partition or raw boot area.
set -euo pipefail
UPDATE_PROFILE_SUFFIX=rgds
UPDATE_CONFIG_NAME=extlinux.conf
UPDATE_SPACE_NAME='root boot tree'

fail() { echo "RG DS update refused: $*" >&2; return 1; }

# The SSH runner prepends the shared functions; offline tests source this file.
if ! declare -F atomic_copy >/dev/null; then
  # shellcheck source=nix/devices/system-update/target.sh
  source "$(dirname "${BASH_SOURCE[0]}")/../../system-update/target.sh"
fi

check_mount_values() {
  local target=$1 source=$2 kind=$3 label=$4 options=$5
  [[ $target:$source:$kind:$label == /:/dev/mmcblk0p2:ext4:NIXOS_RGDS ]] || fail "unexpected root: $target $source $kind $label"
  [[ ,$options, == *,rw,* ]] || fail 'root is not writable'
}

check_layout() {
  local media=$1 table=$2
  [[ $media == MMC ]] || fail 'mmcblk0 is not eMMC'
  grep -Fx 'label: dos' <<<"$table" >/dev/null || fail 'not an MBR installation'
  grep -E '^/dev/mmcblk0p2[[:space:]]*:.*type=83, bootable$' <<<"$table" >/dev/null || fail 'root is not the bootable Linux partition'
  if grep -E '^/dev/mmcblk0p1[[:space:]]*:.*bootable' <<<"$table" >/dev/null; then fail 'FAT partition is bootable'; fi
}

check_policy() {
  local policy=$1 setting
  for setting in 'require-sigs = true' 'max-jobs = 0' 'builders = ' 'fallback = false'; do
    grep -Fx -- "$setting" <<<"$policy" >/dev/null || fail "device policy changed: $setting"
  done
}

check_mounts() {
  [[ $(tr -d '\0' </proc/device-tree/model) == 'Anbernic RG DS' ]] || fail 'wrong board'
  check_mount_values / "$(findmnt --mountpoint / -n -o SOURCE)" "$(findmnt --mountpoint / -n -o FSTYPE)" \
    "$(blkid -p -s LABEL -o value /dev/mmcblk0p2)" "$(findmnt --mountpoint / -n -o OPTIONS)"
  # Read the MBR using util-linux; no command in this updater writes it.
  local path
  check_layout "$(cat /sys/class/block/mmcblk0/device/type)" "$(sfdisk --dump /dev/mmcblk0)"
  [[ ! -e /nix-path-registration ]] || fail 'first-boot registration is incomplete; repair separately'
  for path in /boot /boot/extlinux /boot/nixos /nix/store /nix/var/nix/profiles; do
    [[ -d $path && ! -L $path ]] || fail "missing or symlink directory: $path"
    # Compare filesystem identity, not mount targets: NixOS mounts /nix/store
    # as a read-only bind of the same root filesystem (observed on the RG DS:
    # SOURCE /dev/mmcblk0p2[/nix/store], same st_dev as /).
    [[ $(stat -c %d "$path") == "$(stat -c %d /)" ]] || fail "not on the root filesystem: $path"
  done
  # Reject mounts below the two directories that will be written, but leave the
  # unrelated (normally unmounted) /boot/firmware alone.
  if findmnt -rn -o TARGET | grep -E '^/boot/(extlinux|nixos)/' >/dev/null; then fail 'nested boot mount'; fi
  [[ -f /boot/extlinux/extlinux.conf && ! -L /boot/extlinux/extlinux.conf ]] || fail 'not a regular extlinux.conf'
  check_policy "$(nix config show)"
}

# Keep precisely the union referenced by the active and exact rollback configs.
# The native generator owns filenames. No new manifest or generation DB exists.
prune_payloads() {
  local boot=$1 name cfg line relative top
  local -A keep=()
  for cfg in "$boot/extlinux/extlinux.conf" "$boot/extlinux/extlinux.conf.previous"; do
    [[ -f $cfg && ! -L $cfg ]] || fail 'missing retention config'
    while IFS= read -r line; do
      if [[ $line =~ ^[[:space:]]*(LINUX|INITRD|FDT)[[:space:]]+\.\./nixos/([^[:space:]]+)$ ]]; then
        relative=${BASH_REMATCH[2]}
        top=${relative%%/*}
        [[ $top =~ ^[0-9abcdfghijklmnpqrsvwxyz]{32}-[A-Za-z0-9+._=-]+$ ]] || fail 'unsafe retained payload name'
        keep[$top]=1
      fi
    done <"$cfg"
  done
  [[ ${#keep[@]} -ge 3 ]] || fail 'no retention payloads'
  for name in "$boot"/nixos/*; do
    [[ -e $name && ! -L $name ]] || fail "symlink or missing boot payload: $name"
    top=$(basename "$name")
    [[ $top =~ ^[0-9abcdfghijklmnpqrsvwxyz]{32}-[A-Za-z0-9+._=-]+$ ]] || fail "unknown boot payload: $name"
    if [[ ! ${keep[$top]+present} ]]; then rm -rf -- "$name"; fi
  done
  sync "$boot/nixos"
}

install_payloads() {
  local stage=$1 boot=$2 name relative dest directory
  while IFS= read -r -d '' name; do
    relative=${name#"$stage/payload/"}
    [[ $relative != "$name" && $relative != /* && $relative != *..* ]] || fail 'unsafe staged payload'
    dest=$boot/nixos/$relative
    directory=$(dirname "$dest")
    # FAT is not involved; preserve directory structure for the native DTB path.
    [[ ! -L $directory && ! -L ${directory%/*} ]] || fail 'symlink payload directory'
    mkdir -p -- "$directory"
    copy_payload "$name" "$dest"
  done < <(find "$stage/payload" -type f -print0)
}

status() {
  printf 'current=%s\n' "$(readlink -f /run/current-system)"
  printf 'profile=%s\n' "$(readlink -f /nix/var/nix/profiles/system)"
  printf 'extlinux_sha256=%s\n' "$(sha256sum /boot/extlinux/extlinux.conf | cut -d ' ' -f1)"
  printf 'extlinux_base64=%s\n' "$(base64 -w0 /boot/extlinux/extlinux.conf)"
  printf 'previous_profile=%s\n' "$(readlink -f /nix/var/nix/profiles/system-rgds-previous || true)"
  df -h /
  findmnt --mountpoint /
}

main() {
  check_mounts
  local action=${1:?action} cfg=/boot/extlinux/extlinux.conf profile=/nix/var/nix/profiles/system
  if [[ $action == status ]]; then status; return; fi
  exec {lock}</nix/var/nix/profiles
  flock -n "$lock" || fail 'another update holds the profile lock'
  local expected=${2:?expected system} expected_hash=${3:?expected extlinux hash}
  [[ $(readlink -f /run/current-system) == "$expected" ]] || fail 'booted system changed'
  [[ $(sha256sum "$cfg" | cut -d ' ' -f1) == "$expected_hash" ]] || fail 'extlinux.conf changed since inspection'
  if [[ $action == rollback ]]; then rollback "$cfg" "$profile"; status; return; fi
  [[ $action == update ]] || fail 'unknown action'
  [[ $(readlink -f "$profile") == "$expected" ]] || fail 'profile differs from booted system; reboot or recover first'
  local system=${4:?new system} needed=${5:?required bytes} stage
  [[ $system =~ ^/nix/store/[0-9abcdfghijklmnpqrsvwxyz]{32}-nixos-system-rgds-sd-card-[A-Za-z0-9+._=-]+$ && -x $system/init ]] || fail 'new RG DS system closure is missing'
  [[ $needed =~ ^[0-9]+$ ]] || fail 'invalid space requirement'
  check_space /boot "$needed"
  stage=$(mktemp -d /var/tmp/rgds-update.XXXXXX)
  trap 'rm -rf -- "$stage"' EXIT
  tar --no-same-owner --no-same-permissions -xf - -C "$stage"
  (cd "$stage" && sha256sum --check SHA256SUMS)
  check_mounts
  check_space /boot "$needed"
  install_payloads "$stage" /boot
  commit_update "$cfg" "$profile" "$stage/extlinux.conf" "$system"
  prune_payloads /boot
  rm -rf -- "$stage"
  trap - EXIT
  echo 'Update staged. Reboot separately, then verify the booted system.'
  status
}

if [[ ${BASH_SOURCE[0]:-$0} == "$0" ]]; then main "$@"; fi
