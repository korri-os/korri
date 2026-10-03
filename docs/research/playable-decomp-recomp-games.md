# Inventory of Fully Playable Decompiled and Recompiled Games

Researched 2026-10-02. Verified against primary repositories, project catalogs (PortsDR, PortMaster), and community documentation (Read Only Memo, Held Games).

## Scope and Qualification Criteria

This document indexes retro and classic video games that run as **native standalone applications** via reverse engineering, rather than running inside an emulator at runtime.

### Inclusion criteria

1. **Native execution:** The game code executes directly on host processor architecture (x86_64, aarch64, or WebAssembly) through static binary recompilation or matching/clean-room source decompilation.
2. **Fully playable state:** The game can be played from start to finish. Audio, visual rendering, input, and game persistence (save states or native saves) function properly.
3. **Owned retail asset model:** The project distributes only engine/wrapper code. Players supply assets extracted from legitimate game media.

### Excluded categories

- Real-time emulators (RetroArch, Xenia, RPCS3, PCSX2, Dolphin).
- Incomplete decompilations that compile only back to original console ROMs without a native runtime port (e.g. `zeldaret/oot` or `doldecomp/sms` without a PC port wrapper).
- Fan games or total rebuilds that do not share original game logic or asset formats.

---

## Maturity Ratings

| Tier | Status | Meaning |
|---|---|---|
| **Tier 1: Polished Flagship** | Verified mature | Completed start to finish, extensive modern enhancements (widescreen, arbitrary frame rates, texture replacement, full controller/audio remapping), active community. |
| **Tier 2: Playable & Stable** | Verified playable | Beatable start to finish with authentic console behavior; may have minor visual or audio quirks or require specific configuration. |
| **Tier 3: Playable Beta** | In active testing | Main campaign completable or broad gameplay loops verified, but under active development with known edge-case bugs. |

---

## 1. Nintendo 64

The Nintendo 64 has the largest and most mature ecosystem of native ports, split between hand-crafted source decompilations and Wiseguy's `N64Recomp` static binary recompilation toolchain.

### Decompilation Ports (C/C++ Source Reconstructions)

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **Super Mario 64** | `sm64-port` / `SM64Plus` / `sm64coopdx` / `Ghostship` | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, Switch | First major console decomp port. Full widescreen, 60+ FPS, multiplayer (`coopdx`), camera enhancements. |
| **The Legend of Zelda: Ocarina of Time** | `Ship of Harkinian` (HarbourMasters) | 1 | Linux (x86_64, aarch64), Windows, macOS, Switch, Wii U | Benchmark for decomp ports. Arbitrary resolutions/FPS, gyro aiming, item randomizer, asset modding. In PortMaster. |
| **The Legend of Zelda: Majora's Mask** | `2 Ship 2 Harkinian` (HarbourMasters) | 1 | Linux (x86_64, aarch64), Windows, macOS, Switch, Android | Full parity with Ship of Harkinian. In PortMaster. |
| **Perfect Dark** | `perfect_dark` (Ryan Dwyer / `perfect-dark-dabs-mod`) | 1 | Linux (x86_64, aarch64), Windows, macOS, Switch | Full native port. Modern mouse/dual-stick aiming, high FPS, PortMaster support. |
| **Star Fox 64** | `Starship` (sonicdcer) | 1 | Linux (x86_64), Windows, Switch | Full native decomp port with widescreen, high frame rates, audio fixes. |
| **Mario Kart 64** | `SpaghettiKart` | 1 | Linux (x86_64, aarch64), Windows | Full native port, HD asset packs, widescreen. Available in PortMaster. |
| **Banjo-Kazooie** | `Lighthouse` | 1 | Linux (x86_64, aarch64), Windows, macOS, PS Vita | Decomp-based native PC/handheld port. |

### N64Recomp Ports (Static Binary Recompilation)

