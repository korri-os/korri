# Retroid Pocket Mini V2 systems. The console system remains the verified
# recovery baseline; the default configuration adds the Korri product session.
{ nixpkgs, korri, ... }:
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
  recoveryKernel = pkgs.callPackage ./kernel {
    rpminiFirmware = firmware;
    stdenv = pkgs.gcc15Stdenv;
  };
  recoveryKernelCross = crossPkgs.callPackage ./kernel {
    rpminiFirmware = firmwareCross;
    stdenv = crossPkgs.gcc15Stdenv;
  };
  kernel = pkgs.callPackage ./kernel {
    rpminiFirmware = firmware;
    stdenv = pkgs.gcc15Stdenv;
    kernelConfig = ./kernel/config-korri;
  };
  kernelCross = crossPkgs.callPackage ./kernel {
    rpminiFirmware = firmwareCross;
    stdenv = crossPkgs.gcc15Stdenv;
    kernelConfig = ./kernel/config-korri;
  };
  # Retain the full ROCKNIX configuration as a diagnostic output. The recovery
  # and product images use the two bounded profiles above.
  kernelSourceGcc15 = crossPkgs.callPackage ./kernel {
    rpminiFirmware = firmwareCross;
    stdenv = crossPkgs.gcc15Stdenv;
    kernelConfig = ./kernel/config;
  };
  consoleConfiguration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = {
      inherit korri;
      # Build kernels on development machines. Never compile on the handheld.
      rpminiKernel = recoveryKernelCross;
      rpminiKorriKernel = kernelCross;
      rpminiFirmware = firmwareCross;
      rpminiRocknixBaseline = rocknixBaseline;
    };
    modules = [ ./sd-image.nix ];
  };
  configuration = consoleConfiguration.extendModules {
    modules = [
      (import ../../product/nixos-module.nix { inherit korri; })
      ./portal.nix
    ];
  };
in
{
  inherit
    configuration
    consoleConfiguration
    kernel
    kernelCross
    recoveryKernel
    recoveryKernelCross
    kernelSourceGcc15
    firmware
    firmwareCross
    rocknixBaseline
    ;
  sdImage = configuration.config.system.build.sdImage;
  consoleSdImage = consoleConfiguration.config.system.build.sdImage;
  inputplumberData =
    pkgs: inputplumber: import ./inputplumber-data.nix { inherit pkgs inputplumber; };
  moduleCheck =
    pkgs:
    import ./module-check.nix {
      inherit
        pkgs
        nixpkgs
        korri
        configuration
        consoleConfiguration
        ;
    };
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
  recoveryInitrdModulesCheck =
    pkgs:
    pkgs.makeModulesClosure {
      kernel = recoveryKernelCross.modules;
      firmware = [ firmwareCross ];
      extraFirmwarePaths = consoleConfiguration.config.boot.initrd.extraFirmwarePaths;
      rootModules =
        consoleConfiguration.config.boot.initrd.availableKernelModules
        ++ consoleConfiguration.config.boot.initrd.kernelModules
        ++ consoleConfiguration.config.boot.kernelModules;
      allowMissing = false;
    };
}
