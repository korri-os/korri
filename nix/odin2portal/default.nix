# AYN Odin 2 Portal composition. The first slice is the kernel only: prove
# the ROCKNIX SM8550 tree builds under Nix and produces the Portal DTB.
#
# The aarch64 builder (fuji) lacks the disk for a kernel compile, so the
# kernel is also exposed cross-compiled from x86_64. Both attributes are the
# same package; only the build platform differs.
{ nixpkgs }:

let
  pkgs = import nixpkgs { system = "aarch64-linux"; };
  crossPkgs = (import nixpkgs { system = "x86_64-linux"; }).pkgsCross.aarch64-multiplatform;
in
{
  kernel = pkgs.callPackage ./kernel { };
  kernelCross = crossPkgs.callPackage ./kernel { };
}
