# AYN Odin 2 Portal composition. The slices so far are the kernel and the
# device firmware: prove the ROCKNIX SM8550 tree builds under Nix, produces
# the Portal DTB, and that every blob the device trees name is available.
#
# The aarch64 builder (fuji) lacks the disk for a kernel compile, so both
# packages are also exposed built from x86_64. For the kernel that is a real
# cross-compile; the firmware is pure data whose builder simply runs on the
# build platform. The native attributes stay for an aarch64 builder.
{ nixpkgs }:

let
  mkPkgs =
    system:
    import nixpkgs {
      inherit system;
      # The AYN ADSP, CDSP, WiFi board, and Venus blobs are proprietary
      # vendor firmware.
      config.allowUnfree = true;
    };
  pkgs = mkPkgs "aarch64-linux";
  crossPkgs = (mkPkgs "x86_64-linux").pkgsCross.aarch64-multiplatform;
in
{
  kernel = pkgs.callPackage ./kernel { };
  kernelCross = crossPkgs.callPackage ./kernel { };
  firmware = pkgs.callPackage ./firmware { };
  firmwareCross = crossPkgs.callPackage ./firmware { };
}
