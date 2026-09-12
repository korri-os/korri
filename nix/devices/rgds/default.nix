{ nixpkgs, korri }:
let
  configuration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = { inherit korri; };
    modules = [
      (import ../../../services/inputd/nix/korri-linux-host.nix { inherit korri; })
      ./sd-image.nix
      ./portal.nix
    ];
  };
  # Expose local cross-builds for preflight on development machines, never on
  # the handheld. The distribution workflow builds the native ARM image.
  crossPkgs = (import nixpkgs { system = "x86_64-linux"; }).pkgsCross.aarch64-multiplatform;
  kernelCross = crossPkgs.callPackage ./kernel.nix { };
in
{
  inherit configuration;
  sdImage = configuration.config.system.build.sdImage;
  kernel = configuration.config.boot.kernelPackages.kernel;
  uboot = configuration.pkgs.callPackage ./uboot.nix { };
  inherit kernelCross;
  ubootCross = crossPkgs.callPackage ./uboot.nix { };
  moduleCheck = pkgs: import ./module-check.nix { inherit pkgs configuration; };
  inputplumberData =
    pkgs: inputplumber: import ./inputplumber-data.nix { inherit pkgs inputplumber; };
  # Exercise the same shrinker as NixOS stage-1 against built ARM modules.
  # Cross builds make this gate runnable before another native CI image build.
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
