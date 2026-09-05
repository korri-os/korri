# AYN Odin 2 Portal composition.
#
# The device keeps its Android install. This image lives entirely on an SD
# card and boots through the U-Boot that AYN ships in `loader_a`, reached by
# switching BOOT MODE from Android to Loader. Nothing here writes to internal
# storage.
#
# The kernel and firmware are exposed both natively and cross-built from
# x86_64. The aarch64 builder cannot spare the ~30 GB a kernel compile needs,
# so the system is assembled from the cross-built kernel while the rest of the
# closure substitutes from cache.
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

  kernel = pkgs.callPackage ./kernel { };
  kernelCross = crossPkgs.callPackage ./kernel { };
  rescueKernel = pkgs.callPackage ./kernel-rescue-7.0.2 { };
  rescueKernelCross = crossPkgs.callPackage ./kernel-rescue-7.0.2 { };
  firmware = pkgs.callPackage ./firmware { };
  firmwareCross = crossPkgs.callPackage ./firmware { };

  configuration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = {
      odinKernel = kernelCross;
      odinRescueKernel = rescueKernelCross;
      odinFirmware = firmwareCross;
    };
    modules = [ ./sd-image.nix ];
  };
in
{
  inherit
    kernel
    kernelCross
    rescueKernel
    rescueKernelCross
    firmware
    firmwareCross
    configuration
    ;
  sdImage = configuration.config.system.build.sdImage;
}
