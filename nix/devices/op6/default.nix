# OnePlus 6 (enchilada, SDM845) with the Korri product.
#
# The hardware comes from the mwlaboratories Mobile NixOS fork, as on the
# phoneputer system the phone ran before (phoneputer ea5810e): kernel
# 7.4.0-rc3 (sdm845-linus), firmware, modem, Wi-Fi and the Android boot.img
# chain. GNOME, Stylix, Home Manager and Nixvim stay out. On top go the Korri
# product, its image plugins and ./hardware.nix.
{
  nixpkgs,
  op6-nixpkgs,
  op6-mobile-nixos,
  korri,
  ...
}:
let
  hostSystem = "x86_64-linux";
  hostPkgs = nixpkgs.legacyPackages.${hostSystem};

  mobileModule = import "${op6-mobile-nixos}/lib/configuration.nix" {
    device = "oneplus-enchilada";
  };

  # The kernel the phone runs today. Mobile NixOS builds the device kernel
  # from the system's own nixpkgs, which is Korri's here. Evaluated on the
  # nixpkgs phoneputer used, with only the fork's own modules, it is the same
  # store path as /run/booted-system/kernel on the phone
  # (1iq92v5m...-linux-aarch64-unknown-linux-gnu-7.4.0-rc3), so nothing
  # rebuilds and the boot.img in boot_a/boot_b stays valid. It already has
  # Korri's kernel delta (uinput, hidraw, uhid, joystick and HID drivers,
  # nf_tables; README.md, "Kernel").
  kernelBase = op6-nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    modules = [
      mobileModule
      {
        mobile.boot.stage-1.kernel.buildOn = hostSystem;
        nixpkgs.config.allowUnfree = true;
      }
    ];
  };
  kernel = kernelBase.config.mobile.boot.stage-1.kernel.package;

  configuration = nixpkgs.lib.nixosSystem {
    system = "aarch64-linux";
    specialArgs = {
      inherit korri;
      op6Kernel = kernel;
    };
    modules = [
      mobileModule
      (import ../../product/nixos-module.nix { inherit korri; })
      (import ../../product/image-plugins.nix {
        inherit korri;
        device = "op6";
      })
      ./plugin-seed.nix
      ./hardware.nix
    ];
  };

  deploy = hostPkgs.writeShellApplication {
    name = "op6-deploy";
    runtimeInputs = [
      hostPkgs.openssh
      hostPkgs.gnutar
      hostPkgs.gnugrep
    ];
    text = ''
      toplevel=${configuration.config.system.build.toplevel}
      bundle=${configuration.config.services.korriBundle.initialPackage}
      plugin_seed=${configuration.config.system.build.op6PluginSeed}
      sign_key="$HOME/.config/korri/cache-key.secret"
      default_target=root@10.42.9.1
      default_port=2222
      ${builtins.readFile ./deploy.sh}
    '';
  };
in
{
  inherit configuration kernel;
  packages = {
    op6-toplevel = configuration.config.system.build.toplevel;
    op6-kernel = kernel;
    op6-deploy = deploy;
  };
}