Built using the `N64Recomp` offline binary translation framework and the `RT64` rendering backend.

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **The Legend of Zelda: Majora's Mask** | `Zelda64Recomp` (Wiseguy) | 1 | Linux (x86_64), Windows, macOS, Android | Flagship N64Recomp release. Native ray tracing option, ultra-high refresh rate, mod loader. |
| **Donkey Kong 64** | `DK64 Rekongpiled` | 1 | Linux (x86_64, aarch64), Windows, macOS, Android | Reached v1.0.3. Provides official native Linux ARM64 binary downloads. Widescreen, tagging anywhere. |
| **Banjo-Kazooie** | `BanjoRecomp` | 1 | Linux (x86_64), Windows, macOS | Complete, stable native recompilation. |
| **Star Fox 64** | `Starfox64Recomp` | 1 | Linux (x86_64), Windows, macOS | Recomp alternative to Starship. Fast, accurate, mod support. |
| **Mario Kart 64** | `MarioKart64Recomp` | 2 | Linux (x86_64), Windows, macOS | High frame rates, widescreen, custom 3D model injection. |
| **Bomberman 64** | `BM64Recomp` | 2 | Linux (x86_64), Windows, macOS | Complete campaign and battle mode playable natively. |
| **Bomberman Hero** | `BMHeroRecomp` | 2 | Linux (x86_64), Windows, macOS | Playable start to finish. |
| **Dr. Mario 64** | `drmario64_recomp_plus` | 2 | Linux (x86_64), Windows | Native port of the puzzle classic. |
| **Extreme-G** | `ExtremeGRecomp` | 2 | Linux (x86_64), Windows, macOS | Futuristic racer running at unlocked native frame rates. |
| **Harvest Moon 64** | `HarvestMoon64Recomp` | 2 | Linux (x86_64), Windows, macOS, Android | Farming simulator running natively with persistence and bug fixes. |
| **Snowboard Kids 2** | `snowboardkids2-recomp` | 2 | Linux (x86_64), Windows, macOS | Full campaign and multiplayer playable. |
| **Rocket: Robot on Wheels** | `ROCKET-R` | 2 | Linux (x86_64, aarch64), Windows, Android | Sucker Punch's physics platformer running natively. |
| **Dinosaur Planet** | `dino-recomp` | 2 | Linux (x86_64), Windows | Cancelled Rare title running natively from leaked prototype ROM. |
| **GoldenEye 007** | `GoldenEye64Recomp` | 2 | Linux (x86_64), macOS | Hybrid decomp/recomp native build. |
| **Conker's Bad Fur Day** | `CBFD-Recompiled` | 3 | Linux (x86_64), Windows, macOS | Active beta, boots and plays through major chapters. |
| **Wave Race 64** | `wave-race-64-recomp` | 2 | Linux (x86_64), Windows | Water physics and racing logic running natively. |
| **Pokémon Snap** | `Snap64Recomp` | 2 | Windows, Linux | Playable photography mechanics. |
| **Space Station Silicon Valley** | `SSSV_Recomp` | 2 | Linux (x86_64), Windows, macOS | Fixes original N64 hardware bugs natively. |
| **Diddy Kong Racing** | `DKR-R` / `Golden Balloon` | 2 | Linux (x86_64), Windows, macOS | Playable racing adventure. |
| **Mystical Ninja Starring Goemon** | `Goemon64Recomp` | 3 | Linux (x86_64), Windows, macOS | Playable in active development. |
| **Quest 64** | `Quest64-Recomp` | 3 | Windows | Early playable state. |
| **Duke Nukem: Zero Hour** | `DNZHRecomp` | 3 | Linux (x86_64), Windows, macOS | Playable third-person shooter recomp. |

---

## 2. Xbox 360 & Original Xbox

### Xbox 360 (XenonRecomp & ReXGlue SDK)

