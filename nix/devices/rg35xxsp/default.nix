{ nixpkgs, korri }:
let
  configuration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = { inherit korri; };
    modules = [
      ./sd-image.nix
    ];
  };
  crossPkgs = (import nixpkgs { system = "x86_64-linux"; }).pkgsCross.aarch64-multiplatform;
  kernel = configuration.config.boot.kernelPackages.kernel;
  kernelCross = crossPkgs.callPackage ./kernel.nix { };
  uboot = configuration.pkgs.callPackage ./uboot.nix { };
  ubootCross = crossPkgs.callPackage ./uboot.nix { };
in
{
  inherit configuration;
  sdImage = configuration.config.system.build.sdImage;
  inherit kernel kernelCross;
  inherit uboot ubootCross;
  moduleCheck = pkgs: import ./module-check.nix { inherit pkgs configuration; };
  initrdModulesCheck =
    pkgs:
    pkgs.makeModulesClosure {
      kernel =
        (
          if pkgs.stdenv.hostPlatform.isx86_64 then
            kernelCross
          else
            configuration.config.boot.kernelPackages.kernel
        ).modules;
      firmware = [ pkgs.linux-firmware ];
      rootModules =
        configuration.config.boot.initrd.availableKernelModules
        ++ configuration.config.boot.initrd.kernelModules;
      allowMissing = false;
    };
}
