# Proposed catalogue input to the shared builder.
# Explicit library paths; no pname -> filename guessing.
{ pkgs }:
{
  mgba = {
    core = pkgs.libretro.mgba;
    core-file = "${pkgs.libretro.mgba}/lib/retroarch/cores/mgba_libretro.so";
    systems.gba = {
      title = "Game Boy Advance";
      extensions = [ "gba" ];
    };
    plugin = ./cores/mgba/plugin.ts;
  };
  snes9x = {
    core = pkgs.libretro.snes9x;
    core-file = "${pkgs.libretro.snes9x}/lib/retroarch/cores/snes9x_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "sfc"
        "smc"
      ];
    };
  };
}
