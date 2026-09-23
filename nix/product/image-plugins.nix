# Fresh SD images own the selected plugin closures and receipts. Installed
# systems keep no package reference; the plugin host's GC roots alone own
# selections after first boot.
{ korri, plugins, device }:
{ pkgs, lib, ... }:
let
  system = pkgs.stdenv.hostPlatform.system;
  selection = import ./plugin-selection.nix {
    plugins = plugins.packages.${system};
  };
  seeded = import ../../services/korrid/plugin-host/image-seed.nix {
    inherit pkgs;
    hostPackage = korri.packages.${system}.korri-plugin-host;
    cacheUrl = (import ./requirements.nix { inherit korri; }).constants.publishers."@korri".cacheUrl;
    pluginPackages = selection.${device};
  };
in
{
  sdImage.storePaths = seeded.storePaths;
  sdImage.populateRootCommands = lib.mkAfter seeded.populateRootCommands;
}
