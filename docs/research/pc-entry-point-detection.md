# Choosing a PC game's entry point

Researched 2026-10-16 for a later slice: Korri proposes the program that starts a folder game, and a person confirms when needed. Not in the first build. Claims marked verified were read in the primary source.

## 1. Manifests name it exactly

| Manifest | Field | Source |
| --- | --- | --- |
| GOG `goggame-*.info` | `playTasks[]` with `isPrimary`, `type: FileTask`, `path` | verified, Lutris #4080, Heroic wiki |
| itch.io `.itch.toml` | `[[actions]]` with `name = "play"`, `path` | <https://itch.io/docs/itch/integrating/manifest.html> |
| Steam | `appinfo.vdf` `config.launch.<id>.executable`; `appmanifest_*.acf` holds only `installdir` | verified, SteamKit |
| Epic `.item` | `LaunchExecutable` under `InstallLocation` | Epic docs |
| Legendary | `installed.json` `install_path` + `executable` | `legendary/cli.py` |
| Linux `.desktop` | `Exec`, `Path` | XDG spec |

## 2. Without a manifest: itch.io `dash`

The most complete open-source implementation (verified, <https://github.com/itchio/dash>, `configure.go`, `helper.go`, `suspicious_names.go`):

1. Sniff headers: PE, ELF, Mach-O, scripts with `#!`, `.bat`, `.love`, HTML5.
2. Tag helpers so they never win: Unity crash handlers, Unreal `CrashReportClient`, Ren'Py `python`/`zsync`, Chromium helpers, .NET `createdump`, anything in `node_modules` or a bundled JRE.
3. Drop binaries for another OS or architecture.
4. Keep only candidates at the shallowest depth.
5. Drop PE files that need elevation, and setup-like names without version info.
6. Prefer GUI-subsystem PE files; on Linux prefer a single shell script.
7. Score from 100, largest first, with penalties (`unins*`, `vcredist*`, `dxwebsetup`, crash handlers).

Playnite and Bottles use exclusion lists of the same kind (verified, Playnite `Programs.cs`, Bottles `manager.py`).

## 3. Engine fingerprints (all verified in `dash`)

| Engine | Fingerprint | Program |
| --- | --- | --- |
| Unity | `<Name>_Data/` beside `<Name>.exe`, `UnityPlayer` | `<Name>.exe` |
| Unreal | `Engine/`, `Content/Paks/*.pak` | root launcher or `Binaries/Win64/*-Shipping.exe` |
| Godot | `<Name>.pck`, or `GDPC` trailer | binary sharing the stem |
| Ren'Py | `game/`, `renpy/` | root `<name>.sh` / `.exe` |
| GameMaker | `data.win` or `assets/game.unx` | adjacent runner |
| RPG Maker | `RPG_RT.ldb`; `Game.ini` with RGSS; `rpg_core.js` | `RPG_RT.exe`, `Game.exe`, NW.js binary |
| LÖVE | `main.lua`, `.love`, or zip trailer | fused binary or `love` |
| DOS | MZ without PE, `.com` up to 64 KiB, `dosbox.conf` | main DOS program |

## 4. What only a person can decide

Single-player against multiplayer programs, one program per graphics API, setup tools that must run first, and editors or SDKs shipped beside the game. No launcher publishes accuracy figures.

## Proposed ranking

Manifest, then engine fingerprint, then helper exclusion, then shallowest depth, then name similarity to the folder, then GUI subsystem and native architecture, then size. Ask the person whenever two candidates remain.
