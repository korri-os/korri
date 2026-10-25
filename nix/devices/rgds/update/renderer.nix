# The same pinned NixOS producer used by generic-extlinux-compatible.populateCmd,
# instantiated for the update host rather than the handheld architecture.
{ pkgs }:
import
  (
    pkgs.path
    + "/nixos/modules/system/boot/loader/generic-extlinux-compatible/extlinux-conf-builder.nix"
  )
  {
    inherit pkgs;
    inherit (pkgs) lib;
  }
