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
let
  # nixpkgs marks both of these `badPlatforms = [ "aarch64-linux" ]`. PPSSPP is
  # the only first-party PSP libretro core, and ParaLLEl N64 is the parallel N64
  # core; both are wanted on the SM8550 target, so the platform block is lifted
  # for this catalogue only. Every other core keeps nixpkgs' platform metadata.
  ppsspp = pkgs.libretro.ppsspp.overrideAttrs (old: {
    meta = old.meta // {
      badPlatforms = [ ];
    };
  });

  parallel-n64 = pkgs.libretro.parallel-n64.overrideAttrs (old: {
    meta = old.meta // {
      badPlatforms = [ ];
    };
  });
in
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

  fuse = {
    title = "Fuse";
    description = "Runs ZX Spectrum content with the Fuse libretro core.";
    core = pkgs.libretro.fuse;
    coreFile = "${pkgs.libretro.fuse}/lib/retroarch/cores/fuse_libretro.so";
    systems.zxspectrum = {
      title = "ZX Spectrum";
      extensions = [
        "z80"
        "sna"
        "tap"
        "tzx"
        "szx"
      ];
    };
  };

  mupen64plus = {
    title = "Mupen64Plus-Next";
    description = "Runs Nintendo 64 content with the Mupen64Plus-Next libretro core.";
    core = pkgs.libretro.mupen64plus;
    coreFile = "${pkgs.libretro.mupen64plus}/lib/retroarch/cores/mupen64plus_next_libretro.so";
    systems.n64 = {
      title = "Nintendo 64";
      extensions = [
        "z64"
        "n64"
        "v64"
      ];
    };
  };

  # Mesen overlaps FCEUmm on purpose, the same way PicoDrive overlaps Genesis
  # Plus GX: two cores may claim one system, and the player chooses a runner.
  mesen = {
    title = "Mesen";
    description = "Runs Nintendo Entertainment System content with the Mesen libretro core.";
    core = pkgs.libretro.mesen;
    coreFile = "${pkgs.libretro.mesen}/lib/retroarch/cores/mesen_libretro.so";
    systems.nes = {
      title = "Nintendo Entertainment System";
      extensions = [
        "nes"
        "fds"
      ];
    };
  };

  np2kai = {
    title = "Neko Project II Kai";
    description = "Runs PC-98 content with the Neko Project II Kai libretro core.";
    core = pkgs.libretro.np2kai;
    coreFile = "${pkgs.libretro.np2kai}/lib/retroarch/cores/np2kai_libretro.so";
    systems.pc98 = {
      title = "PC-98";
      extensions = [
        "d88"
        "fdi"
        "hdi"
        "hdm"
        "nhd"
        "xdf"
      ];
    };
  };

  pcsx-rearmed = {
    title = "PCSX-ReARMed";
    description = "Runs PlayStation content with the PCSX-ReARMed libretro core.";
    core = pkgs.libretro.pcsx-rearmed;
    coreFile = "${pkgs.libretro.pcsx-rearmed}/lib/retroarch/cores/pcsx_rearmed_libretro.so";
    systems.psx = {
      title = "PlayStation";
      extensions = [
        "cue"
        "chd"
        "m3u"
        "pbp"
        "ccd"
        "toc"
      ];
    };
  };

  # bsnes overlaps Snes9x 2010 on purpose, the same way PicoDrive overlaps
  # Genesis Plus GX: two cores may claim one system, and the player chooses a
  # runner.
  bsnes = {
    title = "bsnes";
    description = "Runs Super Nintendo content with the bsnes libretro core.";
    core = pkgs.libretro.bsnes;
    coreFile = "${pkgs.libretro.bsnes}/lib/retroarch/cores/bsnes_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "sfc"
        "smc"
      ];
    };
  };

  atari800 = {
    title = "Atari800";
    description = "Runs Atari 8-bit and Atari 5200 content with the Atari800 libretro core.";
    core = pkgs.libretro.atari800;
    coreFile = "${pkgs.libretro.atari800}/lib/retroarch/cores/atari800_libretro.so";
    systems = {
      atari8bit = {
        title = "Atari 8-bit";
        extensions = [
          "xfd"
          "atr"
          "dcm"
          "cas"
          "atx"
          "car"
          "xex"
        ];
      };
      atari5200 = {
        title = "Atari 5200";
        extensions = [ "a52" ];
      };
    };
  };

  beetle-lynx = {
    title = "Beetle Lynx";
    description = "Runs Lynx content with the Beetle Lynx libretro core.";
    core = pkgs.libretro.beetle-lynx;
    coreFile = "${pkgs.libretro.beetle-lynx}/lib/retroarch/cores/mednafen_lynx_libretro.so";
    systems.lynx = {
      title = "Lynx";
      extensions = [
        "lnx"
        "lyx"
        "bll"
        "o"
      ];
    };
  };

  beetle-ngp = {
    title = "Beetle NeoPop";
    description = "Runs Neo Geo Pocket content with the Beetle NeoPop libretro core.";
    core = pkgs.libretro.beetle-ngp;
    coreFile = "${pkgs.libretro.beetle-ngp}/lib/retroarch/cores/mednafen_ngp_libretro.so";
    systems.ngp = {
      title = "Neo Geo Pocket";
      extensions = [
        "ngp"
        "ngc"
        "ngpc"
        "npc"
      ];
    };
  };

  beetle-pcfx = {
    title = "Beetle PC-FX";
    description = "Runs PC-FX content with the Beetle PC-FX libretro core.";
    core = pkgs.libretro.beetle-pcfx;
    coreFile = "${pkgs.libretro.beetle-pcfx}/lib/retroarch/cores/mednafen_pcfx_libretro.so";
    systems.pcfx = {
      title = "PC-FX";
      extensions = [
        "cue"
        "ccd"
        "toc"
        "chd"
      ];
    };
  };

  beetle-saturn = {
    title = "Beetle Saturn";
    description = "Runs Sega Saturn content with the Beetle Saturn libretro core.";
    core = pkgs.libretro.beetle-saturn;
    coreFile = "${pkgs.libretro.beetle-saturn}/lib/retroarch/cores/mednafen_saturn_libretro.so";
    systems.saturn = {
      title = "Sega Saturn";
      extensions = [
        "ccd"
        "chd"
        "cue"
        "toc"
      ];
    };
  };

  beetle-vb = {
    title = "Beetle VB";
    description = "Runs Virtual Boy content with the Beetle VB libretro core.";
    core = pkgs.libretro.beetle-vb;
    coreFile = "${pkgs.libretro.beetle-vb}/lib/retroarch/cores/mednafen_vb_libretro.so";
    systems.vb = {
      title = "Virtual Boy";
      extensions = [
        "vb"
        "vboy"
      ];
    };
  };

  beetle-wswan = {
    title = "Beetle WonderSwan";
    description = "Runs WonderSwan content with the Beetle WonderSwan libretro core.";
    core = pkgs.libretro.beetle-wswan;
    coreFile = "${pkgs.libretro.beetle-wswan}/lib/retroarch/cores/mednafen_wswan_libretro.so";
    systems.wswan = {
      title = "WonderSwan";
      extensions = [
        "ws"
        "wsc"
        "pc2"
        "pcv2"
      ];
    };
  };

  bluemsx = {
    title = "blueMSX";
    description = "Runs MSX, ColecoVision and SG-1000 content with the blueMSX libretro core.";
    core = pkgs.libretro.bluemsx;
    coreFile = "${pkgs.libretro.bluemsx}/lib/retroarch/cores/bluemsx_libretro.so";
    systems = {
      msx = {
        title = "MSX";
        extensions = [
          "mx1"
          "mx2"
          "dsk"
          "cas"
          "ri"
        ];
      };
      colecovision = {
        title = "ColecoVision";
        extensions = [ "col" ];
      };
      sg1000 = {
        title = "SG-1000";
        extensions = [
          "sg"
          "sc"
          "sf"
        ];
      };
    };
  };

  citra = {
    title = "Citra";
    description = "Runs Nintendo 3DS content with the Citra libretro core.";
    core = pkgs.libretro.citra;
    coreFile = "${pkgs.libretro.citra}/lib/retroarch/cores/citra_libretro.so";
    systems."3ds" = {
      title = "Nintendo 3DS";
      extensions = [
        "3ds"
        "3dsx"
        "axf"
        "cci"
        "cxi"
      ];
    };
  };

  desmume = {
    title = "DeSmuME";
    description = "Runs Nintendo DS content with the DeSmuME libretro core.";
    core = pkgs.libretro.desmume;
    coreFile = "${pkgs.libretro.desmume}/lib/retroarch/cores/desmume_libretro.so";
    systems.nds = {
      title = "Nintendo DS";
      extensions = [
        "nds"
        "ids"
      ];
    };
  };

  desmume2015 = {
    title = "DeSmuME 2015";
    description = "Runs Nintendo DS content with the DeSmuME 2015 libretro core.";
    core = pkgs.libretro.desmume2015;
    coreFile = "${pkgs.libretro.desmume2015}/lib/retroarch/cores/desmume2015_libretro.so";
    systems.nds = {
      title = "Nintendo DS";
      extensions = [
        "nds"
        "ids"
      ];
    };
  };

  dolphin = {
    title = "Dolphin";
    description = "Runs GameCube and Wii content with the Dolphin libretro core.";
    core = pkgs.libretro.dolphin;
    coreFile = "${pkgs.libretro.dolphin}/lib/retroarch/cores/dolphin_libretro.so";
    systems = {
      gamecube = {
        title = "GameCube";
        extensions = [ "gcm" ];
      };
      wii = {
        title = "Wii";
        extensions = [
          "wbfs"
          "wad"
          "rvz"
          "wia"
        ];
      };
    };
  };

  dosbox = {
    title = "DOSBox";
    description = "Runs DOS content with the DOSBox libretro core.";
    core = pkgs.libretro.dosbox;
    coreFile = "${pkgs.libretro.dosbox}/lib/retroarch/cores/dosbox_libretro.so";
    systems.dos = {
      title = "DOS";
      extensions = [
        "bat"
        "conf"
      ];
    };
  };

  dosbox-pure = {
    title = "DOSBox-pure";
    description = "Runs DOS content with the DOSBox-pure libretro core.";
    core = pkgs.libretro.dosbox-pure;
    coreFile = "${pkgs.libretro.dosbox-pure}/lib/retroarch/cores/dosbox_pure_libretro.so";
    systems.dos = {
      title = "DOS";
      extensions = [
        "dosz"
        "bat"
        "chd"
        "cue"
        "ins"
        "ima"
        "vhd"
        "jrc"
        "tc"
        "conf"
      ];
    };
  };

  easyrpg = {
    title = "EasyRPG Player";
    description = "Runs RPG Maker content with the EasyRPG Player libretro core.";
    core = pkgs.libretro.easyrpg;
    coreFile = "${pkgs.libretro.easyrpg}/lib/retroarch/cores/easyrpg_libretro.so";
    systems.rpgmaker = {
      title = "RPG Maker";
      extensions = [
        "ldb"
        "lzh"
        "easyrpg"
      ];
    };
  };

  eightyone = {
    title = "81";
    description = "Runs ZX81 content with the 81 libretro core.";
    core = pkgs.libretro.eightyone;
    coreFile = "${pkgs.libretro.eightyone}/lib/retroarch/cores/81_libretro.so";
    systems.zx81 = {
      title = "ZX81";
      extensions = [
        "p"
        "tzx"
        "t81"
      ];
    };
  };

  fbneo = {
    title = "FinalBurn Neo";
    description = "Runs Arcade content with the FinalBurn Neo libretro core.";
    core = pkgs.libretro.fbneo;
    coreFile = "${pkgs.libretro.fbneo}/lib/retroarch/cores/fbneo_libretro.so";
    systems.arcade = {
      title = "Arcade";
      extensions = [
        "cue"
        "ccd"
      ];
    };
  };

  flycast = {
    title = "Flycast";
    description = "Runs Dreamcast content with the Flycast libretro core.";
    core = pkgs.libretro.flycast;
    coreFile = "${pkgs.libretro.flycast}/lib/retroarch/cores/flycast_libretro.so";
    systems.dreamcast = {
      title = "Dreamcast";
      extensions = [
        "chd"
        "cdi"
        "cue"
        "gdi"
      ];
    };
  };

  fmsx = {
    title = "fMSX";
    description = "Runs MSX content with the fMSX libretro core.";
    core = pkgs.libretro.fmsx;
    coreFile = "${pkgs.libretro.fmsx}/lib/retroarch/cores/fmsx_libretro.so";
    systems.msx = {
      title = "MSX";
      extensions = [
        "mx1"
        "mx2"
        "dsk"
        "fdi"
        "cas"
      ];
    };
  };

  freeintv = {
    title = "FreeIntv";
    description = "Runs Intellivision content with the FreeIntv libretro core.";
    core = pkgs.libretro.freeintv;
    coreFile = "${pkgs.libretro.freeintv}/lib/retroarch/cores/freeintv_libretro.so";
    systems.intellivision = {
      title = "Intellivision";
      extensions = [ "int" ];
    };
  };

  gw = {
    title = "GW";
    description = "Runs Game & Watch content with the GW libretro core.";
    core = pkgs.libretro.gw;
    coreFile = "${pkgs.libretro.gw}/lib/retroarch/cores/gw_libretro.so";
    systems.gameandwatch = {
      title = "Game & Watch";
      extensions = [ "mgw" ];
    };
  };

  handy = {
    title = "Handy";
    description = "Runs Lynx content with the Handy libretro core.";
    core = pkgs.libretro.handy;
    coreFile = "${pkgs.libretro.handy}/lib/retroarch/cores/handy_libretro.so";
    systems.lynx = {
      title = "Lynx";
      extensions = [
        "lnx"
        "lyx"
        "o"
      ];
    };
  };

  hatari = {
    title = "hatari";
    description = "Runs Atari ST content with the hatari libretro core.";
    core = pkgs.libretro.hatari;
    coreFile = "${pkgs.libretro.hatari}/lib/retroarch/cores/hatari_libretro.so";
    systems.atarist = {
      title = "Atari ST";
      extensions = [
        "st"
        "msa"
        "stx"
        "dim"
        "ipf"
        "gem"
        "ide"
      ];
    };
  };

  melonds = {
    title = "melonDS";
    description = "Runs Nintendo DS content with the melonDS libretro core.";
    core = pkgs.libretro.melonds;
    coreFile = "${pkgs.libretro.melonds}/lib/retroarch/cores/melonds_libretro.so";
    systems.nds = {
      title = "Nintendo DS";
      extensions = [
        "nds"
        "ids"
        "dsi"
      ];
    };
  };

  neocd = {
    title = "NeoCD";
    description = "Runs Neo Geo CD content with the NeoCD libretro core.";
    core = pkgs.libretro.neocd;
    coreFile = "${pkgs.libretro.neocd}/lib/retroarch/cores/neocd_libretro.so";
    systems.ngcd = {
      title = "Neo Geo CD";
      extensions = [
        "cue"
        "chd"
      ];
    };
  };

  opera = {
    title = "Opera";
    description = "Runs 3DO content with the Opera libretro core.";
    core = pkgs.libretro.opera;
    coreFile = "${pkgs.libretro.opera}/lib/retroarch/cores/opera_libretro.so";
    systems."3do" = {
      title = "3DO";
      extensions = [
        "chd"
        "cue"
      ];
    };
  };

  pcsx2 = {
    title = "LRPS2";
    description = "Runs PlayStation 2 content with the LRPS2 libretro core.";
    core = pkgs.libretro.pcsx2;
    coreFile = "${pkgs.libretro.pcsx2}/lib/retroarch/cores/pcsx2_libretro.so";
    systems.ps2 = {
      title = "PlayStation 2";
      extensions = [
        "ciso"
        "cue"
        "chd"
      ];
    };
  };

  play = {
    title = "Play!";
    description = "Runs PlayStation 2 content with the Play! libretro core.";
    core = pkgs.libretro.play;
    coreFile = "${pkgs.libretro.play}/lib/retroarch/cores/play_libretro.so";
    systems.ps2 = {
      title = "PlayStation 2";
      extensions = [
        "chd"
        "cue"
      ];
    };
  };

  ppsspp = {
    title = "PPSSPP";
    description = "Runs PlayStation Portable content with the PPSSPP libretro core.";
    core = ppsspp;
    coreFile = "${ppsspp}/lib/retroarch/cores/ppsspp_libretro.so";
    systems.psp = {
      title = "PlayStation Portable";
      extensions = [
        "prx"
        "pbp"
        "chd"
      ];
    };
  };

  prboom = {
    title = "PrBoom";
    description = "Runs Doom content with the PrBoom libretro core.";
    core = pkgs.libretro.prboom;
    coreFile = "${pkgs.libretro.prboom}/lib/retroarch/cores/prboom_libretro.so";
    systems.doom = {
      title = "Doom";
      extensions = [
        "wad"
        "iwad"
        "pwad"
        "pk3"
      ];
    };
  };

  prosystem = {
    title = "ProSystem";
    description = "Runs Atari 7800 content with the ProSystem libretro core.";
    core = pkgs.libretro.prosystem;
    coreFile = "${pkgs.libretro.prosystem}/lib/retroarch/cores/prosystem_libretro.so";
    systems.atari7800 = {
      title = "Atari 7800";
      extensions = [
        "a78"
        "cdf"
      ];
    };
  };

  puae = {
    title = "PUAE";
    description = "Runs Amiga content with the PUAE libretro core.";
    core = pkgs.libretro.puae;
    coreFile = "${pkgs.libretro.puae}/lib/retroarch/cores/puae_libretro.so";
    systems.amiga = {
      title = "Amiga";
      extensions = [
        "adf"
        "adz"
        "dms"
        "fdi"
        "ipf"
        "hdf"
        "hdz"
        "lha"
        "slave"
        "info"
        "uae"
        "rp9"
      ];
    };
  };

  same_cdi = {
    title = "SAME CDi (Git)";
    description = "Runs CD-i content with the SAME CDi (Git) libretro core.";
    core = pkgs.libretro.same_cdi;
    coreFile = "${pkgs.libretro.same_cdi}/lib/retroarch/cores/same_cdi_libretro.so";
    systems.cdi = {
      title = "CD-i";
      extensions = [
        "chd"
        "cue"
      ];
    };
  };

  scummvm = {
    title = "ScummVM";
    description = "Runs ScummVM content with the ScummVM libretro core.";
    core = pkgs.libretro.scummvm;
    coreFile = "${pkgs.libretro.scummvm}/lib/retroarch/cores/scummvm_libretro.so";
    systems.scummvm = {
      title = "ScummVM";
      extensions = [ "scummvm" ];
    };
  };

  thepowdertoy = {
    title = "ThePowderToy";
    description = "Runs The Powder Toy content with the ThePowderToy libretro core.";
    core = pkgs.libretro.thepowdertoy;
    coreFile = "${pkgs.libretro.thepowdertoy}/lib/retroarch/cores/thepowdertoy_libretro.so";
    systems.powdertoy = {
      title = "The Powder Toy";
      extensions = [ "cps" ];
    };
  };

  tic80 = {
    title = "TIC-80";
    description = "Runs TIC-80 content with the TIC-80 libretro core.";
    core = pkgs.libretro.tic80;
    coreFile = "${pkgs.libretro.tic80}/lib/retroarch/cores/tic80_libretro.so";
    systems.tic80 = {
      title = "TIC-80";
      extensions = [ "tic" ];
    };
  };

  vecx = {
    title = "vecx";
    description = "Runs Vectrex content with the vecx libretro core.";
    core = pkgs.libretro.vecx;
    coreFile = "${pkgs.libretro.vecx}/lib/retroarch/cores/vecx_libretro.so";
    systems.vectrex = {
      title = "Vectrex";
      extensions = [ "vec" ];
    };
  };

  vice-x128 = {
    title = "VICE x128";
    description = "Runs Commodore 128 content with the VICE x128 libretro core.";
    core = pkgs.libretro.vice-x128;
    coreFile = "${pkgs.libretro.vice-x128}/lib/retroarch/cores/vice_x128_libretro.so";
    systems.c128 = {
      title = "Commodore 128";
      extensions = [
        "d64"
        "d71"
        "d80"
        "d81"
        "d82"
        "g64"
        "g41"
        "x64"
        "t64"
        "tap"
        "prg"
        "p00"
        "crt"
        "vfl"
        "vsf"
        "nib"
        "d2m"
        "d4m"
      ];
    };
  };

  vice-x64 = {
    title = "VICE x64";
    description = "Runs Commodore 64 content with the VICE x64 libretro core.";
    core = pkgs.libretro.vice-x64;
    coreFile = "${pkgs.libretro.vice-x64}/lib/retroarch/cores/vice_x64_libretro.so";
    systems.c64 = {
      title = "Commodore 64";
      extensions = [
        "d64"
        "d71"
        "d80"
        "d81"
        "d82"
        "g64"
        "g41"
        "x64"
        "t64"
        "tap"
        "prg"
        "p00"
        "crt"
        "vfl"
        "vsf"
        "nib"
        "d2m"
        "d4m"
      ];
    };
  };

  vice-x64dtv = {
    title = "VICE x64dtv";
    description = "Runs Commodore 64 DTV content with the VICE x64dtv libretro core.";
    core = pkgs.libretro.vice-x64dtv;
    coreFile = "${pkgs.libretro.vice-x64dtv}/lib/retroarch/cores/vice_x64dtv_libretro.so";
    systems.c64dtv = {
      title = "Commodore 64 DTV";
      extensions = [
        "d64"
        "d71"
        "d80"
        "d81"
        "d82"
        "g64"
        "g41"
        "x64"
        "t64"
        "tap"
        "prg"
        "p00"
        "crt"
        "vfl"
        "vsf"
        "nib"
        "d2m"
        "d4m"
      ];
    };
  };

  vice-x64sc = {
    title = "VICE x64sc";
    description = "Runs Commodore 64 content with the VICE x64sc libretro core.";
    core = pkgs.libretro.vice-x64sc;
    coreFile = "${pkgs.libretro.vice-x64sc}/lib/retroarch/cores/vice_x64sc_libretro.so";
    systems.c64 = {
      title = "Commodore 64";
      extensions = [
        "d64"
        "d71"
        "d80"
        "d81"
        "d82"
        "g64"
        "g41"
        "x64"
        "t64"
        "tap"
        "prg"
        "p00"
        "crt"
        "vfl"
        "vsf"
        "nib"
        "d2m"
        "d4m"
      ];
    };
  };

  vice-xcbm2 = {
    title = "VICE xcbm2";
    description = "Runs Commodore CBM-II content with the VICE xcbm2 libretro core.";
    core = pkgs.libretro.vice-xcbm2;
    coreFile = "${pkgs.libretro.vice-xcbm2}/lib/retroarch/cores/vice_xcbm2_libretro.so";
    systems.cbm2 = {
      title = "Commodore CBM-II";
      extensions = [
        "d64"
        "d71"
        "d80"
        "d81"
        "d82"
        "g64"
        "g41"
        "x64"
        "t64"
        "tap"
        "prg"
        "p00"
        "crt"
        "vfl"
        "vsf"
        "nib"
        "d2m"
        "d4m"
      ];
    };
  };

  vice-xcbm5x0 = {
    title = "VICE xcbm5x0";
    description = "Runs Commodore CBM-5x0 content with the VICE xcbm5x0 libretro core.";
    core = pkgs.libretro.vice-xcbm5x0;
    coreFile = "${pkgs.libretro.vice-xcbm5x0}/lib/retroarch/cores/vice_xcbm5x0_libretro.so";
    systems.cbm5x0 = {
      title = "Commodore CBM-5x0";
      extensions = [
        "d64"
        "d71"
        "d80"
        "d81"
        "d82"
        "g64"
        "g41"
        "x64"
        "t64"
        "tap"
        "prg"
        "p00"
        "crt"
        "vfl"
        "vsf"
        "nib"
        "d2m"
        "d4m"
      ];
    };
  };

  vice-xpet = {
    title = "VICE xpet";
    description = "Runs Commodore PET content with the VICE xpet libretro core.";
    core = pkgs.libretro.vice-xpet;
    coreFile = "${pkgs.libretro.vice-xpet}/lib/retroarch/cores/vice_xpet_libretro.so";
    systems.pet = {
      title = "Commodore PET";
      extensions = [
        "d64"
        "d71"
        "d80"
        "d81"
        "d82"
        "g64"
        "g41"
        "x64"
        "t64"
        "tap"
        "prg"
        "p00"
        "crt"
        "vfl"
        "vsf"
        "nib"
        "d2m"
        "d4m"
      ];
    };
  };

  vice-xplus4 = {
    title = "VICE xplus4";
    description = "Runs Commodore Plus/4 content with the VICE xplus4 libretro core.";
    core = pkgs.libretro.vice-xplus4;
    coreFile = "${pkgs.libretro.vice-xplus4}/lib/retroarch/cores/vice_xplus4_libretro.so";
    systems.plus4 = {
      title = "Commodore Plus/4";
      extensions = [
        "d64"
        "d71"
        "d80"
        "d81"
        "d82"
        "g64"
        "g41"
        "x64"
        "t64"
        "tap"
        "prg"
        "p00"
        "crt"
        "vfl"
        "vsf"
        "nib"
        "d2m"
        "d4m"
      ];
    };
  };

  vice-xscpu64 = {
    title = "VICE xscpu64";
    description = "Runs Commodore 64 SuperCPU content with the VICE xscpu64 libretro core.";
    core = pkgs.libretro.vice-xscpu64;
    coreFile = "${pkgs.libretro.vice-xscpu64}/lib/retroarch/cores/vice_xscpu64_libretro.so";
    systems.c64supercpu = {
      title = "Commodore 64 SuperCPU";
      extensions = [
        "d64"
        "d71"
        "d80"
        "d81"
        "d82"
        "g64"
        "g41"
        "x64"
        "t64"
        "tap"
        "prg"
        "p00"
        "crt"
        "vfl"
        "vsf"
        "nib"
        "d2m"
        "d4m"
      ];
    };
  };

  vice-xvic = {
    title = "VICE xvic";
    description = "Runs Commodore VIC-20 content with the VICE xvic libretro core.";
    core = pkgs.libretro.vice-xvic;
    coreFile = "${pkgs.libretro.vice-xvic}/lib/retroarch/cores/vice_xvic_libretro.so";
    systems.vic20 = {
      title = "Commodore VIC-20";
      extensions = [
        "d64"
        "d71"
        "d80"
        "d81"
        "d82"
        "g64"
        "g41"
        "x64"
        "t64"
        "tap"
        "prg"
        "p00"
        "crt"
        "vfl"
        "vsf"
        "nib"
        "d2m"
        "d4m"
        "20"
        "40"
        "60"
        "a0"
        "b0"
      ];
    };
  };

  virtualjaguar = {
    title = "Virtual Jaguar";
    description = "Runs Atari Jaguar content with the Virtual Jaguar libretro core.";
    core = pkgs.libretro.virtualjaguar;
    coreFile = "${pkgs.libretro.virtualjaguar}/lib/retroarch/cores/virtualjaguar_libretro.so";
    systems.jaguar = {
      title = "Atari Jaguar";
      extensions = [
        "j64"
        "jag"
        "abs"
        "cof"
        "prg"
        "cue"
        "cdi"
        "chd"
      ];
    };
  };

  yabause = {
    title = "Yabause";
    description = "Runs Sega Saturn content with the Yabause libretro core.";
    core = pkgs.libretro.yabause;
    coreFile = "${pkgs.libretro.yabause}/lib/retroarch/cores/yabause_libretro.so";
    systems.saturn = {
      title = "Sega Saturn";
      extensions = [
        "ccd"
        "chd"
        "cue"
        "mds"
      ];
    };
  };

  beetle-gba = {
    title = "Beetle GBA";
    description = "Runs Game Boy Advance content with the Beetle GBA libretro core.";
    core = pkgs.libretro.beetle-gba;
    coreFile = "${pkgs.libretro.beetle-gba}/lib/retroarch/cores/mednafen_gba_libretro.so";
    systems.gba = {
      title = "Game Boy Advance";
      extensions = [
        "gba"
        "agb"
      ];
    };
  };

  beetle-pce = {
    title = "Beetle PCE";
    description = "Runs PC Engine, SuperGrafx and PC Engine CD content with the Beetle PCE libretro core.";
    core = pkgs.libretro.beetle-pce;
    coreFile = "${pkgs.libretro.beetle-pce}/lib/retroarch/cores/mednafen_pce_libretro.so";
    systems = {
      pce = {
        title = "PC Engine";
        extensions = [ "pce" ];
      };
      supergrafx = {
        title = "SuperGrafx";
        extensions = [ "sgx" ];
      };
      pcecd = {
        title = "PC Engine CD";
        extensions = [
          "cue"
          "ccd"
          "chd"
          "toc"
        ];
      };
    };
  };

  beetle-psx = {
    title = "Beetle PSX";
    description = "Runs PlayStation content with the Beetle PSX libretro core.";
    core = pkgs.libretro.beetle-psx;
    coreFile = "${pkgs.libretro.beetle-psx}/lib/retroarch/cores/mednafen_psx_libretro.so";
    systems.psx = {
      title = "PlayStation";
      extensions = [
        "cue"
        "toc"
        "ccd"
        "pbp"
        "chd"
      ];
    };
  };

  beetle-psx-hw = {
    title = "Beetle PSX HW";
    description = "Runs PlayStation content with the Beetle PSX HW libretro core.";
    core = pkgs.libretro.beetle-psx-hw;
    coreFile = "${pkgs.libretro.beetle-psx-hw}/lib/retroarch/cores/mednafen_psx_hw_libretro.so";
    systems.psx = {
      title = "PlayStation";
      extensions = [
        "cue"
        "toc"
        "ccd"
        "pbp"
        "chd"
      ];
    };
  };

  beetle-supafaust = {
    title = "Beetle Supafaust";
    description = "Runs Super Nintendo content with the Beetle Supafaust libretro core.";
    core = pkgs.libretro.beetle-supafaust;
    coreFile = "${pkgs.libretro.beetle-supafaust}/lib/retroarch/cores/mednafen_supafaust_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "smc"
        "sfc"
        "swc"
        "gd3"
        "gd7"
        "dx2"
        "bsx"
        "fig"
      ];
    };
  };

  beetle-supergrafx = {
    title = "Beetle SuperGrafx";
    description = "Runs SuperGrafx content with the Beetle SuperGrafx libretro core.";
    core = pkgs.libretro.beetle-supergrafx;
    coreFile = "${pkgs.libretro.beetle-supergrafx}/lib/retroarch/cores/mednafen_supergrafx_libretro.so";
    systems.supergrafx = {
      title = "SuperGrafx";
      extensions = [ "sgx" ];
    };
  };

  blastem = {
    title = "BlastEm";
    description = "Runs Mega Drive, Sega 32X, Master System, Game Gear, SG-1000 and ColecoVision content with the BlastEm libretro core.";
    core = pkgs.libretro.blastem;
    coreFile = "${pkgs.libretro.blastem}/lib/retroarch/cores/blastem_libretro.so";
    systems = {
      md = {
        title = "Mega Drive";
        extensions = [
          "md"
          "gen"
          "smd"
          "68k"
          "sgd"
        ];
      };
      sega32x = {
        title = "Sega 32X";
        extensions = [ "32x" ];
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
        extensions = [
          "sg"
          "sg1"
          "sc"
          "sc3"
          "sf7"
        ];
      };
      colecovision = {
        title = "ColecoVision";
        extensions = [ "col" ];
      };
    };
  };

  bsnes-hd = {
    title = "bsnes-hd beta";
    description = "Runs Super Nintendo content with the bsnes-hd beta libretro core.";
    core = pkgs.libretro.bsnes-hd;
    coreFile = "${pkgs.libretro.bsnes-hd}/lib/retroarch/cores/bsnes_hd_beta_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "smc"
        "sfc"
        "swc"
        "fig"
        "bs"
      ];
    };
  };

  bsnes-mercury = {
    title = "bsnes-mercury Accuracy";
    description = "Runs Super Nintendo content with the bsnes-mercury Accuracy libretro core.";
    core = pkgs.libretro.bsnes-mercury;
    coreFile = "${pkgs.libretro.bsnes-mercury}/lib/retroarch/cores/bsnes_mercury_accuracy_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "sfc"
        "smc"
        "bml"
        "bs"
        "st"
      ];
    };
  };

  bsnes-mercury-balanced = {
    title = "bsnes-mercury Balanced";
    description = "Runs Super Nintendo content with the bsnes-mercury Balanced libretro core.";
    core = pkgs.libretro.bsnes-mercury-balanced;
    coreFile = "${pkgs.libretro.bsnes-mercury-balanced}/lib/retroarch/cores/bsnes_mercury_balanced_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "sfc"
        "smc"
        "bml"
        "bs"
        "st"
      ];
    };
  };

  bsnes-mercury-performance = {
    title = "bsnes-mercury Performance";
    description = "Runs Super Nintendo content with the bsnes-mercury Performance libretro core.";
    core = pkgs.libretro.bsnes-mercury-performance;
    coreFile = "${pkgs.libretro.bsnes-mercury-performance}/lib/retroarch/cores/bsnes_mercury_performance_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "sfc"
        "smc"
        "bml"
        "bs"
        "st"
      ];
    };
  };

  gpsp = {
    title = "gpSP";
    description = "Runs Game Boy Advance content with the gpSP libretro core.";
    core = pkgs.libretro.gpsp;
    coreFile = "${pkgs.libretro.gpsp}/lib/retroarch/cores/gpsp_libretro.so";
    systems.gba = {
      title = "Game Boy Advance";
      extensions = [ "gba" ];
    };
  };

  meteor = {
    title = "Meteor";
    description = "Runs Game Boy Advance content with the Meteor libretro core.";
    core = pkgs.libretro.meteor;
    coreFile = "${pkgs.libretro.meteor}/lib/retroarch/cores/meteor_libretro.so";
    systems.gba = {
      title = "Game Boy Advance";
      extensions = [ "gba" ];
    };
  };

  nestopia = {
    title = "Nestopia";
    description = "Runs Nintendo Entertainment System content with the Nestopia libretro core.";
    core = pkgs.libretro.nestopia;
    coreFile = "${pkgs.libretro.nestopia}/lib/retroarch/cores/nestopia_libretro.so";
    systems.nes = {
      title = "Nintendo Entertainment System";
      extensions = [
        "nes"
        "fds"
      ];
    };
  };

  parallel-n64 = {
    title = "ParaLLEl N64";
    description = "Runs Nintendo 64 content with the ParaLLEl N64 libretro core.";
    core = parallel-n64;
    coreFile = "${parallel-n64}/lib/retroarch/cores/parallel_n64_libretro.so";
    systems.n64 = {
      title = "Nintendo 64";
      extensions = [
        "n64"
        "v64"
        "z64"
        "ndd"
      ];
    };
  };

  quicknes = {
    title = "QuickNES";
    description = "Runs Nintendo Entertainment System content with the QuickNES libretro core.";
    core = pkgs.libretro.quicknes;
    coreFile = "${pkgs.libretro.quicknes}/lib/retroarch/cores/quicknes_libretro.so";
    systems.nes = {
      title = "Nintendo Entertainment System";
      extensions = [ "nes" ];
    };
  };

  sameboy = {
    title = "SameBoy";
    description = "Runs Game Boy and Game Boy Color content with the SameBoy libretro core.";
    core = pkgs.libretro.sameboy;
    coreFile = "${pkgs.libretro.sameboy}/lib/retroarch/cores/sameboy_libretro.so";
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

  smsplus-gx = {
    title = "SMS Plus GX";
    description = "Runs Master System and Game Gear content with the SMS Plus GX libretro core.";
    core = pkgs.libretro.smsplus-gx;
    coreFile = "${pkgs.libretro.smsplus-gx}/lib/retroarch/cores/smsplus_libretro.so";
    systems = {
      sms = {
        title = "Master System";
        extensions = [ "sms" ];
      };
      gg = {
        title = "Game Gear";
        extensions = [ "gg" ];
      };
    };
  };

  snes9x = {
    title = "Snes9x";
    description = "Runs Super Nintendo content with the Snes9x libretro core.";
    core = pkgs.libretro.snes9x;
    coreFile = "${pkgs.libretro.snes9x}/lib/retroarch/cores/snes9x_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "smc"
        "sfc"
        "swc"
        "fig"
        "bs"
        "st"
      ];
    };
  };

  snes9x2002 = {
    title = "Snes9x 2002";
    description = "Runs Super Nintendo content with the Snes9x 2002 libretro core.";
    core = pkgs.libretro.snes9x2002;
    coreFile = "${pkgs.libretro.snes9x2002}/lib/retroarch/cores/snes9x2002_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "smc"
        "sfc"
        "swc"
        "fig"
        "bsx"
      ];
    };
  };

  snes9x2005 = {
    title = "Snes9x 2005";
    description = "Runs Super Nintendo content with the Snes9x 2005 libretro core.";
    core = pkgs.libretro.snes9x2005;
    coreFile = "${pkgs.libretro.snes9x2005}/lib/retroarch/cores/snes9x2005_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "smc"
        "sfc"
        "swc"
        "fig"
        "bsx"
      ];
    };
  };

  snes9x2005-plus = {
    title = "Snes9x 2005 Plus";
    description = "Runs Super Nintendo content with the Snes9x 2005 Plus libretro core.";
    core = pkgs.libretro.snes9x2005-plus;
    coreFile = "${pkgs.libretro.snes9x2005-plus}/lib/retroarch/cores/snes9x2005_plus_libretro.so";
    systems.snes = {
      title = "Super Nintendo";
      extensions = [
        "smc"
        "sfc"
        "swc"
        "fig"
        "bsx"
      ];
    };
  };

  stella2014 = {
    title = "Stella 2014";
    description = "Runs Atari 2600 content with the Stella 2014 libretro core.";
    core = pkgs.libretro.stella2014;
    coreFile = "${pkgs.libretro.stella2014}/lib/retroarch/cores/stella2014_libretro.so";
    systems.atari2600 = {
      title = "Atari 2600";
      extensions = [ "a26" ];
    };
  };

  swanstation = {
    title = "SwanStation";
    description = "Runs PlayStation content with the SwanStation libretro core.";
    core = pkgs.libretro.swanstation;
    coreFile = "${pkgs.libretro.swanstation}/lib/retroarch/cores/swanstation_libretro.so";
    systems.psx = {
      title = "PlayStation";
      extensions = [
        "psexe"
        "cue"
        "chd"
        "pbp"
        "mds"
        "psf"
      ];
    };
  };

  tgbdual = {
    title = "TGB Dual";
    description = "Runs Game Boy and Game Boy Color content with the TGB Dual libretro core.";
    core = pkgs.libretro.tgbdual;
    coreFile = "${pkgs.libretro.tgbdual}/lib/retroarch/cores/tgbdual_libretro.so";
    systems = {
      gb = {
        title = "Game Boy";
        extensions = [
          "gb"
          "dmg"
          "sgb"
        ];
      };
      gbc = {
        title = "Game Boy Color";
        extensions = [
          "gbc"
          "cgb"
        ];
      };
    };
  };

  vba-m = {
    title = "VBA-M";
    description = "Runs Game Boy Advance, Game Boy and Game Boy Color content with the VBA-M libretro core.";
    core = pkgs.libretro.vba-m;
    coreFile = "${pkgs.libretro.vba-m}/lib/retroarch/cores/vbam_libretro.so";
    systems = {
      gba = {
        title = "Game Boy Advance";
        extensions = [ "gba" ];
      };
      gb = {
        title = "Game Boy";
        extensions = [
          "gb"
          "dmg"
        ];
      };
      gbc = {
        title = "Game Boy Color";
        extensions = [
          "gbc"
          "cgb"
          "sgb"
        ];
      };
    };
  };

  vba-next = {
    title = "VBA Next";
    description = "Runs Game Boy Advance content with the VBA Next libretro core.";
    core = pkgs.libretro.vba-next;
    coreFile = "${pkgs.libretro.vba-next}/lib/retroarch/cores/vba_next_libretro.so";
    systems.gba = {
      title = "Game Boy Advance";
      extensions = [ "gba" ];
    };
  };
}
