# RG353M opt-in for the existing Linux RetroArch executor.
{ pkgs, ... }:
{
  imports = [ ./gba-library-access.nix ];

  # Native device testing crossed 68 C at the previous 1.8 GHz ceiling.
  # Keep schedutil; the 1.104 GHz ceiling sustained GBA below 60 C.
  powerManagement.cpufreq.max = 1104000;

  services.korriLinuxHost.deviceConfig = pkgs.writeText "korrid-rg353m-gba-host.toml" ''
    label = "rg353m"
    [environment]
    DISPLAY = ":0"
    XDG_SESSION_TYPE = "x11"
    PULSE_SERVER = "unix:/run/korri-game-audio/native"
  '';
}
