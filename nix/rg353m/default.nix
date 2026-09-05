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
rec {
  inherit configuration;
  portalPreviewConfiguration = configuration.extendModules {
    modules = [ (import ./portal-preview.nix { inherit korri; }) ];
  };
  rkMppServiceModule =
    configuration.config.boot.kernelPackages.callPackage ./rk-mpp-service-module.nix
      { };
  rockchipMpp = configuration.pkgs.callPackage ./rockchip-mpp.nix { };
  ffmpegRockchip = configuration.pkgs.callPackage ./ffmpeg-rockchip.nix {
    inherit rockchipMpp;
  };
  sunshineFfmpegRkmpp =
    configuration.pkgs.callPackage ../../services/sunshine/ffmpeg-rkmpp-static.nix
      {
        inherit rockchipMpp;
      };
  sunshineRkmpp = configuration.pkgs.callPackage ../../services/sunshine/package.nix {
    sunshine = configuration.pkgs.sunshine;
    cudaSupport = false;
    rkmppSupport = true;
    ffmpegRkmpp = sunshineFfmpegRkmpp;
    inherit rockchipMpp;
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
