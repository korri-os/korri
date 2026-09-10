# Shared policy for the RG353M and Odin 2 Portal configurations.
# This module chooses neither a board nor an image format.
{ pkgs, ... }:

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
  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];
  system.stateVersion = "25.11";
}
