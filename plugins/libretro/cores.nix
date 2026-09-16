# The libretro core catalogue. One entry produces one separate plugin package:
# its own id, its own runner, its own RetroArch inside its own closure.
#
# Every entry names its core library file exactly. A file name is never derived
# from a package name, because the two disagree often enough to matter:
# beetle-pce-fast ships mednafen_pce_fast_libretro.so.
#
# Extensions are conservative on purpose. An ambiguous extension such as "bin"
# belongs to several systems at once, so it is left out until a real library
# needs it.
{ pkgs }:
{
  mgba = {
    title = "mGBA";
    description = "Runs Game Boy Advance content with the mGBA libretro core.";
    core = pkgs.libretro.mgba;
    coreFile = "${pkgs.libretro.mgba}/lib/retroarch/cores/mgba_libretro.so";
    systems.gba = {
      title = "Game Boy Advance";
      extensions = [ "gba" ];
    };
  };

  gambatte = {
    title = "Gambatte";
    description = "Runs Game Boy and Game Boy Color content with the Gambatte libretro core.";
    core = pkgs.libretro.gambatte;
    coreFile = "${pkgs.libretro.gambatte}/lib/retroarch/cores/gambatte_libretro.so";
    systems = {
      gb = {
        title = "Game Boy";
        extensions = [ "gb" ];
      };
      gbc = {
        title = "Game Boy Color";
        extensions = [ "gbc" ];
      };
    };
  };

  fceumm = {
    title = "FCEUmm";
    description = "Runs Nintendo Entertainment System content with the FCEUmm libretro core.";
    core = pkgs.libretro.fceumm;
    coreFile = "${pkgs.libretro.fceumm}/lib/retroarch/cores/fceumm_libretro.so";
    systems.nes = {
      title = "Nintendo Entertainment System";
      extensions = [
        "nes"
        "fds"
      ];
    };
  };

  snes9x2010 = {
    title = "Snes9x 2010";
    description = "Runs Super Nintendo content with the Snes9x 2010 libretro core.";
    core = pkgs.libretro.snes9x2010;
    coreFile = "${pkgs.libretro.snes9x2010}/lib/retroarch/cores/snes9x2010_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "sfc"
        "smc"
      ];
    };
  };

  genesis-plus-gx = {
    title = "Genesis Plus GX";
    description = "Runs Sega 8-bit and 16-bit content with the Genesis Plus GX libretro core.";
    core = pkgs.libretro.genesis-plus-gx;
    coreFile = "${pkgs.libretro.genesis-plus-gx}/lib/retroarch/cores/genesis_plus_gx_libretro.so";
    systems = {
      md = {
        title = "Mega Drive";
        extensions = [
          "md"
          "gen"
          "smd"
        ];
      };
      sms = {
        title = "Master System";
        extensions = [ "sms" ];
      };
      gg = {
        title = "Game Gear";
        extensions = [ "gg" ];
      };
      sg1000 = {
        title = "SG-1000";
        extensions = [ "sg" ];
      };
    };
  };

  picodrive = {
    title = "PicoDrive";
    description = "Runs Mega Drive and 32X content with the ARM-tuned PicoDrive libretro core.";
    core = pkgs.libretro.picodrive;
    coreFile = "${pkgs.libretro.picodrive}/lib/retroarch/cores/picodrive_libretro.so";
    # Mega Drive overlaps Genesis Plus GX on purpose. Two cores may claim one
    # system: discovery returns every claim and the player picks a runner.
    systems = {
      md = {
        title = "Mega Drive";
        extensions = [
          "md"
          "gen"
          "smd"
        ];
      };
      sega32x = {
        title = "Sega 32X";
        extensions = [ "32x" ];
      };
    };
  };

  beetle-pce-fast = {
    title = "Beetle PCE Fast";
    description = "Runs PC Engine content with the Beetle PCE Fast libretro core.";
    core = pkgs.libretro.beetle-pce-fast;
    coreFile = "${pkgs.libretro.beetle-pce-fast}/lib/retroarch/cores/mednafen_pce_fast_libretro.so";
    systems.pce = {
      title = "PC Engine";
      extensions = [ "pce" ];
    };
  };

  stella = {
    title = "Stella";
    description = "Runs Atari 2600 content with the Stella libretro core.";
    core = pkgs.libretro.stella;
    coreFile = "${pkgs.libretro.stella}/lib/retroarch/cores/stella_libretro.so";
    systems.atari2600 = {
      title = "Atari 2600";
      extensions = [ "a26" ];
    };
  };
}
