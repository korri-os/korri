# Shared by the initrd and stage-2 recorder. Controller identity comes from
# the working board's sysfs survey and DTS: ff370000 is SD, ff390000 is eMMC.
# Arguments permit filesystem-fixture tests; production uses /dev and /sys.
r36tmax_boot_partition() {
  local dev_root="${1:-/dev}" sys_root="${2:-/sys}" part node
  part=$(readlink -e "$dev_root/disk/by-label/NIXOS_BOOT") || return 1
  node=$(readlink -e "$sys_root/class/block/${part##*/}") || return 1
  case "$node" in
    */ff370000.mmc/mmc_host/*/block/mmcblk*/mmcblk*p1)
      printf '%s\n' "$part"
      ;;
    *) return 1 ;;
  esac
}
