{
  description = "Korri";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    inputplumber-nixpkgs.url = "github:NixOS/nixpkgs/9a37a7b2ae651b6182ef08d0d446a964339bcdfe";
    flake-utils.url = "github:numtide/flake-utils";
    # The publisher imports Korri for its builder. Read its source without
    # evaluating its flake here, so the image does not create a flake cycle.
    plugins = {
      url = "github:korri-os/plugins";
      flake = false;
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    proseql = {
      url = "github:simonwjackson/proseql/7ba57cf17c01b15ccdb030237a96b6376a349253";
      flake = false;
    };
  };

  # This flake is an index: it wires inputs and composes per-area nix
  # expressions that live next to the code they serve. No derivations or
  # shells are defined inline here.
  outputs =
    {
      self,
      nixpkgs,
      inputplumber-nixpkgs,
      flake-utils,
      plugins,
      rust-overlay,
      crane,
      proseql,
    }:
    let
      rg353m = import ./nix/devices/rg353m {
        inherit nixpkgs plugins;
        korri = self;
      };
      rgds = import ./nix/devices/rgds {
        inherit nixpkgs plugins;
        korri = self;
      };
      r36tmax = import ./nix/devices/r36tmax {
        inherit nixpkgs plugins;
        korri = self;
      };
      rpminiv2 = import ./nix/devices/rpminiv2 {
        inherit nixpkgs plugins;
        korri = self;
      };
      odin2portal = import ./nix/devices/odin2portal {
        inherit nixpkgs plugins;
        korri = self;
      };
      rg35xxsp = import ./nix/devices/rg35xxsp {
        inherit nixpkgs;
        korri = self;
      };
      nixosModules = {
        korri-product = import ./nix/product/nixos-module.nix { korri = self; };
        korri-device-cache = import ./nix/device-cache/nixos-module.nix;
        korri-bundle = import ./services/inputd/nix/korri-bundle-module.nix { korri = self; };
        korri-input = import ./services/inputd/nix/korri-input.nix { korri = self; };
        korrid-linux-device = import ./services/korrid/nixos-module.nix { korri = self; };
        korri-linux-host = import ./services/inputd/nix/korri-linux-host.nix { korri = self; };
        korri-portal = import ./clients/portal/nix/nixos-module.nix { korri = self; };
        korri-plugin-host = import ./services/korrid/plugin-host/nixos-module.nix { korri = self; };
      };
    in
    {
      inherit nixosModules;
      # Where Korri's own signed cache lives. Device configurations consume it as
      # a substituter and the publisher reads it here; see nix/cache/identity.nix.
      cache = import ./nix/cache/identity.nix;
      nixosConfigurations = {
        # nixos-rebuild resolves nixosConfigurations.<hostname> when no target is
        # named, and the host is rg353m. This name therefore has to mean the
        # device as it ships with the complete shared product.
        rg353m = rg353m.configuration;
        rg353m-rescue = rg353m.rescueConfiguration;
        rgds = rgds.configuration;
        rpminiv2 = rpminiv2.configuration;
        r36tmax = r36tmax.configuration;
        odin2portal = odin2portal.configuration;
        # RG35XXSP remains a product device. It returns here after its product
        # composition and delivery land; its packages stay available for bring-up.
      };
    }
    // flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
          config = {
            allowUnfree = true;
          };
        };
        pluginHost = import ./services/korrid/plugin-host {
          inherit pkgs crane korridPackage;
          inputdPackage = inputplumber.packages.korri-inputd;
          sunshinePackage =
            if sunshineV4l2m2mPackage != null then sunshineV4l2m2mPackage else sunshinePackage;
          hostModule = nixosModules.korri-plugin-host;
        };
        korridPackage = import ./services/korrid/package.nix {
          inherit pkgs proseql crane;
        };
        sunshinePackage = pkgs.callPackage ./services/sunshine/package.nix {
          sunshine = pkgs.sunshine;
          cudaSupport = system == "x86_64-linux";
        };
        sunshineV4l2m2mPackage =
          if system == "aarch64-linux" then
            let
              rockchipMpp = pkgs.callPackage ./services/sunshine/rockchip-mpp.nix { };
              ffmpegArm = pkgs.callPackage ./services/sunshine/ffmpeg-rkmpp-static.nix {
                inherit rockchipMpp;
              };
            in
            pkgs.callPackage ./services/sunshine/package.nix {
              sunshine = pkgs.sunshine;
              cudaSupport = false;
              rkmppSupport = true;
              ffmpegRkmpp = ffmpegArm;
              ffmpegV4l2m2m = ffmpegArm;
              inherit rockchipMpp;
              libdrm = pkgs.libdrm;
            }
          else
            null;
        inputplumber = import ./services/inputd/nix {
          inherit
            pkgs
            system
            crane
            korridPackage
            sunshinePackage
            sunshineV4l2m2mPackage
            ;
          inputplumberNixpkgs = inputplumber-nixpkgs;
          korriBundleModule = nixosModules.korri-bundle;
          korriInputModule = nixosModules.korri-input;
          korridLinuxDeviceModule = nixosModules.korrid-linux-device;
          korriLinuxHostModule = nixosModules.korri-linux-host;
        };
        tasks = import ./nix/tasks.nix {
          inherit pkgs proseql;
          extraHelpText = pkgs.lib.optionalString pkgs.stdenv.isLinux ''

            nix run .#korri-dev -- [--physical]
                Run isolated korrid and inputd development processes without host mutation.
            nix run .#korri-bundle-select -- COMMAND
                Select or roll back one immutable Korri bundle without NixOS activation.
            ${pluginHost.help}'';
        };
      in
      {
        apps = tasks // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux (inputplumber.apps // pluginHost.apps);
        devShells.default = import ./devshell.nix {
          inherit pkgs;
          hooksInstall = tasks.hooks-install.program;
        };
        devShells.portal = import ./clients/portal/devshell.nix { inherit pkgs; };
        devShells.korrid = import ./services/korrid/devshell.nix { inherit pkgs proseql; };
        devShells.inputd = import ./services/inputd/devshell.nix { inherit pkgs; };
        devShells.plugin-host = pluginHost.devShell;
        lib = pluginHost.lib;
        packages = {
          korrid = korridPackage;
          korri-portal = import ./clients/portal/nix/package.nix { inherit pkgs; };
          default = self.packages.${system}.korrid;
        }
        // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux (
          inputplumber.packages
          // pluginHost.packages
          // {
            rg353m-inputplumber-data = rg353m.inputplumberData pkgs inputplumber.packages.inputplumber-korri;
            rgds-inputplumber-data = rgds.inputplumberData pkgs inputplumber.packages.inputplumber-korri;
            rpminiv2-inputplumber-data = rpminiv2.inputplumberData pkgs inputplumber.packages.inputplumber-korri;
            korri-portal-shell = import ./clients/linux/package.nix { inherit pkgs; };
            korri-plymouth-theme = pkgs.callPackage ./brand/plymouth/package.nix { };
            korri-chromium = import ./clients/linux/chromium/package.nix { inherit pkgs; };
          }
        )
        // pkgs.lib.optionalAttrs (system == "aarch64-linux") {
          rg353m-sd-image = rg353m.sdImage;
          rg353m-rescue-sd-image = rg353m.rescueSdImage;
          rg353m-uboot = rg353m.uboot;
          rgds-sd-image = rgds.sdImage;
          rgds-kernel = rgds.kernel;
          rgds-uboot = rgds.uboot;
          rpminiv2-sd-image = rpminiv2.sdImage;
          rpminiv2-recovery-sd-image = rpminiv2.consoleSdImage;
          rpminiv2-kernel = rpminiv2.kernel;
          rpminiv2-recovery-kernel = rpminiv2.recoveryKernel;
          rpminiv2-firmware = rpminiv2.firmware;
          r36tmax-sd-image = r36tmax.sdImage;
          r36tmax-rk-mpp-service-module = r36tmax.rkMppServiceModule;
          r36tmax-recovery-sd-image = r36tmax.consoleSdImage;
          r36tmax-kernel = r36tmax.kernel;
          r36tmax-uboot = r36tmax.uboot;
          r36tmax-diagnostic-sd-image = r36tmax.diagnosticSdImage;
          r36tmax-mainline-sd-image = r36tmax.mainlineSdImage;
          odin2portal-kernel = odin2portal.kernel;
          odin2portal-rescue-kernel = odin2portal.rescueKernel;
          odin2portal-firmware = odin2portal.firmware;
          odin2portal-sd-image = odin2portal.sdImage;
          odin2portal-gamescope = odin2portal.rocknix.gamescope;
          odin2portal-mangohud = odin2portal.rocknix.mangohud;
          odin2portal-alsa-lib = odin2portal.rocknix.alsaLib;
          odin2portal-inputplumber-data = odin2portal.rocknix.inputplumberData;
          rg35xxsp-sd-image = rg35xxsp.sdImage;
          rg35xxsp-kernel = rg35xxsp.kernel;
          rg35xxsp-uboot = rg35xxsp.uboot;
        }
        // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          korri-chromium-aarch64 = import ./clients/linux/chromium/cross.nix { inherit pkgs; };
          rpminiv2-kernel = rpminiv2.kernelCross;
          rpminiv2-recovery-kernel = rpminiv2.recoveryKernelCross;
          rpminiv2-kernel-source-gcc15 = rpminiv2.kernelSourceGcc15;
          rpminiv2-firmware = rpminiv2.firmwareCross;
          rpminiv2-rocknix-baseline = rpminiv2.rocknixBaseline;
          rgds-kernel = rgds.kernelCross;
          r36tmax-kernel = r36tmax.kernelCross;
          rgds-uboot = rgds.ubootCross;
          odin2portal-kernel = odin2portal.kernelCross;
          odin2portal-rescue-kernel = odin2portal.rescueKernelCross;
          odin2portal-firmware = odin2portal.firmwareCross;
          rg35xxsp-uboot = rg35xxsp.ubootCross;
          rg35xxsp-kernel = rg35xxsp.kernelCross;
        };
        checks = pkgs.lib.optionalAttrs pkgs.stdenv.isLinux (
          inputplumber.checks
          // pluginHost.checks
          // {
            korri-product = import ./nix/product/check.nix {
              inherit pkgs nixpkgs;
              korri = self;
              productModule = nixosModules.korri-product;
              configurations = self.nixosConfigurations;
            };
            korri-product-module = import ./nix/product/module-check.nix {
              inherit pkgs nixpkgs;
              korri = self;
              productModule = nixosModules.korri-product;
            };
            korri-product-image-plugins = import ./nix/product/image-plugins-check.nix {
              inherit pkgs plugins;
              korri = self;
              devices = { inherit rg353m rgds r36tmax rpminiv2 odin2portal; };
            };
            korri-device-cache = import ./nix/device-cache/module-check.nix {
              inherit pkgs;
              cacheModule = nixosModules.korri-device-cache;
            };
            korri-base = import ./nix/base/module-check.nix {
              inherit pkgs nixpkgs;
              korri = self;
            };
            korri-sd-card = import ./nix/formats/sd-card-check.nix { inherit pkgs nixpkgs; };
            korri-image-dist = import ./nix/formats/image-dist-check.nix { inherit pkgs; };
            korri-inputplumber-data = import ./nix/base/inputplumber-data-check.nix {
              inherit pkgs;
              inputplumber = inputplumber.packages.inputplumber-korri;
              dataPackage = self.packages.${system}.rg353m-inputplumber-data;
            };
            rg353m-usb-gadget = rg353m.usbGadgetCheck pkgs;
            rg353m-audio = rg353m.audioCheck pkgs;
            rg353m-diagnostics = rg353m.diagnosticsCheck pkgs;
            rg353m-registry = rg353m.registryCheck pkgs;
            rg353m-bluetooth = rg353m.bluetoothCheck pkgs;
            rg353m-firmware = import ./nix/devices/rg353m/firmware-check.nix {
              inherit pkgs;
              korri = self;
            };
            rgds = rgds.moduleCheck pkgs;
            rg35xxsp = rg35xxsp.moduleCheck pkgs;
            rpminiv2 = rpminiv2.moduleCheck pkgs;
            rpminiv2-delivery = import ./nix/devices/rpminiv2/delivery-check.nix { inherit pkgs; };
            rpminiv2-inputplumber = self.packages.${system}.rpminiv2-inputplumber-data;
            rpminiv2-initrd-modules = rpminiv2.initrdModulesCheck pkgs;
            rpminiv2-recovery-initrd-modules = rpminiv2.recoveryInitrdModulesCheck pkgs;
            r36tmax = r36tmax.moduleCheck pkgs;
            r36tmax-registry = r36tmax.registryCheck pkgs;
            r36tmax-mpp-binding = r36tmax.mppBindingCheck pkgs;
            r36tmax-recovery = (import ./nix/devices/r36tmax/recovery { inherit pkgs; }).checks;
            r36tmax-mmc = import ./nix/devices/r36tmax/wifi/mmc-check.nix { inherit pkgs; };
            rgds-inputplumber = self.packages.${system}.rgds-inputplumber-data;
            rgds-initrd-modules = rgds.initrdModulesCheck pkgs;
            rg353m-inputplumber = self.packages.${system}.rg353m-inputplumber-data;
            odin2portal = odin2portal.moduleCheck pkgs;
            odin2portal-inputplumber = odin2portal.inputplumberData pkgs inputplumber.packages.inputplumber-korri;
            korri-portal-module = import ./clients/portal/nix/module-check.nix {
              inherit pkgs nixpkgs;
              korri = self;
            };
            korri-boot-splash = import ./brand/plymouth/module-check.nix {
              inherit pkgs nixpkgs;
            };
          }
          // pkgs.lib.optionalAttrs (system == "aarch64-linux") {
            r36tmax-mpp-compile = r36tmax.mppCompileCheck;
          }
        );
      }
    );
}
