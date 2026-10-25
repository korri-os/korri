{ nixpkgs, korri }:
let
  configuration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = { inherit korri; };
    modules = [
      (import ../../product/nixos-module.nix { inherit korri; })
      (import ../../product/image-plugins.nix {
        inherit korri;
        device = "rgds";
      })
      ./sd-image.nix
      ./portal.nix
    ];
  };
  # Expose local cross-builds for preflight on development machines, never on
  # the handheld. The distribution workflow builds the native ARM image.
  crossPkgs = (import nixpkgs { system = "x86_64-linux"; }).pkgsCross.aarch64-multiplatform;
  kernelCross = crossPkgs.callPackage ../../product/kernel/mainline-7.2.nix { };
in
rec {
  inherit configuration;
  sdImage = configuration.config.system.build.sdImage;
  # With a system on eMMC, an SD sharing either filesystem label can make
  # stage-1 or /boot/firmware select the wrong disk. Match the RG353M rescue
  # labels and keep the full product composition, not a separate runtime.
  rescueConfiguration = configuration.extendModules {
    modules = [
      {
        image.baseName = nixpkgs.lib.mkForce "nixos-rgds-rescue";
        sdImage.rootVolumeLabel = nixpkgs.lib.mkForce "KORRI_RESCUE";
        sdImage.firmwarePartitionName = nixpkgs.lib.mkForce "KORRI_RBOOT";
      }
    ];
  };
  rescueSdImage = rescueConfiguration.config.system.build.sdImage;
  rescueCheck = pkgs: import ./rescue-check.nix { inherit pkgs configuration rescueConfiguration; };
  kernel = configuration.config.boot.kernelPackages.kernel;
  uboot = configuration.pkgs.callPackage ./uboot.nix { };
  inherit kernelCross;
  ubootCross = crossPkgs.callPackage ./uboot.nix { };
  mppCheck =
    pkgs:
    import ../rockchip/mpp-check.nix {
      inherit pkgs configuration;
      device = "rgds";
    };
  moduleCheck = pkgs: import ./module-check.nix { inherit pkgs configuration; };
  bluetoothCheck = pkgs: import ./bluetooth-check.nix { inherit pkgs configuration; };
  firmwareCheck = pkgs: import ./firmware-check.nix { inherit pkgs configuration; };
  registryCheck = pkgs: import ./nixpkgs-registry-check.nix { inherit pkgs configuration; };
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
