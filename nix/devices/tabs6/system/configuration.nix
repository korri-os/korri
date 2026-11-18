# NixOS for the Galaxy Tab S6 Wi-Fi (SM-T860, gts6lwifi).
#
# Moved from the tabs6 repository (nix/system, commit 97c2734). On its own this
# is the tabs6 Linux console: sway + foot + tmux, the RNDIS/ACM lifeline,
# iwd Wi-Fi. tabs6.console.enable turns the console's own session, lifeline
# and network stack off, so the Korri product (../hardware.nix) can supply
# its own; the board support (kernel, boot, firmware, modem, audio, charge)
# stays in both.
{ config, lib, ... }:
{
  imports = [
    ./kernel.nix
    ./boot.nix
    ./lifeline.nix
    ./network.nix
    ./console.nix
    ./device.nix
    ./plugin-seed.nix
    ../usb-role/module.nix
  ];

  options.tabs6.console.enable = lib.mkOption {
    type = lib.types.bool;
    default = true;
    description = ''
      The tabs6 Linux console: simon's sway session, the RNDIS + ACM lifeline
      with sshd, standalone iwd with networkd on wlan0, idle suspend and the
      power key that blanks the screen. The Korri product turns it off and
      supplies the shell, the product USB gadget, NetworkManager and SSH.
    '';
  };

  config = lib.mkMerge [
    {
      networking.hostName = "tabs6";
      time.timeZone = "America/Denver";
      i18n.defaultLocale = "en_US.UTF-8";

      nix.settings = {
        experimental-features = [
          "nix-command"
          "flakes"
        ];
        trusted-users = [ "root" ];
      };
      # Never build on the tablet: it only receives closures.
      nix.settings.max-jobs = 0;
      nix.gc = {
        automatic = true;
        dates = "weekly";
        options = "--delete-older-than 14d";
      };

      documentation.enable = false;
      documentation.nixos.enable = false;
      services.xserver.enable = false;
      environment.defaultPackages = lib.mkForce [ ];
      # The closure does not need a copy of nixpkgs.
      nixpkgs.flake.setNixPath = false;
      nixpkgs.flake.setFlakeRegistry = false;
    }
    (lib.mkIf config.tabs6.console.enable {
      # programs.sway turns on the graphical-desktop defaults: speech synthesis
      # (~0.8 GiB with voices), GTK portals and the default font set. The console
      # uses none of them.
      services.speechd.enable = lib.mkForce false;
      xdg.portal.enable = lib.mkForce false;
      fonts.enableDefaultPackages = lib.mkForce false;

      system.stateVersion = "26.11";
    })
  ];
}
