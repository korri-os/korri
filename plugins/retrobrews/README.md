# RetroBrews

`@korri:retrobrews` declares curated Game Boy Advance and NES homebrew from
the RetroBrews collections on GitHub (https://github.com/retrobrews) as
**declared games with pinned artifacts**
([ADR 0013](../../docs/adr/0013-declared-games-can-come-from-artifacts.md),
[ADR 0012](../../docs/adr/0012-korrid-acquires-declared-artifacts.md)). The
entries come from legacy `product/plugins/acquisition-fixtures/index.ts`
(legacy `0e4cec9d`). The plugin has no `providers` export and no handlers:
each game is in the library before the download, and its first launch (or the
Download action) has korrid download, verify and keep the file in its cache.

| Export | Meaning |
|---|---|
| `artifacts.<key>` | One file `{ url, sha256, bytes }`, no `unpack`. The ready directory `<storage-root>/cache/retrobrews/<key>/<version>/` then holds the verified file named by the URL's last segment. |
| `games.<key>.releases.<release>` | `system`, `hashes` (the content file's SHA-256, which is the downloaded file itself), `artifacts: [id]`, `file: { artifact: id, path: <file name> }`. |

`plugin.nix` declares nothing. `hosts` is for provider plugins that make
request turns; artifact downloads follow ADR 0012's own policy.

The plugin declares no system. `gba` and `nes` are declared by the
publisher's `@korri:mgba` and `@korri:fceumm` plugins, whose runners admit
them by system (`plugins/libretro/systems.nix`: `gba` extensions `gba`,
`agb` with the logo at 0x04; `nes` extension `nes` with the `NES\x1a`
header; `cores.nix`: mgba `[ "gba" ]`, fceumm `[ "nes" "fds" ]`).

RetroBrews publishes no versions. Each artifact `version` (its cache
directory) and release key are the first 12 hex digits of the file's SHA-256,
so new bytes make a new release and a new directory. Game keys are the file
stems; legacy ids (`nes-games:ambushed.nes`) are not valid local keys.

## Pins

| Game | System | URL | Bytes | SHA-256 | Measured |
|---|---|---|---|---|---|
| Ambushed (SlyDog Studios) | `nes` | `https://raw.githubusercontent.com/retrobrews/nes-games/master/ambushed.nes` | 16,400 | `c4efb1e252e39dd957ecc76ff2a868159bb3a6dc66e07addfcdbf9e6660dfb62` | 2026-10-20T05:55:56Z |
| Anguna (Nathan Tolbert, Chris Hildenbrand) | `gba` | `https://raw.githubusercontent.com/retrobrews/gba-games/master/anguna.gba` | 1,710,888 | `cd3b969d5ed6b38b2039c11b894d6db2fc3fad209a7fd046bee241cc66069851` | 2026-10-20T05:58:00Z |

All answered 200 with no redirect. The first Anguna GET at 05:55:57Z answered
503 (a 461-byte error body); three retries from 05:58:00Z returned the pinned
bytes. The probe at 06:00:57Z took about three minutes for Anguna. Legacy
listed only these two RetroBrews entries; none was dropped.

## Licence and terms

Each collection's README ends: "The ROMs listed here have been approved for
free distribution on this site/project only. If you want to share it, please
contact owner/developer." Korri therefore fetches each file from RetroBrews
onto the person's own device and never redistributes it. No game bytes are in
this repository. The GitHub repositories declare no licence. The acceptance
specimen is **Anguna**.

## Checks

```sh
bun test plugins/retrobrews/plugin.test.ts
plugins/retrobrews/live-probe.ts       # GETs the pinned files, checks size, SHA-256, header
plugins/retrobrews/live-probe.ts --record plugins/retrobrews/fixtures
```

The unit tests restate korrid's admission rules for `artifacts` and `games`
and check each title and author against `fixtures/<repository>-README.md`,
written by the probe's `--record` mode.
