# Two real systems share the same SD hardware and device policy. The console
# system remains a recovery baseline; only the candidate starts the Korri stack.
{ nixpkgs, korri }:
let
  bootchain = import ./bootloader { inherit nixpkgs; };
  consoleConfiguration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = { inherit korri; };
    modules = [
      ./sd-image.nix
      ../../base
      ../../device-cache/nixos-module.nix
      (
        { lib, pkgs, ... }:
        {
          # The existing hardware image predates nix/base's state version. This
          # is a new image, not an in-place migration of an installed system.
          system.stateVersion = lib.mkForce "25.11";
          environment.systemPackages = [ (pkgs.callPackage ./session-sanity.nix { }) ];
        }
      )
    ];
  };
  configuration = consoleConfiguration.extendModules {
    modules = [
      (import ../../../services/inputd/nix/korri-linux-host.nix { inherit korri; })
      (import ../../../clients/portal/nix/nixos-module.nix { inherit korri; })
      ./portal.nix
    ];
  };
  # Owner-operated image only. Keys and Wi-Fi credentials are provisioned on
  # the card, never included in a public image or Nix store derivation.
  diagnosticConfiguration = configuration.extendModules {
    modules = [
      (
        { lib, ... }:
        {
          services.openssh.enable = lib.mkForce true;
          services.openssh.openFirewall = lib.mkForce true;
        }
      )
    ];
  };
  # Keep the new loader a separate boot-test image until cold/warm boots pass.
  mainlineConfiguration = consoleConfiguration.extendModules {
    modules = [
      (
        { lib, pkgs, ... }:
        {
          image.baseName = lib.mkForce "nixos-r36t-max-mainline";
          sdImage.populateFirmwareCommands = lib.mkForce ":";
          sdImage.postBuildCommands = lib.mkForce ''
            loader=${bootchain.uboot}/u-boot-rockchip.bin
            bytes=$(${pkgs.coreutils}/bin/stat -c %s "$loader")
            test "$bytes" -le $((16 * 1024 * 1024 - 64 * 512))
            dd if="$loader" of="$img" bs=512 seek=64 conv=notrunc
            dd if="$img" of=loader-readback bs=1M iflag=skip_bytes,count_bytes \
              skip=$((64 * 512)) count="$bytes" status=none
            ${pkgs.diffutils}/bin/cmp "$loader" loader-readback
          '';
        }
      )
    ];
  };
  rkMppServiceModule =
    configuration.config.boot.kernelPackages.callPackage ./mpp/rk-mpp-service-module.nix
      { };
in
{
  inherit rkMppServiceModule;
  inherit
    configuration
    consoleConfiguration
    diagnosticConfiguration
    mainlineConfiguration
    ;
  inherit (bootchain) uboot;
  diagnosticSdImage = diagnosticConfiguration.config.system.build.sdImage;
  mainlineSdImage = mainlineConfiguration.config.system.build.sdImage;
  sdImage = configuration.config.system.build.sdImage;
  consoleSdImage = consoleConfiguration.config.system.build.sdImage;
  mppCompileCheck = configuration.config.boot.kernelPackages.callPackage ./mpp/compile-check.nix {
    inherit rkMppServiceModule;
  };
  mppBindingCheck = pkgs: import ./mpp/binding-check.nix { inherit pkgs; };
  kernel = configuration.config.boot.kernelPackages.kernel;
  # Host-side preflight only. Devices download complete prebuilt generations.
  kernelCross =
    nixpkgs.legacyPackages.x86_64-linux.pkgsCross.aarch64-multiplatform.callPackage
      ./dts/kernel-trimmed.nix
      { };
  registryCheck =
    pkgs:
    import ./nixpkgs-registry-check.nix {
      inherit pkgs;
      configurations = [
        configuration
        consoleConfiguration
        diagnosticConfiguration
        mainlineConfiguration
      ];
    };
  moduleCheck =
    pkgs:
    import ./module-check.nix {
      inherit
        pkgs
        configuration
        consoleConfiguration
        diagnosticConfiguration
        mainlineConfiguration
        ;
    };
}
