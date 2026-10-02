# How other projects decide what system a game file belongs to

Researched on 2026-10-01 for the shared-extension discovery problem. A
recompilation such as Skate 3 must accept a specific Xbox 360 `.iso`, while
Dolphin, PCSX2 and PPSSPP accept other `.iso` files. The user set two
constraints: a file extension never belongs exclusively to one system, and a
game is not bound to a specific system folder.

## Answer

The projects split into two groups.

- **Frontends decide by folder or by the user.** ES-DE, Playnite and RomM tie a
  scanned file to a platform through a per-system folder or a scanner that the
  user binds to one folder and one emulator profile. They avoid the shared
  extension problem by making the user sort the files first.
- **Emulators and achievement hashers decide by content.** Dolphin, Xenia,
  RetroArch's database scanner and rcheevos read fixed bytes inside the file.
  An extension only narrows which checks to try.

No source examined lets an extension alone choose between systems. The
content-based projects are the only ones that meet both of Korri's
constraints.

## Frontends

| Product | Decides the system by | Shared extension | Several launchers per game | Folder required |
|---|---|---|---|---|
| ES-DE | Per-system folder, then the system's extension list | The folder decides. `.iso` appears on 41 lines of the Linux `es_systems.xml`, which are system extension lists. | Yes. Each system lists several `<command label=...>` entries, used system-wide or per game. | Yes |
| Playnite | The emulator profile chosen for an auto-scan folder; a checksum lookup adds metadata | The profile decides | Yes, through emulator profiles | One scanner is one folder plus one profile |
| RomM | Platform folder name, which "has to match a known slug" | The folder decides | Not examined | Yes |
| RetroArch manual scan | The system name the user enters | The user decides | Not examined | One directory per playlist |

Evidence:

- ES-DE `USERGUIDE.md`: "just put your game files into the folder
  corresponding to the platform name". Its Linux `es_systems.xml` gives
  `gc` the path `%ROMPATH%/gc` and extensions that include `.iso`, and gives
  `ps2` the path `%ROMPATH%/ps2` with `.iso` too. `xbox360` uses
  `%ROMPATH%/xbox360`. Sources:
  <https://gitlab.com/es-de/emulationstation-de/-/raw/master/USERGUIDE.md>,
  <https://gitlab.com/es-de/emulationstation-de/-/raw/master/resources/systems/linux/es_systems.xml>.
- Playnite's manual: "How games are imported is controller by an emulator and
  its selected profile", and custom profiles "primarily matches games by
  specified file extensions". The scan setup takes one `Scan folder` and one
  emulator profile. Its checksum scan looks up an emulation database, and
  `chd` files are excluded by default because the database has no records
  for them. Source:
  <https://raw.githubusercontent.com/Playnite/Docs/main/docs/manual/features/emulationSupport/addingEmulatedGames.md>.
- RomM: "The platform folder name has to match a known slug". Source:
  <https://docs.romm.app/5.1.0/getting-started/folder-structure/>.

The ES-DE model is a direct example of the design the user rejected. Move a
GameCube `.iso` into the `ps2` folder and ES-DE treats it as a PS2 game.

## Content identification

| Project | What it reads | What it proves | Cost |
|---|---|---|---|
| Dolphin | `u32` at `0x18` equals `WII_DISC_MAGIC`, else `u32` at `0x1C` equals `GAMECUBE_DISC_MAGIC` | Wii or GameCube disc | 8 bytes |
| Xenia | `"MICROSOFT*XBOX*MEDIA"` at sector 32 (2 KiB sectors) from one of five partition offsets | An Xbox game disc file system (GDFX) | 5 small reads |
| RetroArch scanner | A table of magic strings at fixed offsets, then a serial search | System plus serial for a database lookup | Up to 600 KiB per file |
| rcheevos | Candidate consoles chosen by extension, then each console's hasher validates and hashes specific content | System plus a stable game hash | Executable and metadata, not the whole disc |

Evidence:

- Dolphin `Source/Core/DiscIO/Volume.cpp`, `TryCreateDisc`:
  `reader->ReadSwapped<u32>(0x18) == WII_DISC_MAGIC` and
  `reader->ReadSwapped<u32>(0x1C) == GAMECUBE_DISC_MAGIC`; otherwise "No
  known magic words found". Source:
  <https://raw.githubusercontent.com/dolphin-emu/dolphin/master/Source/Core/DiscIO/Volume.cpp>.
- Xenia `src/xenia/vfs/devices/disc_image_device.cc`, `Verify`: tries
  offsets `0x00000000, 0x0000FB20, 0x00020600, 0x02080000, 0x0FD90000`, then
  `VerifyMagic` compares `"MICROSOFT*XBOX*MEDIA"` at
  `game_offset + (32 * kXESectorSize)`. If no offset matches, the file is
  "likely not a real GDFX source". Source:
  <https://raw.githubusercontent.com/xenia-project/xenia/master/src/xenia/vfs/devices/disc_image_device.cc>.