Uses static translation of PowerPC Xenon binaries into C++ with native graphics backends.

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **Skate 3** | `skate3recomp` (mchughalex) | 1 | Linux (x86_64, aarch64 experimental), Windows, macOS | Fully custom native Vulkan renderer, 120+ FPS, Vert+ / ultrawide display match, cvar tuning. Verified on host `aka` and `fuji`. |
| **Sonic Unleashed** | `UnleashedRecomp` (hedge-dev) | 1 | Linux (x86_64), Windows, Switch | Proved 360 static recomp. Runs flawlessly on Steam Deck and mid-tier PCs without Xenia overhead. |
| **The Simpsons Game** | `TheSimpsonsGameRecomp` | 2 | Linux (x86_64), Windows | Developed with Steam Deck as primary test platform. Native renderer dropped frame render times by 45%. |
| **Fable 2** | `Fable-2-Recomp` | 2 | Windows, Linux | Only mainline Fable never released on PC. Developer confirmed beatable start to finish. |
| **Spider-Man: Edge of Time** | `Project 2099` | 2 | Windows | Never had a PC release. Includes installer, launcher, KBM controls, mod support. |
| **Castlevania: Symphony of the Night (XBLA)** | `NocturneRecomp` (birabittoh) | 2 | Windows, Linux | XBLA release recompiled on ReXGlue. Native 60 FPS, widescreen, clean audio. |
| **Banjo-Kazooie: Nuts & Bolts** | `reNut` | 2 | Windows | Rare's vehicle builder running natively without Xenia emulation stalls. |
| **Ace Combat 6: Fires of Liberation** | `AC6_recomp` | 2 | Windows | Xbox 360 exclusive running natively on PC. |
| **Lost Odyssey** | `LostOdysseyRecomp` | 2 | Windows | Mistwalker multi-disc JRPG running natively. Turn-based combat operates smoothly. |
| **Blue Dragon** | `re:Blue` | 2 | Windows, Linux, macOS | Mistwalker's other exclusive JRPG running via ReXGlue. |
| **Viva Piñata: Trouble in Paradise** | `ReTiP` | 2 | Windows | Simulation and garden management fully playable. |
| **Ninja Gaiden II** | `Ninja's Dawn` | 2 | Windows | High-speed action game; eliminates the severe frame drops of original 360 hardware. |
| **Kameo: Elements of Power** | `KameoRePowered` | 2 | Windows, Linux | Launch title running natively. |
| **Lollipop Chainsaw** | `Re-Cherry` | 2 | Windows | Suda51 action game native recompilation. |
| **Naughty Bear** | `NaughtyBear_ReStuff` | 2 | Windows | Base game and DLC content playable. |
| **Dead Rising 2: Case West & Case Zero** | `Dead_Rising_2_Case_West_Xenon_Recomp` | 2 | Windows | Standalone digital epilogues/prologues running natively. |
| **Army of Two** | `ArmyOfTwoRecomp` | 2 | Windows, Linux | Co-op third-person shooter playable. |

### Original Xbox

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **Halo: Combat Evolved** | `halo-ce-universal` | 1 | Windows, Linux (x86_64), Android (aarch64) | Groundbreaking native port. Includes first-class Android arm64 build, OpenGL ES 3, cross-platform system link multiplayer. |
| **X-Men Legends** | `OpenXML1xbox` | 2 | Windows | Raven Software action RPG running natively. |

---

## 3. PlayStation 2

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **Jak and Daxter: The Precursor Legacy** | `OpenGOAL` (water111 / OpenGOAL Team) | 1 | Linux (x86_64), Windows, macOS | Complete native reconstruction of Naughty Dog's GOAL language. Native 4K, 60+ FPS, widescreen, modern controls. Requires x86_64 AVX. |
| **Jak II** | `OpenGOAL` | 1 | Linux (x86_64), Windows, macOS | Complete, fully playable end-to-end with full audio and cutscene support. |
| **Jak 3** | `OpenGOAL` | 1 | Linux (x86_64), Windows, macOS | Reached full completion across the trilogy. |
| **Street Fighter III: 3rd Strike** | `3SX` / `3SXW` | 2 | Linux (x86_64), Windows, macOS | Native arcade/PS2 fighting game port. |
| **Sly Cooper and the Thievius Raccoonus** | `ProjectCane` | 3 | Windows | Playable beta based on active `sly1` decompilation. |

