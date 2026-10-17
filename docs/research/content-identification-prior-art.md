# Content identification: prior art

Researched 2026-10-16 for the discovery redesign (ADR 0001, 0002, 0005). Claims marked verified were read in the primary source.

## Hash lists

| | No-Intro | Redump |
| --- | --- | --- |
| Terms | Data Usage License: copy, modify and redistribute, no attribution needed (verified, <https://datomatic.no-intro.org/terms.html>) | Metadata declared public domain (verified, <https://redump.info/about>) |
| Size | ~50–80 MB zipped, ~250–350 MB as XML | ~30–50 MB zipped, ~250–350 MB as XML |

libretro-database compiles both into `.rdb` files that RetroArch downloads at run time; the repository has no licence file (verified, <https://github.com/libretro/libretro-database>).

## Cartridge headers

No-Intro hashes the payload without the header (verified). clrmamepro header detectors declare the rule (<https://mamedev.emulab.it/clrmamepro/docs/xmlheaders.txt>):

| Format | Header | Detection | Detector |
| --- | --- | --- | --- |
| NES iNES | 16 bytes | `4E45531A` at 0 | `nes.xml` |
| Lynx `.lnx` | 64 bytes | `LYNX` at 0, or `BS9` at 6 | `lynx.xml` |
| Atari 7800 `.a78` | 128 bytes | `ATARI7800` at 1 | `a7800.xml` |
| SNES copier | 512 bytes | size mod 1024 = 512 | `snes.xml` |
| FDS | 16 bytes | `4644531A` at 0 | `fds.xml` |

Detector shape: `<rule start_offset end_offset operation>` with `<data offset value result/>` and `<file size="PO2" result/>`; offsets in hex.

## Discs

- Redump hashes each track separately and never the `.cue` (verified, <https://wiki.redump.info/Redumper>).
- CHD v5 stores `rawsha1` (raw data, at offset 0x40) and `sha1` (data plus metadata, at 0x54); `rawsha1` survives recompression (verified, MAME `chd.h`).
- RetroArch reads a disc serial from track 1 first and falls back to the CRC32 of the primary data track; `.cue` parsing removes the referenced `.bin` files from the scan (verified, `tasks/task_database_cue.c`).

## Archives

RetroArch matches the archive's CRC, then each inner file's CRC (`archive.zip#inner.rom`). ES-DE hashes only the outer archive and recommends unzipping for hash lookups (verified, `tasks/task_database.c`, ES-DE `ScreenScraper.cpp`).

## Path patterns that name a title

Steam ROM Manager's glob parser captures `${title}`, for example `${title}/*@(.exe)` (verified, `src/models/parser.model.ts`, `files/presets/Non Steam PC.json`). ES-DE treats a folder named like a file (`Game.m3u/`) as one game. Playnite and LaunchBox use file or parent-folder names, with no pattern syntax.
