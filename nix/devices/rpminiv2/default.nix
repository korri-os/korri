# Retroid Pocket Mini V2 first-boot candidate. The device identity and DTB
# name come from ROCKNIX's sm8250-retroidpocket-rpminiv2.dts.
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
  kernel = pkgs.callPackage ./kernel { };
  kernelCross = crossPkgs.callPackage ./kernel { };
  firmware = pkgs.callPackage ./firmware { };
  firmwareCross = crossPkgs.callPackage ./firmware { };
  configuration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = {
      inherit korri;
      # Follow the Odin packaging: build the kernel on x86, assemble the
      # native ARM system on a builder, and never compile on the handheld.
      rpminiKernel = kernelCross;
      rpminiFirmware = firmwareCross;
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