---

## 4. GameCube & Wii

### GameCube

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **The Legend of Zelda: Twilight Princess** | `Dusklight` (Twilit Realm Team) | 1 | Linux (x86_64, aarch64), Windows, macOS, Android | Shipped v2.0.3. Built on Aurora engine via OpenGL ES / Vulkan. Includes mod system, randomizer. Playable on low-cost ARM handhelds via PortMaster. |
| **Star Fox Adventures** | `Foxhollow` | 2 | Linux (x86_64), Windows, macOS | Native port following 100% byte-matched decompilation in 2026. |
| **Super Mario Strikers** | `strikers` / `BallPad` | 2 | Linux (x86_64), Windows, macOS, Switch, iOS | Fully playable arcade soccer game running natively. |
| **The Legend of Zelda: The Wind Waker** | `Wind-Waker-Recomp` / `BlueWake` | 2 | Windows, macOS, iOS | Recompilation port under active evolution. |
| **Pikmin** | `Open Nectar` | 2 | Windows, Linux, Android | Real-time strategy title running natively on PC and mobile. |
| **Animal Crossing** | `ACGC-PC-Port` | 2 | Windows | Native PC port of the GameCube original. |
| **Soulcalibur II** | `Ring Out` | 2 | Windows, Linux | Weapon fighter running natively. |

### Wii

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **Mario Kart Wii** | `Wiicompiled` / `driftdroid` | 2 | Linux (x86_64, aarch64), Windows, Android | First major Wii static recompilation. PowerPC translation ahead of time. Requires high-spec discrete GPU. |
| **Mario Strikers Charged** | `Strikers-WiiCompiled` | 2 | Linux (x86_64), Windows, macOS | Wii sequel running natively with unlocked performance. |
| **Kirby's Return to Dream Land** | `KirbyWii recomp` | 2 | Linux (x86_64), Windows | Platformer running natively. |

---

## 5. PlayStation 1

### Dedicated Decompilation & Native Ports

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **WipEout** | `wipeout-rewrite` / `WipeOut Phantom Edition` (phoboslab) | 1 | Linux (x86_64, aarch64), Windows, macOS, WebAssembly | Complete native C port reverse-engineered from source leak / PS1 binary. Uncapped frame rates, widescreen, audio CD music replay. Runs on virtually any device. |
| **Driver 2** | `REDRIVER2` (OpenDriver2) | 1 | Linux (x86_64), Windows | Fully decompiled native PC port. 60 FPS, widescreen, fixed collision bugs, improved draw distance. |
| **The Legend of Dragoon** | `Severed Chains` | 1 | Linux (x86_64), Windows, macOS | Complete reverse-engineered Java/C++ native runtime. All 4 discs fully beatable, zero loading times, 60 FPS additions. |
| **Doom (PlayStation)** | `PsyDoom` (int10h) | 1 | Linux (x86_64), Windows, macOS | Reverse engineered from PS1 MIPS disassembly. Recreates Williams' custom renderer, reverb audio, and lighting natively. |
| **Pepsiman** | `Pepsiman Recompiled` | 1 | WebAssembly, Windows | First major PSXRecomp browser release. Runs at 60 FPS in any modern web browser with persistent saves. |
| **Castlevania: Symphony of the Night** | `SymphonyRecomp` (BlackLabelHQ) | 2 | Windows, Linux, macOS, Android | Built on RecompOne / ReXGlue. Open beta, beatable start to finish. |

### PSXRecomp Static Recompilation Ecosystem

