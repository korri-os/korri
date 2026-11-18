# AYN Thor composition.
#
# The Thor is the Odin 2 Portal's SM8550 sibling with two AMOLED screens. Like
# the Portal, it keeps its Android install: this image lives on a microSD card
# and boots through the U-Boot AYN ships in `loader_a`. Nothing here writes to
# internal storage or to a firmware partition.
#
# Shared with the Portal, by import rather than by copy:
#   - the kernel package and its .config (../odin2portal/kernel), cross-built
#     from x86_64 for the same memory reason as the Portal;
#   - the firmware package (../odin2portal/firmware), with variant = "thor";
#   - the ROCKNIX SM8550 userspace harvest (../odin2portal/rocknix);
#   - the SD image and platform policy (../odin2portal/sd-image.nix), with the
#     Thor's labels, DTB and host name forced over the Portal's.
# Thor-only: the DTB (./kernel), the InputPlumber composite (./inputplumber)
# and the display/input/USB facts (./hardware.nix).
{
  nixpkgs,
  korri,
  # The evaluated ../odin2portal composition, shared so its packages are
  # the same derivations the Portal uses.
  odin,
}:

let
  mkPkgs =
    system:
    import nixpkgs {
      inherit system;
      config.allowUnfree = true;
    };
  pkgs = mkPkgs "aarch64-linux";
  crossPkgs = (mkPkgs "x86_64-linux").pkgsCross.aarch64-multiplatform;

  dtb = pkgs.callPackage ./kernel { odinKernel = odin.kernel; };
  dtbCross = crossPkgs.callPackage ./kernel { odinKernel = odin.kernelCross; };
  firmware = pkgs.callPackage ../odin2portal/firmware { variant = "thor"; };
  firmwareCross = crossPkgs.callPackage ../odin2portal/firmware { variant = "thor"; };
  inputplumberData =
    pkgs: inputplumber: pkgs.callPackage ./inputplumber { inherit inputplumber; };

  configuration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = {
      inherit korri;
      # The Portal's sd-image.nix and platform-policy.nix take these names.
      odinKernel = odin.kernelCross;
      odinFirmware = firmwareCross;
      odinRocknix = odin.rocknix;
      thorDtb = dtbCross;
      thorInputplumberData = inputplumberData pkgs korri.packages.aarch64-linux.inputplumber-korri;
    };
    modules = [
      (import ../../product/nixos-module.nix { inherit korri; })
      (import ../../product/image-plugins.nix {
        inherit korri;
        device = "thor";
      })
      ./sd-image.nix
      ./hardware.nix
    ];
  };
in
{
  inherit
    dtb
    dtbCross
    firmware
    firmwareCross
    configuration
    inputplumberData
    ;
  sdImage = configuration.config.system.build.sdImage;
  moduleCheck = pkgs: import ./module-check.nix { inherit pkgs configuration; };
}
