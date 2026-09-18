# Retroid Pocket Mini V2 first-boot candidate. The device identity and DTB
# name come from ROCKNIX's sm8250-retroidpocket-rpminiv2.dts.
{ nixpkgs, ... }:
let
  mkPkgs =
    system:
    import nixpkgs {
      inherit system;
      config.allowUnfree = true;
    };
  pkgs = mkPkgs "aarch64-linux";
  buildPkgs = mkPkgs "x86_64-linux";
  crossPkgs = buildPkgs.pkgsCross.aarch64-multiplatform;
  rocknixBaseline = buildPkgs.callPackage ./rocknix-baseline { };
  firmware = pkgs.callPackage ./firmware { };
  firmwareCross = crossPkgs.callPackage ./firmware { };
  kernel = pkgs.callPackage ./kernel {
    rpminiFirmware = firmware;
    rpminiRocknixBaseline = rocknixBaseline;
  };
  kernelCross = crossPkgs.callPackage ./kernel {
    rpminiFirmware = firmwareCross;
    rpminiRocknixBaseline = rocknixBaseline;
  };
  configuration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = {
      # Follow the Odin packaging: build the kernel on x86, assemble the
      # native ARM system on a builder, and never compile on the handheld.
      rpminiKernel = kernelCross;
      rpminiFirmware = firmwareCross;
      rpminiRocknixBaseline = rocknixBaseline;
    };
    modules = [ ./sd-image.nix ];
  };
in
{
  inherit
    configuration
    kernel
    kernelCross
    firmware
    firmwareCross
    rocknixBaseline
    ;
  sdImage = configuration.config.system.build.sdImage;
  moduleCheck = pkgs: import ./module-check.nix { inherit pkgs configuration; };
  initrdModulesCheck =
    pkgs:
    pkgs.makeModulesClosure {
      kernel = kernelCross.modules;
      firmware = [ firmwareCross ];
      extraFirmwarePaths = configuration.config.boot.initrd.extraFirmwarePaths;
      rootModules =
        configuration.config.boot.initrd.availableKernelModules
        ++ configuration.config.boot.initrd.kernelModules
        ++ configuration.config.boot.kernelModules;
      allowMissing = false;
    };
}