The `PSXRecomp` toolchain and community catalog (`Alexbeavs/psxrecomp-ports`, `retcomm-catalog`) supply build kits generating native executables (Windows x64, Linux x64, macOS Apple Silicon arm64, macOS Intel) from owned disc dumps. Verified playable releases include:

- **Action & Adventure:** *Alien Resurrection*, *Ape Escape*, *Apocalypse*, *Armored Core*, *Armored Core: Project Phantasma*, *Armored Core: Master of Arena*, *Blood Omen: Legacy of Kain*, *Brave Fencer Musashi*, *Bushido Blade 2*, *Crash Bandicoot 1-3*, *Crash Team Racing*, *Dino Crisis 1 & 2*, *Driver*, *Fear Effect 1 & 2*, *Future Cop: L.A.P.D.*, *Jackie Chan Stuntmaster*, *Legacy of Kain: Soul Reaver*, *MDK*, *MediEvil 1 & 2*, *Metal Gear Solid*, *Nightmare Creatures 1 & 2*, *Oddworld: Abe's Oddysee*, *Parasite Eve*, *Silent Hill*, *Spider-Man 1 & 2*, *Spyro the Dragon*, *Syphon Filter 1-3*, *Tenchu: Stealth Assassins*.
- **Racing & Sports:** *Colin McRae Rally 2.0*, *Destruction Derby 2 & Raw*, *Rollcage Stage II*, *Tony Hawk's Pro Skater 1-4*, *Vigilante 8*, *Wipeout XL / 2097*.
- **RPGs & Strategy:** *Alundra*, *Azure Dreams*, *Diablo (PSX)*, *Final Fantasy VIII & IX*, *Jade Cocoon*, *Koudelka*, *Mega Man Legends 1 & 2*, *Suikoden 1 & 2*, *Threads of Fate*, *Valkyrie Profile*, *Wild Arms*.
- **Fighting & Arcade:** *Bloody Roar II*, *Fighting Force*, *Metal Slug X*, *Mortal Kombat 4 / Trilogy*, *Rival Schools*, *Street Fighter Alpha 3 / EX2 Plus*, *Tekken 3*.

---

## 6. 8-Bit & 16-Bit Consoles & Handhelds

### Super Nintendo (SNES)

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **The Legend of Zelda: A Link to the Past** | `Zelda3` (snesrev) | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, Switch, 3DS | 100% C reimplementation from assembly. True 16:9/16:10 widescreen, high-resolution MSU-1 audio, pixel shaders, zero lag. |
| **Super Metroid** | `sm` (snesrev) | 1 | Linux (x86_64, aarch64), Windows, macOS, Switch | Full C reverse engineering. Widescreen support, modern gamepad bindings. |
| **Super Mario World** | `smw` (snesrev) / `SuperMarioWorldRecomp` | 1 | Linux (x86_64), Windows | Full native C port with widescreen and unlocked features. |
| **Donkey Kong Country 1, 2, 3** | `DKC1Recomp`, `DKC2Recomp`, `DKC3Recomp` | 2 | Windows, macOS | Complete static recompilations of Rare's SNES trilogy. |
| **F-Zero** | `FZeroSNESRecomp` | 2 | Windows, Linux | Mode 7 racer running natively at arbitrary frame rates. |
| **Mega Man X & X2** | `MegaManXSNESRecomp`, `MegaManX2Recomp` | 2 | Windows, Linux | Capcom action platformers running natively. |