- RetroArch `tasks/task_database_cue.c`, `MAGIC_NUMBERS` and
  `detect_system`. Entries include GameCube `\xc2\x33\x9f\x3d` at `0x1c`, Wii
  `\x5d\x1c\x9e\xa3` at `0x18`, PSP `"PSP GAME"` at `0x8008`, Dreamcast
  `"SEGA SEGAKATANA"` at `0x10`, and PS1 `"Sony Computer "` at `0x24f8`. It
  also checks RVZ, WIA and WBFS container offsets. `SERIAL_PROBE_SPAN` is
  600 KiB. Source:
  <https://raw.githubusercontent.com/libretro/RetroArch/master/tasks/task_database_cue.c>.
- RetroArch `tasks/task_database.c`, `task_database_iterate_playlist`:
  `.iso`, `.wbfs`, `.rvz` and `.wia` go to serial lookup. `.cue`, `.chd` and
  `.pbp` try a serial and then "fallback to crc". Other files use a CRC
  lookup. Source:
  <https://raw.githubusercontent.com/libretro/RetroArch/master/tasks/task_database.c>.
- rcheevos `src/rhash/hash.c`, `rc_hash_initialize_iterator_iso` lists PS2,
  PSP, PS3, 3DO, Sega CD/Saturn, GameCube and Wii as candidates for `.iso`.
  `rc_hash_iterate` tries each candidate until one hasher succeeds. For an
  unknown extension it logs "No console mapping specified ... trying full file
  hash". Source:
  <https://raw.githubusercontent.com/RetroAchievements/rcheevos/develop/src/rhash/hash.c>.
- RetroAchievements documents what each hasher reads. Examples: GameCube
  hashes the Apploader and its DOL segments. PS1 and PS2 parse `SYSTEM.CNF`,
  using `BOOT=` and `BOOT2=` respectively. PSP hashes `PSP_GAME\PARAMS.SFO`
  and `EBOOT.BIN`. Source:
  <https://docs.retroachievements.org/developer-docs/game-identification.html>.

Known weaknesses in the sources themselves:

- RetroArch's table labels `"PLAYSTATION"` at `0x9320` as PS2 with the comment
  "PS1 CD and PS2 CD". The PS1 entry comes earlier, so table order resolves
  that collision.
- Xenia checks only the media signature. The same signature is used by
  original Xbox discs, so it does not by itself separate Xbox from Xbox 360.
  The Skate 3 flake installs from `default.xex`, which is an Xbox 360
  executable. Checking for that file is inferred, not verified in any source
  examined here.

## What this means for Korri

These are my conclusions, not decisions.

1. **rcheevos is the closest model.** An extension proposes candidates. Each
   candidate validates the content. The first confirmed validation wins. It
   works the same in any folder. This keeps shared extensions without
   exclusive ownership.
2. **Plugins must declare probes, not run them.** Korri plugins are effect-free.
   A plugin can declare a fixed check, such as bytes at an offset. korrid
   reads the bytes. This matches how Dolphin, Xenia and RetroArch express
   their checks as data or short fixed reads.
3. **Exact content hashes stay the strongest proof for specific content.** A
   recompilation needs one dump. A whole-file hash proves that dump. A header
   probe only proves the system.
4. **Folders remain hints at most.** Every content-based project here works
   without them. The folder-based frontends need users to sort files and
   still misclassify a misplaced file.
5. **Cost is bounded.** Magic checks read bytes. RetroArch caps its serial
   search at 600 KiB. A whole-file hash of a 7 GB Xbox 360 image reads all of
   it, which is why Korri already caches hashes by size and modification time.

## Current Korri state that conflicts with the constraint

Commit `13a074832` landed runner `releases` on main. Its `SCRIPTING.md`
section says: "Different systems claiming the same extension still fail with
`ClaimConflict`." This is the behavior the user rejected. The hash route works
only after some plugin claims `.iso` without a conflict.

## Not verified

- Playnite's scanner source code. Only its manual was read.
- LaunchBox and Pegasus. Neither was checked against primary sources.
- How Xenia or another tool tells an Xbox disc from an Xbox 360 disc.
- rcheevos `hash_disc.c` validation code. Only the candidate list and
  iteration in `hash.c` were read.

Supporting search notes from two research subagents are in
`/tmp/korri-prior-art/frontends.md` and
`/tmp/korri-prior-art/content-identification.md`. They lacked a fetch tool
and rest on search excerpts. The claims above were checked against the
fetched source files listed with each claim.
