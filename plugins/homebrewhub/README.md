# Homebrew Hub

`@korri:homebrewhub` declares curated Game Boy homebrew from Homebrew Hub
(https://hh3.gbdev.io, the gbdev community database
https://github.com/gbdev/database) as **declared games with pinned artifacts**
([ADR 0013](../../docs/adr/0013-declared-games-can-come-from-artifacts.md),
[ADR 0012](../../docs/adr/0012-korrid-acquires-declared-artifacts.md)). The
entries come from legacy `product/plugins/acquisition-fixtures/index.ts`
(legacy `0e4cec9d`). The plugin has no `providers` export and no handlers:
the game is in the library before the download, and its first launch (or the
Download action) has korrid download, verify and keep the file in its cache.

| Export | Meaning |
|---|---|
| `artifacts.<key>` | One file `{ url, sha256, bytes }`, no `unpack`. The ready directory `<storage-root>/cache/homebrewhub/<key>/<version>/` then holds the verified file named by the URL's last segment. |
| `games.<key>.releases.<release>` | `system`, `hashes` (the content file's SHA-256, which is the downloaded file itself), `artifacts: [id]`, `file: { artifact: id, path: <file name> }`. |

`plugin.nix` declares nothing. `hosts` is for provider plugins that make
request turns; artifact downloads follow ADR 0012's own policy.

The plugin declares no system. `system: "gb"` is declared by the publisher's
`@korri:gambatte` plugin, whose runner admits it by system
(`plugins/libretro/systems.nix`: extensions `gb`, `dmg`, `sgb`; `cores.nix`:
gambatte `systems = [ "gb" "gbc" ]`).

Homebrew Hub publishes no versions. The artifact `version` (its cache
directory) and the release key are the first 12 hex digits of the file's
SHA-256, so new bytes make a new release and a new directory.

## Entries

| Game | Kept | Evidence |
|---|---|---|
| `2048gb` (GB) | yes | see the pin below |
| `basil-termini_2048-advance` (GBA) | no | Its download URL `.../files/2048%20jam.gba` answers 404. Homebrew Hub now lists `files/2048_jam.gba`, but this plugin covers Game Boy only. |
| `disabled-downloads` | no | A legacy test fixture; the API answers 404 "No entry found". |

## Pin

| Field | Value |
|---|---|
| URL | `https://hh3.gbdev.io/static/database-gb/entries/2048gb/2048.gb` |
| Bytes | 32,768 |
| SHA-256 | `3ea2376b15b34bd26b10e6b31d2753bf8b086098dbdb5ee84d9c0f03d66d3d7f` |
| Measured | 2026-10-20T05:55:56Z (200, no redirect); probe again 06:00:56Z |

## Licence and terms

The acceptance specimen is **2048gb** by Sanqui (the file above). Homebrew
Hub's record gives the licence as `Zlib`, and the source repository
https://github.com/Sanqui/2048-gb carries the zlib licence ("Copyright (c)
2014 \"Sanqui\""). Korri fetches the file from Homebrew Hub onto the person's
device. No game bytes are in this repository.

## Checks

```sh
bun test plugins/homebrewhub/plugin.test.ts
plugins/homebrewhub/live-probe.ts      # GETs the pinned file, checks size, SHA-256, header
plugins/homebrewhub/live-probe.ts --record plugins/homebrewhub/fixtures
```

The unit tests restate korrid's admission rules for `artifacts` and `games`
and check the entry against `fixtures/2048gb.json`, Homebrew Hub's API record,
written by the probe's `--record` mode.