### Sega Genesis / Mega Drive & Sega CD

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **Sonic the Hedgehog (2013)** | `Sonic-1-2-2013-Decompilation` (Rubberduckycooly) | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, Switch | Native PC/ARM execution of Christian Whitehead's Retro Engine (v3) mobile release. In PortMaster. |
| **Sonic the Hedgehog 2 (2013)** | `Sonic-1-2-2013-Decompilation` | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, Switch | Native Retro Engine v4 port with widescreen and Tails/Knuckles modes. In PortMaster. |
| **Sonic CD (2011)** | `Sonic-CD-11-Decompilation` | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, Switch | Native Retro Engine v3 port with both US/JP soundtracks. In PortMaster. |
| **Sonic 3 & Knuckles** | `Sonic 3 A.I.R.` (Eukaryot) | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, iOS, Switch | Native C++ execution layer on top of original Motorola 68000 ROM bytecode. 16:9 widescreen, 60 FPS, CD audio. |
| **Sonic Mania** | `Sonic-Mania-Decompilation` (Rubberduckycooly) | 1 | Linux (x86_64, aarch64), Windows, macOS, Switch | Full decompilation of Retro Engine v5. In PortMaster. |
| **Streets of Rage 2** | `SOR2 NE` | 2 | Windows | Native PC port of the beat-'em-up classic. |

### Game Boy, GBC, & Game Boy Advance

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **Pokémon Red / Blue / Yellow** | `Gen1Recomp` | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, iOS, Switch | Native engine recreation running from `pokered` disassembly. Optional 3D/first-person view, native controller input. |
| **The Legend of Zelda: Link's Awakening DX** | `LADXHD` | 1 | Linux (x86_64), Windows, macOS | HD widescreen PC port running from GBC disassembly. Smooth scrolling, high-res assets. |
| **The Legend of Zelda: The Minish Cap** | `Project Picori` / `tmc-android` | 2 | Linux (x86_64), Windows, macOS, Android, 3DS | Native standalone port compiled from `zeldaret/tmc` decompilation. |
| **Sonic Advance 1 & 2** | `sa1`, `sa2` | 2 | Windows, Linux | Native ports built on GBA source decompilations. |
| **Resident Evil Gaiden** | `regaiden-recomp` | 2 | Windows, Android | Native port of the portable survival horror entry. |
| **Wario Land 4** | `warioland4` | 2 | Linux (x86_64), Windows | Standalone native port from matching GBA decompilation. |

### NES

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **Super Mario Bros.** | `Super Mario Bros Remastered` | 1 | Linux (x86_64), Windows, Android | Widescreen native port rebuilt from NES disassembly. |
| **Zelda II: The Adventure of Link** | `z2rs` | 2 | Linux (x86_64), Windows, macOS | Native PC port with modern enhancements and quality-of-life options. |

---

## 7. Classic PC, MS-DOS, & Windows Reverse Engineering

Reverse-engineered C/C++ engine reconstructions that read original commercial data files and eliminate legacy DOSBox or 16-bit Windows wrappers.

