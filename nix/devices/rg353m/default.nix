{ nixpkgs, korri }:

let
  system = "aarch64-linux";
  configuration = nixpkgs.lib.nixosSystem {
    inherit system;
    specialArgs = { inherit korri; };
    modules = [
      (import ../../product/nixos-module.nix { inherit korri; })
      ./sd-image.nix
      ./hardware.nix
    ];
  };
in
rec {
  inherit configuration;
  sdImage = configuration.config.system.build.sdImage;
  # A rescue card has to sit in the slot of a device that already has a system
  # on eMMC. The sd-image module derives the root device from the volume label,
  # as /dev/disk/by-label/<rootVolumeLabel>, so two roots sharing a label leave
  # /dev/disk/by-label holding one symlink and stage-1 mounts whichever device
  # udev settled last. Give the card its own label so each system resolves only
  # its own root while keeping the complete product composition.
  rescueConfiguration = configuration.extendModules {
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
  registryCheck =
    pkgs:
    import ./nixpkgs-registry-check.nix {
      inherit pkgs configuration;
    };
  diagnosticsCheck =
    pkgs:
    import ./diagnostics-check.nix {
      inherit pkgs configuration;
    };
  audioCheck =
    pkgs:
    import ./audio-check.nix {
      inherit pkgs configuration;
    };
  bluetoothCheck =
    pkgs:
    import ./bluetooth-check.nix {
      inherit pkgs configuration;
    };
}
