# Retroid Pocket Mini V2 systems. The console system remains the verified
# recovery baseline; the default configuration adds the Korri product session.
{
  nixpkgs,
  korri,
  ...
}:
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
  # Both product exports use this same off-device cross build and config contract.
  kernelCross = crossPkgs.callPackage ./kernel {
    rpminiFirmware = firmwareCross;
    stdenv = crossPkgs.gcc15Stdenv;
    kernelConfig = ./kernel/config-korri;
  };
  dtDriverCheck = import ./kernel/dt-driver-check.nix {
    pkgs = buildPkgs;
    kernel = kernelCross;
  };
  # Retain the unchanged ROCKNIX configuration as a diagnostic output. Product
  # uses that baseline plus the explicit delta; recovery stays on its TTY trim.
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
      (import ../../product/image-plugins.nix {
        inherit korri;
        device = "rpminiv2";
      })
      ./portal.nix
      (
        { lib, ... }:
        {
          # Direct image builds must pass the same compiled-driver gate as checks.
          sdImage.populateRootCommands = lib.mkBefore ''
            test -s ${dtDriverCheck}
          '';
        }
      )
    ];
  };
in
{
  inherit
    configuration
    consoleConfiguration
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
        dtDriverCheck
        ;
    };
  inputdActionCheck =
    pkgs:
    import ./inputd-action-check.nix {
      inherit pkgs configuration;
      inputd = korri.packages.${pkgs.stdenv.hostPlatform.system}.korri-inputd;
    };
  volumeCheck = pkgs: import ./volume-vm-test.nix { inherit pkgs configuration; };
  wifiCheck = pkgs: import ./wifi-check.nix { inherit pkgs configuration; };
  systemdCheck =
    pkgs:
    import ../../product/systemd/check.nix {
      inherit pkgs configuration consoleConfiguration;
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