| Game | Project | Tier | Platforms | Notes |
|---|---|:---:|---|---|
| **Diablo 1 + Hellfire** | `DevilutionX` (diasurgical) | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, Switch, iOS | Decompiled from original binary and PDBs. High resolutions, multiplayer, controller support, mobile touch UI. |
| **Grand Theft Auto III** | `re3` | 1 | Linux (x86_64, aarch64), Windows, Switch, PS Vita | Reverse engineered from RenderWare PC/PS2 binaries. Native widescreen, modern camera, fast asset loading. |
| **Grand Theft Auto: Vice City** | `reVC` | 1 | Linux (x86_64, aarch64), Windows, Switch, PS Vita | Companion project to `re3`. Uncapped frame rates, modern controller and resolution support. |
| **3D Pinball: Space Cadet** | `SpaceCadetPinball` (k4zmu2a) | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, WebAssembly | Decompiled from Windows NT `pinball.exe`. High-DPI scaling, cross-platform portability. |
| **Carmageddon** | `dethrace` (dethrace-labs) | 1 | Linux (x86_64), Windows, macOS | 100% reverse-engineered C source port of the MS-DOS release. |
| **Fallout 1** | `fallout1-ce` (alexbatalov) | 1 | Linux (x86_64, aarch64), Windows, macOS, Android | Fully reverse engineered from original x86 executable. Native resolutions, high-FPS animations, touch controls. |
| **Fallout 2** | `fallout2-ce` (alexbatalov) | 1 | Linux (x86_64, aarch64), Windows, macOS, Android | Complete native engine reimplementation. Native Android and Linux builds. |
| **RollerCoaster Tycoon 2** | `OpenRCT2` | 1 | Linux (x86_64, aarch64), Windows, macOS, Android | Massive engineering achievement: 100% decompiled from Chris Sawyer's x86 assembly to modern C++. Uncapped FPS, online multiplayer, OpenGL rendering. |
| **Transport Tycoon Deluxe** | `OpenTTD` | 1 | Linux (x86_64, aarch64), Windows, macOS, Android | 100% reverse-engineered and rewritten in C++. Massive maps, modern UI, network play. |
| **Heroes of Might and Magic II** | `fheroes2` | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, Switch | Clean-room engine reverse engineering. Complete AI rewrite, high resolution, modern multiplayer. |
| **Caesar III** | `Julius` & `Augustus` | 1 | Linux (x86_64, aarch64), Windows, macOS, Android, Switch | Full reverse engineering. `Julius` is an accurate 1:1 port; `Augustus` adds gameplay balance and enhancements. |
| **Theme Hospital** | `CorsixTH` | 1 | Linux (x86_64), Windows, macOS, Android | Reverse-engineered Lua/C++ engine reimplementation using original bullfrog assets. |
| **Jagged Alliance 2** | `JA2-Stracciatella` | 1 | Linux (x86_64), Windows, macOS, Android | Restored and cleaned cross-platform native engine for the tactical RPG. |
| **Tomb Raider 1 & 2** | `Tomb1Main`, `Tomb2Main` / `TRX` | 1 | Linux (x86_64), Windows | Reverse-engineered PC binaries replacing DOS wrappers with native SDL2, high frame rates, and texture filtering. |
| **The Elder Scrolls II: Daggerfall** | `Daggerfall Unity` | 1 | Linux (x86_64), Windows, macOS | Full reverse engineering of Daggerfall DOS mechanics in a modern engine reading original BSA files. |

---

## 8. Summary by Hardware Architecture & Platform

| Platform / Toolchain | Playable Count | Native Linux x86_64 | Native Linux aarch64 (ARM64) | Key Handheld Integration |
|---|:---:|:---:|:---:|---|
| **Nintendo 64 (Decomp & Recomp)** | 28+ | Universal | Widespread (Ship of Harkinian, 2S2H, DK64, SM64, Perfect Dark, SpaghettiKart) | Core pillar of PortMaster; official standalone ARM64 releases. |
| **PlayStation 1 (Decomp & PSXRecomp)** | 75+ | Universal | Selective (Wipeout, PsyDoom, plus WebAssembly browser path for all PSXRecomp titles) | High compatibility via desktop Linux & WebAssembly browsers. |
| **PlayStation 2 (OpenGOAL)** | 3 | Full | None (x86_64 AVX mandatory) | Steam Deck / x86 handhelds only. |
| **Xbox 360 (XenonRecomp / ReXGlue)** | 17+ | Growing rapidly (Skate 3, Unleashed, Simpsons, Fable 2, Nocturne) | Experimental (Skate 3 tested on Neoverse-N1 / Fuji; no shipping consumer handheld builds) | Steam Deck & ROG Ally are primary community targets. |
| **Original Xbox** | 2 | Full | Yes (`halo-ce-universal` ships native Android arm64) | Demonstrates 6th gen console ARM feasibility. |
| **GameCube & Wii** | 9 | Full | Yes (`Dusklight`, `Wiicompiled`) | `Dusklight` v2.0 brings GameCube Zelda to low-cost ARM handhelds via PortMaster. |
| **16-Bit & 8-Bit (SNES, Genesis, GBA)** | 18+ | Universal | Universal | Ubiquitous across Linux handhelds, Anbernic devices, and PortMaster. |
| **Classic PC / DOS Engines** | 15+ | Universal | Universal | Standard Linux desktop and PortMaster packaging. |
