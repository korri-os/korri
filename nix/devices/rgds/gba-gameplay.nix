# RG DS opt-in for the existing Linux RetroArch executor.
#
# The plugin host installs RetroArch and the mGBA core; korrid launches them
# through its existing "@korri:retroarch" route. Nothing here names a plugin or
# approves one: installation and enablement stay explicit operator actions.
{ pkgs, ... }:
{
  imports = [ ./gba-library-access.nix ];

  services.korri.pluginHost.enable = true;

  # This device's compositor publishes an Xwayland display; audio is not yet
  # verified on this board, so no audio server is named here.
  services.korriLinuxHost.deviceConfig = pkgs.writeText "korrid-rgds-gba-host.toml" ''
    label = "rgds"
    [environment]
    DISPLAY = ":0"
    XDG_SESSION_TYPE = "x11"
  '';

  # The RG353M caps its clock because sustained GBA crossed 68 C there. This
  # board's thermals are unmeasured, so no ceiling is invented: measure under
  # load first, then set one with the number that measurement produces.
}
