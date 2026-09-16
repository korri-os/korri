{ pkgs }:
let
  mkPlugin = import ../../services/korrid/plugin-host/builder.nix { inherit pkgs; };
  retroarch = mkPlugin {
    publisher.namespace = "@korri";
    source = ../retroarch;
    plugin = ../retroarch/plugin.nix;
  };
in
{
  packages.mgba = pkgs.libretro.mgba;
  files.mgba = "${pkgs.libretro.mgba}/lib/retroarch/cores/mgba_libretro.so";
  requires = [ retroarch ];
}
