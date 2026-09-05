# First-boot root expansion for a GPT image on an mmcblk-rooted card.
#
# Two things the upstream sd-image expansion gets wrong here. It reads the
# partition number from `lsblk -npo MAJ:MIN`, which holds for /dev/sda2
# (8:2) but not for /dev/mmcblk0p2, whose minor is 98; sysfs states the
# number directly. And it assumes MBR: on a GPT disk the backup header sits
# where the image ended, not where the card ends, so growing the partition
# past it corrupts the table. sgdisk -e relocates the backup header to the
# true end of the disk first.
#
# The script runs under `set -e` in boot.postBootCommands, so every step
# that may legitimately fail on a rerun is allowed to, and the Nix database
# registration at the end must still be reached.
{ config, pkgs, ... }:

{
  sdImage.expandOnBoot = false;

  boot.postBootCommands = ''
    if [ -f /nix-path-registration ]; then
      set -euo pipefail
      set -x

      rootPart="$(${pkgs.util-linux}/bin/findmnt -n -o SOURCE /)"
      bootDevice="$(${pkgs.util-linux}/bin/lsblk -npo PKNAME "$rootPart")"
      partNum="$(cat "/sys/class/block/$(${pkgs.coreutils}/bin/basename "$rootPart")/partition")"

      ${pkgs.gptfdisk}/bin/sgdisk -e "$bootDevice" || true
      echo ",+," | ${pkgs.util-linux}/bin/sfdisk -N"$partNum" --no-reread --force "$bootDevice" || true
      ${pkgs.util-linux}/bin/partx -u "$bootDevice" || true
      ${pkgs.e2fsprogs}/bin/resize2fs "$rootPart" || true

      ${config.nix.package.out}/bin/nix-store --load-db < /nix-path-registration
      rm -f /nix-path-registration
    fi
  '';
}
