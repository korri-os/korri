# Build-only entry point. No image, deployment, or network configuration effects.
{
  nixpkgs ? (builtins.getFlake (toString ../../../..)).inputs.nixpkgs,
}:
let
  pkgs = nixpkgs.legacyPackages.x86_64-linux.pkgsCross.aarch64-multiplatform;
  kernel = pkgs.callPackage ../dts/kernel-trimmed.nix { };
  kernelPackages = pkgs.linuxPackagesFor kernel;
  source = pkgs.callPackage ./source.nix { };
  driver = kernelPackages.callPackage ./driver.nix { };
  # No compilation is involved; build the local-only firmware package on the host.
  firmware = nixpkgs.legacyPackages.x86_64-linux.callPackage ./firmware.nix { };
in
{
  inherit
    kernel
    source
    driver
    firmware
    ;
  checks = nixpkgs.legacyPackages.x86_64-linux.callPackage ./checks.nix {
    inherit
      kernel
      source
      driver
      firmware
      ;
  };
}
