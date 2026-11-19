# Samsung Galaxy Tab S6 Wi-Fi (SM-T860, gts6lwifi, SM8150) systems. The
# console system is the tabs6 Linux console that boots on the tablet today;
# the default configuration extends it with the Korri product session.
#
# Moved from the tabs6 repository at commit 97c2734 (README.md, "Where it
# came from").
{
  nixpkgs,
  tabs6-nixpkgs,
  korri,
  ...
}:
let
  hostSystem = "x86_64-linux";

  # The kernel and the device firmware keep the package set they were proven
  # with on the tablet (tabs6-nixpkgs, Linux 7.3.4). With the same inputs the
  # console kernel is the same derivation the tabs6 repository builds.
  kernelHostPkgs = tabs6-nixpkgs.legacyPackages.${hostSystem};
  kernelCrossPkgs = import tabs6-nixpkgs {
    localSystem = hostSystem;
    crossSystem = "aarch64-linux";
  };
  # The firmware and the images that carry it stay on the build host: the
  # stock members are proprietary and never go to a binary cache.
  firmwareHostPkgs = import tabs6-nixpkgs {
    system = hostSystem;
    config.allowUnfreePredicate =
      pkg:
      builtins.elem (pkg.name or "") [
        "tabs6-device-firmware"
        "NON-HLOS.bin.lz4"
        "modem.bin.lz4"
        "vendor.img.ext4.lz4"
      ];
  };
  # Host tools (image assembly, deploy) use Korri's package set.
  hostPkgs = nixpkgs.legacyPackages.${hostSystem};

  consoleKernel =
    (import ./kernel {
      pkgs = kernelHostPkgs;
      cross = kernelCrossPkgs;
    }).kernel;
  korriKernel =
    (import ./kernel {
      pkgs = kernelHostPkgs;
      cross = kernelCrossPkgs;
      productFragment = ./kernel/config/korri.config;
    }).kernel;
  firmware = firmwareHostPkgs.callPackage ./firmware { };

  consoleConfiguration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = {
      inherit korri;
      # Cross-built on x86_64. Never compile on the tablet.
      tabs6Kernel = consoleKernel;
      tabs6KorriKernel = korriKernel;
    };
    modules = [ ./system/configuration.nix ];
  };
  configuration = consoleConfiguration.extendModules {
    modules = [
      (import ../../product/nixos-module.nix { inherit korri; })
      (import ../../product/image-plugins.nix {
        inherit korri;
        device = "tabs6";
      })
      ./hardware.nix
    ];
  };

  images =
    config: deployTarget:
    import ./images {
      inherit nixpkgs firmware deployTarget;
      pkgsHost = hostPkgs;
      inherit (config.config.system.build) toplevel tabs6PluginSeed;
      inherit (config.config.tabs6) placeholderKernel;
      pluginStorePaths = config.config.sdImage.storePaths;
    };
  productImages = images configuration {
    # The product requires signed paths (nix/device-cache). Sign on Zao with
    # its cache key, as nix/devices/system-update does; SSH is the product's
    # key-only SSH plugin on 2222 over the product gadget.
    signKey = "$HOME/.config/korri/cache-key.secret";
    target = "root@10.42.7.1";
    port = 2222;
  };
  consoleImages = images consoleConfiguration {
    # The console trusts Korri's cache keys (system/configuration.nix), so it
    # is signed like the product. A console that predates that trust still
    # needs a one-off `nix copy --no-check-sigs` of the closure first.
    signKey = "$HOME/.config/korri/cache-key.secret";
    target = "root@172.16.42.1";
    port = 22;
  };
in
{
  inherit
    configuration
    consoleConfiguration
    consoleKernel
    korriKernel
    firmware
    ;
  packages = {
    # The two aarch64 systems, for the build host that deploys them.
    tabs6-toplevel = configuration.config.system.build.toplevel;
    tabs6-console-toplevel = consoleConfiguration.config.system.build.toplevel;
    tabs6-kernel = consoleKernel;
    tabs6-korri-kernel = korriKernel;
    tabs6-firmware = firmware;
    tabs6-stage-firmware = hostPkgs.writeShellApplication {
      name = "tabs6-stage-firmware";
      runtimeInputs = [
        hostPkgs.gnutar
        hostPkgs.findutils
      ];
      text = builtins.readFile ./firmware/stage-stock-firmware.sh;
    };
    tabs6-ext4-cat = productImages.ext4-cat;
    tabs6-rootfs-image = productImages.rootfs;
    tabs6-esp-image = productImages.esp;
    tabs6-deploy-over-ssh = productImages.deploy;
    tabs6-console-rootfs-image = consoleImages.rootfs;
    tabs6-console-esp-image = consoleImages.esp;
    tabs6-console-deploy-over-ssh = consoleImages.deploy;
  };
  checks = {
    tabs6-esp = import ./esp/check.nix { pkgs = hostPkgs; };
    tabs6-usb-role = import ./usb-role/check.nix { pkgs = hostPkgs; };
  };
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
  systemdCheck =
    pkgs:
    import ../../product/systemd/check.nix {
      inherit pkgs configuration consoleConfiguration;
    };
}
