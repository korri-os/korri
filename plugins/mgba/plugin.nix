{ pkgs }:
let
  frontend = import ../retroarch/plugin.nix { inherit pkgs; };
in
{
  packages = frontend.packages // {
    mgba = pkgs.libretro.mgba;
  };
  files = frontend.files // {
    mgba = "${pkgs.libretro.mgba}/lib/retroarch/cores/mgba_libretro.so";
  };
}
