{ nixpkgs, korri }:
let
  configuration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = { inherit korri; };
    modules = [ ./sd-image.nix ];
  };
  # Expose local cross-builds for preflight on development machines, never on
  # the handheld. The distribution workflow builds the native ARM image.
  crossPkgs = (import nixpkgs { system = "x86_64-linux"; }).pkgsCross.aarch64-multiplatform;
in
{
  inherit configuration;
  sdImage = configuration.config.system.build.sdImage;
  kernel = configuration.config.boot.kernelPackages.kernel;
  uboot = configuration.pkgs.callPackage ./uboot.nix { };
  kernelCross = crossPkgs.callPackage ./kernel.nix { };
  ubootCross = crossPkgs.callPackage ./uboot.nix { };
  moduleCheck = pkgs: import ./module-check.nix { inherit pkgs configuration; };
}
