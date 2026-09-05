# Correct first-boot root expansion for mmcblk-rooted SD images.
#
# The upstream sd-image expansion reads the partition number from
# `lsblk -npo MAJ:MIN`, taking the minor number. That holds for /dev/sda2
# (8:2) but not for /dev/mmcblk1p2, whose minor is 98. sfdisk then fails on a
# partition that does not exist, and because the first-boot script runs under
# `set -e`, everything after it is skipped, including the
# `nix-store --load-db` that registers the shipped store. The result is a root
# filesystem stuck at image size and an unusable Nix database.
#
# Read the partition number from sysfs instead, where it is stated directly.
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

      echo ",+," | ${pkgs.util-linux}/bin/sfdisk -N"$partNum" --no-reread --force "$bootDevice" || true
      ${pkgs.util-linux}/bin/partx -u "$bootDevice" || true
      ${pkgs.e2fsprogs}/bin/resize2fs "$rootPart" || true

      ${config.nix.package.out}/bin/nix-store --load-db < /nix-path-registration
      rm -f /nix-path-registration
    fi
  '';
}
