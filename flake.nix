{
  description = "Korri";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    inputplumber-nixpkgs.url = "github:NixOS/nixpkgs/9a37a7b2ae651b6182ef08d0d446a964339bcdfe";
    flake-utils.url = "github:numtide/flake-utils";
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
      rust-overlay,
      crane,
      proseql,
    }:
    let
      rg353m = import ./nix/rg353m {
        inherit nixpkgs;
        korri = self;
      };
      odin2portal = import ./nix/odin2portal { inherit nixpkgs; };
      nixosModules = {
        korri-bundle = import ./services/inputd/nix/korri-bundle-module.nix { korri = self; };
        korri-input = import ./services/inputd/nix/korri-input.nix { korri = self; };
        korrid-linux-device = import ./services/korrid/nixos-module.nix { korri = self; };
        korri-linux-host = import ./services/inputd/nix/korri-linux-host.nix { korri = self; };
      };
    in
    {
      inherit nixosModules;
      nixosConfigurations = {
        rg353m = rg353m.configuration;
        odin2portal = odin2portal.configuration;
      };
    }
    // flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
          # Required by the Android SDK composition in clients/android.
          config = {
            android_sdk.accept_license = true;
            allowUnfree = true;
          };
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
            pkgs.callPackage ./services/sunshine/package.nix {
              sunshine = pkgs.sunshine;
              cudaSupport = false;
              ffmpegV4l2m2m = pkgs.callPackage ./services/sunshine/ffmpeg-v4l2m2m-static.nix { };
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
      in
      {
        apps =
          (import ./nix/tasks.nix {
            inherit pkgs proseql;
            extraHelpText = pkgs.lib.optionalString pkgs.stdenv.isLinux ''

              nix run .#korri-dev -- [--physical]
                  Run isolated korrid and inputd development processes without host mutation.
              nix run .#korri-bundle-select -- COMMAND
                  Select or roll back one immutable Korri bundle without NixOS activation.'';
          })
          // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux inputplumber.apps;
        devShells.android = import ./clients/android/devshell.nix { inherit pkgs; };
        devShells.portal = import ./clients/portal/devshell.nix { inherit pkgs; };
        devShells.korrid = import ./services/korrid/devshell.nix { inherit pkgs proseql; };
        devShells.inputd = import ./services/inputd/devshell.nix { inherit pkgs; };
        devShells.retroarch = import ./plugins/retroarch/android/devshell.nix { inherit pkgs; };
        packages = {
          korrid = korridPackage;
          default = self.packages.${system}.korrid;
        }
        // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux inputplumber.packages
        // pkgs.lib.optionalAttrs (system == "aarch64-linux") {
          rg353m-sd-image = rg353m.sdImage;
          rg353m-uboot = rg353m.uboot;
          odin2portal-kernel = odin2portal.kernel;
          odin2portal-rescue-kernel = odin2portal.rescueKernel;
          odin2portal-firmware = odin2portal.firmware;
          odin2portal-sd-image = odin2portal.sdImage;
          odin2portal-gamescope = odin2portal.rocknix.gamescope;
          odin2portal-mangohud = odin2portal.rocknix.mangohud;
          odin2portal-alsa-lib = odin2portal.rocknix.alsaLib;
          odin2portal-inputplumber-data = odin2portal.rocknix.inputplumberData;
        }
        // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          odin2portal-kernel = odin2portal.kernelCross;
          odin2portal-rescue-kernel = odin2portal.rescueKernelCross;
          odin2portal-firmware = odin2portal.firmwareCross;
        };
        checks = pkgs.lib.optionalAttrs pkgs.stdenv.isLinux (
          inputplumber.checks
          // {
            rg353m-usb-gadget = rg353m.usbGadgetCheck pkgs;
          }
        );
      }
    );
}
