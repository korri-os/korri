{ nixpkgs, korri }:

let
  system = "aarch64-linux";
  configuration = nixpkgs.lib.nixosSystem {
    inherit system;
    specialArgs = { inherit korri; };
    modules = [
      (import ../../services/inputd/nix/korri-linux-host.nix { inherit korri; })
      ./sd-image.nix
    ];
  };
in
{
  inherit configuration;
  portalPreviewConfiguration = configuration.extendModules {
    modules = [ (import ./portal-preview.nix { inherit korri; }) ];
  };
  sdImage = configuration.config.system.build.sdImage;
  uboot = configuration.pkgs.callPackage ./uboot.nix { };
  inputplumberData = pkgs: inputplumber: import ./inputplumber-data.nix { inherit pkgs inputplumber; };
  usbGadgetCheck =
    pkgs:
    pkgs.callPackage ./usb-gadget-check.nix {
      inherit configuration;
    };
}
