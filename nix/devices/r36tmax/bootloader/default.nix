# RK3326 boot-chain spike.
#
# Build-only R36T Max candidate, not a device or SD-image loader selection.
# Board compatibility and cold/warm boots remain separate acceptance gates.
{ nixpkgs }:

let
  system = "aarch64-linux";
  pkgs = import nixpkgs {
    inherit system;
    # This spike deliberately includes redistributable proprietary DDR code.
    config.allowUnfreePredicate =
      pkg:
      builtins.elem (nixpkgs.lib.getName pkg) [
        "rkbin"
        "uboot-odroid-go2_defconfig"
      ];
  };

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
