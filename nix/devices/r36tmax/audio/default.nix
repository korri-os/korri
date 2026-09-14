# ALSA 1.2.14 src/ucm/utils.c:337-350 consumes ALSA_CONFIG_UCM2.
# Keep the whole native data tree so absolute UCM includes and card discovery
# resolve normally. Only rk817-sound/HiFi.conf differs from the pinned package.
{ pkgs, ... }:
let
  ucm = pkgs.callPackage ./ucm.nix { };
  environment = {
    ALSA_CONFIG_UCM2 = "${ucm}/share/alsa/ucm2";
  };
in
{
  # Same session + service seams as NixOS hardware.alsa's native device vars.
  # This module is imported only by R36T Max. It starts no sound server and
  # changes no mixer level. A process bypassing UCM bypasses this guard.
  environment.sessionVariables = environment;
  systemd.globalEnvironment = environment;
}
