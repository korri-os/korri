# RK3326 boot-chain spike.
#
# Deliberately not a device directory. The boot chain is a property of the
# SoC and is identical across every R36-class RK3326 board, while panel,
# joystick, and Wi-Fi nodes are per-board facts. Which unit this targets is
# still unresolved, so no device name is committed here.
{ nixpkgs }:

let
  system = "aarch64-linux";
  pkgs = nixpkgs.legacyPackages.${system};

  armTrustedFirmwarePX30 = pkgs.callPackage ./atf-px30.nix { };
  uboot = pkgs.callPackage ./uboot.nix { inherit armTrustedFirmwarePX30; };
  ubootTplSizeProbe = pkgs.callPackage ./uboot-tpl-size-probe.nix {
    inherit armTrustedFirmwarePX30;
  };
in
{
  inherit
    armTrustedFirmwarePX30
    uboot
    ubootTplSizeProbe
    ;
}
