# Product boot on every device: the Korri splash over a quiet kernel, and a
# verbose development boot kept beside it (owner decisions 2026-09-30).
#
# A device supplies only services.korri.bootSplash.refreshRate, a panel fact.
# The development boot is a NixOS specialisation, so every generation carries
# it. How a person selects it at power-on is a separate, per-loader question.
{ lib, ... }:
{
  imports = [ ../../brand/plymouth/nixos-module.nix ];

  services.korri.bootSplash = {
    enable = true;
    # The development boot below is the debugging channel, so the normal boot
    # can hide kernel and initrd text behind the splash.
    quiet = true;
  };

  specialisation.development.configuration = {
    system.nixos.tags = [ "development" ];
    # Text on the panel from the first kernel line, as the RG353M booted
    # before 2026-09-30: no splash, kernel log level 7, verbose initrd.
    services.korri.bootSplash.enable = lib.mkForce false;
    boot.consoleLogLevel = lib.mkOverride 40 7;
    # NixOS appends loglevel=7 after any device "quiet"; the kernel applies
    # the last level it reads.
    boot.initrd.verbose = lib.mkOverride 40 true;
  };
}
