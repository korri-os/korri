---
id: 01M1PEYHWW22GY1RT3VFSX5MWT
slug: report-the-nixpkgs-sd-image-expand-bug-for-mmcblk-rooted-dev
title: Report the nixpkgs sd-image expand bug for mmcblk-rooted devices
origin: parked
status: To Do
priority: low
labels:
  - upstream
  - nixpkgs
  - sd-image
created: 2026-09-04
source: se-work
---

# Report the nixpkgs sd-image expand bug for mmcblk-rooted devices

## Why it matters

The upstream NixOS sd-image first-boot expansion reads the partition number from the minor number in lsblk MAJ:MIN. That is correct for /dev/sda2 but wrong for every mmcblk device, where /dev/mmcblk1p2 reports minor 98. sfdisk then fails on a partition that does not exist, and because the script runs under set -e, the following nix-store --load-db never runs. Affected devices boot with an unexpanded root and an empty Nix database, which presents much later as confusing store corruption. This hits every SD-card NixOS install on Raspberry Pi, Rockchip, and Allwinner hardware, not just the RG353M.

## Acceptance Criteria

- [ ] An upstream nixpkgs issue or PR is filed against nixos/modules/installer/sd-card/sd-image.nix
- [ ] The report includes the MAJ:MIN evidence and the set -e consequence for nix-store --load-db
- [ ] The suggested fix reads /sys/class/block/<dev>/partition instead

## Related

- `nix/rg353m/expand-root.nix`

## Notes

Local fix already landed in nix/rg353m/expand-root.nix and can be offered upstream nearly verbatim.
