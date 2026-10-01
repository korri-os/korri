# The Anbernic H700 family: one kernel, one U-Boot and one controller profile
# for the RG35XX SP and RG35XX Pro. Device directories call mkConfiguration
# with their own module of device-tree, identity and SD-label facts.
{ nixpkgs, korri }:
let
  mkPkgs =
    system:
    import nixpkgs {
      inherit system;
      config.allowUnfree = true;
    };
  pkgs = mkPkgs "aarch64-linux";
  crossPkgs = (mkPkgs "x86_64-linux").pkgsCross.aarch64-multiplatform;
  # GCC 15, the compiler ROCKNIX builds this configuration with (its
  # CONFIG_CC_VERSION_TEXT), as on the RP Mini V2.
  kernel = pkgs.callPackage ./kernel { stdenv = pkgs.gcc15Stdenv; };
  kernelCross = crossPkgs.callPackage ./kernel { stdenv = crossPkgs.gcc15Stdenv; };
  uboot = pkgs.callPackage ./uboot.nix { };
  ubootCross = crossPkgs.callPackage ./uboot.nix { };
in
{
  inherit
    kernel
    kernelCross
    uboot
    ubootCross
    ;
  # Product images take the kernel and U-Boot built on a development machine.
  # Never compile them on the handheld.
  mkConfiguration =
    {
      device,
      module,
    }:
    nixpkgs.lib.nixosSystem {
      system = "aarch64-linux";
      specialArgs = {
        inherit korri;
        h700Kernel = kernelCross;
        h700Uboot = ubootCross;
      };
      modules = [
        (import ../../product/nixos-module.nix { inherit korri; })
        (import ../../product/image-plugins.nix {
          inherit korri device;
        })
        ./board.nix
        module
      ];
    };
  inputplumberData =
    pkgs: inputplumber: import ./inputplumber-data.nix { inherit pkgs inputplumber; };
}
