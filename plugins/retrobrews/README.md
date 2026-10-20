# RetroBrews

`@korri:retrobrews` lists curated Game Boy Advance and NES homebrew from the
RetroBrews collections on GitHub (https://github.com/retrobrews). It is the
RetroBrews half of legacy `product/plugins/acquisition-fixtures/index.ts`
(legacy `0e4cec9d`), with the legacy entry fields and search rules.
`plugin.ts` performs no effects and asks korrid for no requests: search and
details read the entries in the file, and `artifact.resolve-download` maps the
claim URL (the GitHub file page) or the raw file URL to a `FinalDownload` of
the raw file. korrid downloads it from `raw.githubusercontent.com`, the only
host `plugin.nix` declares.

The plugin declares no system. `artifact.system` is `gba` or `nes`, which the
publisher's `@korri:mgba` and `@korri:fceumm` plugins declare and run
(`plugins/libretro/systems.nix`, `cores.nix`).

## Entries

| Entry | System | Kept | Evidence (2026-10-20) |
|---|---|---|---|
| `nes-games:ambushed.nes`, Ambushed by SlyDog Studios | `nes` | yes | raw file 200, 16,400 bytes, iNES header `4e45531a` |
| `gba-games:anguna.gba`, Anguna by Nathan Tolbert | `gba` | yes | raw file 200, 1,710,888 bytes, GBA header logo at 0x04 |

Legacy listed only these two RetroBrews entries; none was dropped.
`platform` carries the system id, as legacy pico8 did; the legacy fixture
used its old system ids (`nintendo-nes`, `nintendo-gameboy-advance`), which
stay in the search text.

## Licence and terms

Each collection's README ends: "The ROMs listed here have been approved for
free distribution on this site/project only. If you want to share it, please
contact owner/developer." Korri therefore fetches each file from RetroBrews
onto the person's own device and never redistributes it. No game bytes are in
this repository. The GitHub repositories declare no licence.

The acceptance specimen is **Anguna** by Nathan Tolbert and Chris Hildenbrand
(the README's credit): `anguna.gba`, 1,710,888 bytes, SHA-256
`cd3b969d5ed6b38b2039c11b894d6db2fc3fad209a7fd046bee241cc66069851`. The NES
entry `ambushed.nes` is 16,400 bytes, SHA-256
`c4efb1e252e39dd957ecc76ff2a868159bb3a6dc66e07addfcdbf9e6660dfb62`. Both
measured 2026-10-20.

## Checks

```sh
bun test plugins/retrobrews/plugin.test.ts
plugins/retrobrews/live-probe.ts       # live entries, small GETs, prints timestamps
plugins/retrobrews/live-probe.ts --record plugins/retrobrews/fixtures
```

`fixtures/<repository>-README.md` are the collections' READMEs, written by the
probe's `--record` mode. The unit tests check each entry's title and author
against them. The probe reads only the first 64 bytes of each game file.
