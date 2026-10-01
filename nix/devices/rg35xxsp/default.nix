# Anbernic RG35XX SP: the H700 family product with this board's facts.
{ nixpkgs, korri }:
let
  h700 = import ../h700 { inherit nixpkgs korri; };
  configuration = h700.mkConfiguration {
    device = "rg35xxsp";
    module = ./hardware.nix;
  };
  # ROCKNIX ships two SP trees because two panels exist and nothing on the
  # outside tells them apart ("try the other DTB"; rocknix.org H700
  # installation guide). The v2-panel tree changes only the panel's init file.
  # Same system, same labels; only the device tree and image name differ.
  v2PanelConfiguration = configuration.extendModules {
    modules = [
      (
        { lib, ... }:
        {
          hardware.deviceTree = {
            filter = lib.mkForce "sun50i-h700-anbernic-rg35xx-sp-v2-panel.dtb";
            name = lib.mkForce "allwinner/sun50i-h700-anbernic-rg35xx-sp-v2-panel.dtb";
          };
          image.baseName = lib.mkForce "nixos-rg35xxsp-v2-panel";
        }
      )
    ];
  };
in
{
  inherit configuration v2PanelConfiguration;
  sdImage = configuration.config.system.build.sdImage;
  v2PanelSdImage = v2PanelConfiguration.config.system.build.sdImage;
}
