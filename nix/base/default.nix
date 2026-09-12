# Shared policy for the RG353M and Odin 2 Portal configurations.
# This module chooses neither a board nor an image format.
{ pkgs, ... }:
let
  korriCache = import ../cache/identity.nix;
in
{
  imports = [ ./wifi.nix ];

  networking.networkmanager.enable = true;

  # Keep physical recovery access, including the RG353M USB serial console.
  # Physical console access grants root. Network SSH is off for image releases;
  # owner-controlled SSH through the plugin host remains separate work.
  services.getty.autologinUser = "root";
  services.openssh = {
    enable = false;
    openFirewall = false;
    settings = {
      KbdInteractiveAuthentication = false;
      PasswordAuthentication = false;
      PermitRootLogin = "prohibit-password";
    };
  };

  users.users.root.initialHashedPassword = "";

  environment.systemPackages = [ pkgs.iw ];
  documentation.enable = false;

  # Korri's own outputs are not on any public cache, and devices may not build
  # them: nix/device-cache/nixos-module.nix forces max-jobs = 0 and
  # fallback = false. Without a Korri cache a device generation can only be
  # delivered by rewriting the card. korri-os/nix-cache serves those outputs
  # over the standard cache protocol; see nix/cache/README.md.
  #
  # Each builder signs with its own key, so a machine can be revoked by
  # removing one line here. require-sigs stays true and no signature bypass is
  # permitted; an unsigned path fails the download instead of being built.
  # nixpkgs validates a generated nix.conf by building a check derivation for
  # the target system. These configurations are aarch64, so adding any
  # substituter makes every x86_64 evaluation of a device config require an
  # aarch64 builder; without one, Nix 2.31 fails with the misleading
  # "experimental Nix feature 'dynamic-derivations' is disabled". The check
  # was run once against the real conf on fuji and passed. Turn it off so the
  # layout check and CI stay self-contained on x86_64, and re-enable it when
  # aarch64 build capacity is configured everywhere that evaluates these.
  nix.checkConfig = false;

  nix.settings = {
    # This list is also the publisher's filter list: nix/cache/upload-staged.sh
    # treats every entry except Korri's own as a cache it may skip. Add one here
    # and the publisher widens in the same change, by construction.
    substituters = [
      "https://cache.nixos.org/"
      korriCache.url
    ];
    trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
    ]
    ++ korriCache.publicKeys;
    experimental-features = [
      "nix-command"
      "flakes"
    ];
  };
  system.stateVersion = "25.11";
}
