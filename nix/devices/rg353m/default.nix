{ nixpkgs, korri }:

let
  system = "aarch64-linux";
  configuration = nixpkgs.lib.nixosSystem {
    inherit system;
    specialArgs = { inherit korri; };
    modules = [
      (import ../../../services/inputd/nix/korri-linux-host.nix { inherit korri; })
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
    configuration.pkgs.callPackage ../../../services/sunshine/ffmpeg-rkmpp-static.nix
      {
        inherit rockchipMpp;
      };
  sunshineRkmpp = configuration.pkgs.callPackage ../../../services/sunshine/package.nix {
    sunshine = configuration.pkgs.sunshine;
    cudaSupport = false;
    rkmppSupport = true;
    ffmpegRkmpp = sunshineFfmpegRkmpp;
    inherit rockchipMpp;
  };
  # Build the card from the portal configuration for the same reason the host
  # name does: a card written from the base configuration boots and draws
  # nothing, which reads as dead hardware rather than a missing surface.
  sdImage = portalPreviewConfiguration.config.system.build.sdImage;
  # A rescue card has to sit in the slot of a device that already has a system
  # on eMMC. The sd-image module derives the root device from the volume label,
  # as /dev/disk/by-label/<rootVolumeLabel>, so two roots sharing a label leave
  # /dev/disk/by-label holding one symlink and stage-1 mounts whichever device
  # udev settled last. Give the card its own label so each system resolves only
  # its own root, and build it from the portal configuration so the card runs
  # the product rather than a host with no surface.
  rescueConfiguration = portalPreviewConfiguration.extendModules {
    modules = [
      {
        image.baseName = nixpkgs.lib.mkForce "nixos-rg353m-rescue";
        sdImage.rootVolumeLabel = nixpkgs.lib.mkForce "KORRI_RESCUE";
        # The NixOS SD builder derives /boot/firmware's device from this label.
        sdImage.firmwarePartitionName = nixpkgs.lib.mkForce "KORRI_RBOOT";
      }
    ];
  };
  rescueSdImage = rescueConfiguration.config.system.build.sdImage;
  uboot = configuration.pkgs.callPackage ./uboot.nix { };
  inputplumberData =
    pkgs: inputplumber: import ./inputplumber-data.nix { inherit pkgs inputplumber; };
  usbGadgetCheck =
    pkgs:
    pkgs.callPackage ./usb-gadget-check.nix {
      inherit configuration;
    };
  audioCheck =
    pkgs:
    import ./audio-check.nix {
      inherit pkgs;
      configuration = portalPreviewConfiguration;
    };
}
