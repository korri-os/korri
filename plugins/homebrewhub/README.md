# Homebrew Hub

`@korri:homebrewhub` lists curated Game Boy homebrew from Homebrew Hub
(https://hh3.gbdev.io, the gbdev community database
https://github.com/gbdev/database). It is the Homebrew Hub half of legacy
`product/plugins/acquisition-fixtures/index.ts` (legacy `0e4cec9d`), with the
legacy entry fields and search rules. `plugin.ts` performs no effects and asks
korrid for no requests: search and details read the entries in the file, and
`artifact.resolve-download` maps the claim URL (the entry's API record) or a
file URL of the entry to a `FinalDownload`. korrid downloads the file from
`hh3.gbdev.io`, the only host `plugin.nix` declares.

The plugin declares no system. `artifact.system` is `gb`, which the
publisher's `@korri:gambatte` plugin declares and runs
(`plugins/libretro/systems.nix`, `cores.nix`).

## Entries

| Entry | Kept | Evidence (2026-10-20) |
|---|---|---|
| `2048gb` (GB) | yes | API record 200; `2048.gb` 200, 32,768 bytes |
| `basil-termini_2048-advance` (GBA) | no | Its download URL `.../files/2048%20jam.gba` answers 404. Homebrew Hub now lists `files/2048_jam.gba`, but this provider covers Game Boy only. |
| `disabled-downloads` | no | A legacy test fixture; the API answers 404 "No entry found". |

`platform` carries the system id (`gb`), as legacy pico8 did; the legacy
fixture used its old system id `nintendo-gameboy`, which stays in the search
text.

## Licence and terms

The acceptance specimen is **2048gb** by Sanqui: `2048.gb`, 32,768 bytes,
SHA-256 `3ea2376b15b34bd26b10e6b31d2753bf8b086098dbdb5ee84d9c0f03d66d3d7f`
(measured 2026-10-20). Homebrew Hub's record gives the licence as `Zlib`, and
the source repository https://github.com/Sanqui/2048-gb carries the zlib
licence ("Copyright (c) 2014 \"Sanqui\""). Korri fetches the file from Homebrew
Hub onto the person's device. No game bytes are in this repository.

## Checks

```sh
bun test plugins/homebrewhub/plugin.test.ts
plugins/homebrewhub/live-probe.ts      # live entries, small GETs, prints timestamps
plugins/homebrewhub/live-probe.ts --record plugins/homebrewhub/fixtures
```

`fixtures/2048gb.json` is Homebrew Hub's API record for the entry, written by
the probe's `--record` mode. The unit tests check the curated entry against it.
