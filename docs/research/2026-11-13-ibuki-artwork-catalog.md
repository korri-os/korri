# Ibuki artwork: selected system and running bundle differed (#464)

Verified with read-only SSH on 2026-11-13. No service restart, launch, build,
deploy or write was performed on ibuki. No credential file was read.

## Runtime correction

The CLI reports `ee2dc075c`, but `korri version` describes the CLI, not the
running daemon (`clients/cli/src/main.rs`). The observed paths were:

| Check | Observed target |
|---|---|
| `/proc/$(systemctl show korrid -p MainPID --value)/exe` | `/nix/store/5qxngmbddiybpza6prabngbilaqnjacq-korrid-0.0.0/bin/korrid` |
| `/nix/var/nix/gcroots/korri-bundle/active/bin/korrid` | The same `5qxng…` binary, through `/nix/store/b4yax1jr636mv5lj805fq8azjgd5ld3s-korri-bundle-0.0.0` |
| `/run/current-system/sw/bin/korrid` | `/nix/store/wqxai5gbrd7fr1yj8lqzp7kwv4bpbn26-korrid-0.0.0/bin/korrid` |

Local, pinned Nix evaluation verified the source of both output paths:

```sh
nix eval --raw 'git+file:///home/simonwjackson/code/sandbox/korri?rev=460b0c2f258ae8bbeb7bd83e28ff0f5a10b4cef3#packages.x86_64-linux.korrid.outPath'
# /nix/store/5qxngmbddiybpza6prabngbilaqnjacq-korrid-0.0.0
nix eval --raw 'git+file:///home/simonwjackson/code/sandbox/korri?rev=ee2dc075c6ae32e027fad4b100f380e6f0b2bdbe#packages.x86_64-linux.korrid.outPath'
# /nix/store/wqxai5gbrd7fr1yj8lqzp7kwv4bpbn26-korrid-0.0.0
```

`korrid.service` starts `korri-bundle-launch korrid` with
`KORRI_BUNDLE_ACTIVE=/nix/var/nix/gcroots/korri-bundle/active`.
`services/inputd/deploy/README.md` documents that routine host generations
preserve an existing bundle selection. Thus updating the system and restarting
this unit can still run the earlier daemon. Changing that selection is a
separate deployment step; it was deliberately not done during this diagnosis.

## Catalog and source evidence

Captured only `device.yaml`, `catalog/library.yaml`, the root-owned
`/run/korri-plugin-host/enabled-packages.json`, its enumerated plugin source
files, the one cached cart, discovery `ownership.json`, the artwork attempts
file, and read-only `korri --json library games list` output.

The catalog contains 83 games. Its 31 provider-linked acquired/declared games
are all listed by the live RPC. None of their release keys occurs in the
current discovery journal's `origins`; none has an artwork attempt. This
explains their rejection by the **running `460b0c2f2`** implementation, which
requires discovery ownership. Applying that predicate to the captured records
selected 51 games, exactly the 51 ids already in the attempts file. This
excludes all 31 acquired games regardless of their download readiness. The
unchanged attempts-file mtime is consistent with that result; the executable
checks above, not the mtime, establish which binary was running. The remaining
folder game is a SHA-1 CHD, also excluded by that earlier implementation's
SHA-256-only gate.

A local test probe using `ee2dc075c`, the captured catalog, the real installed
plugin sources and the copied cache selected **all 83 games**, including all
31 below. Operation detection for the installed artwork plugin returned
`Ok(true)`. A first probe without the cache selected 82; the only excluded
entry was Celeste. Copying its actual cached cart made it eligible. No
SteamGridDB request was made by either probe.

The installed source packages were:

| Plugin | Package |
|---|---|
| `@korri:starter-pack` | `/nix/store/6142igx9pyflxdz5zh8msdsp39541207-korri-plugin` |
| `@korri:homebrewhub` | `/nix/store/ndvp04a1cjil737xppmf88ikmppgpwaa-korri-plugin` |
| `@korri:retrobrews` | `/nix/store/i1gr91gb5p85mjvng3pyby218hcxc0cz-korri-plugin` |
| `@korri:pico8` | `/nix/store/xwlnzih46aw53278iasp6lfc9iys7fy1-korri-plugin` |
| `@korri:solarus` | `/nix/store/ifz07g8ng3fq0pbk8m2gjkaq1nv1c3c7-korri-plugin` |
| `@korri:zquest-classic` | `/nix/store/bwg69cxmjqp9qyivjbd38qbllvgsd482-korri-plugin` |

Starter-pack declares **Package**, not AccountRoot: `file: { packageFile:
"cartridges", path: … }`. All 27 declarations match the library hashes.
Twenty-four PICO-8 records also have non-missing saved locations under
`/srv/korri/starter-pack/cartridges`. Those saved locations take precedence in
`config/linux_routes.rs`. The other three quests use the package declaration
and its `cartridges` output at
`/nix/store/8n1pdwslxwpk1685m3a34h2rjddg1f14-korri-starter-pack-cartridges/share/starter-pack/cartridges`.

Homebrew Hub and RetroBrews declare pinned Artifact content. Their files need
not exist for the route to enter the catalog: `artifact_target` intentionally
defers readiness to the launch gate. Therefore the absence of a downloaded
`2048.gb` is not a listing error. Celeste uses `acquired_game_content` and the
single ready provider cache.

## Per-game diagnosis

Every row was shown by the live RPC. Every row was rejected by the running
`460b0c2f2` ownership gate, and admitted by the captured `ee2dc075c` probe.
No row was rejected by `ee2dc075c`'s `artwork_games` on this capture.

| Catalog id | Source used for listing / eventual play | `ee2dc075c` artwork admission |
|---|---|---|
| `2048gb` | Artifact `@korri:homebrewhub/2048gb`, file `2048.gb`; not downloaded | Artifact declaration (no readiness requirement) |
| `air-delivery` | Saved file `air_delivery_1-3.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `ambushed` | Artifact `@korri:retrobrews/ambushed`, file `ambushed.nes`; not downloaded | Artifact declaration (no readiness requirement) |
| `anguna` | Artifact `@korri:retrobrews/anguna`, file `anguna.gba`; not downloaded | Artifact declaration (no readiness requirement) |
| `birds-with-guns` | Saved file `birdswithguns-5.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `celeste` | Provider cache `cache/pico8/pico8/b8c78f53…/15133.p8.png` | Ready acquired cache |
| `celeste-classic-2-lani-s-trek` | Saved file `celeste_classic_2-5.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `charge` | Saved file `42930.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `cherry-bomb` | Saved file `cherrybomb-0.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `combo-pool` | Saved file `combopool-0.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `defacto` | Saved file `48406.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `demon-castle` | Saved file `demon_castle-1.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `dungeon-solitaire` | Saved file `dungeonsolitaire-0.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `high-stakes` | Saved file `highstakes-2.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `into-ruins` | Saved file `intoruins.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `lantern-hollow` | Starter-pack Package `cartridges/lantern-hollow.qst` | Package declaration |
| `little-necromancer` | Saved file `littlenecromancer-4.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `mai-chan-s-sweet-buns` | Saved file `56576.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `marble-merger` | Saved file `marble_merger-5.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `perlshaw-s-problems` | Starter-pack Package `cartridges/perlshaws-problems.solarus` | Package declaration |
| `picoracer-2048` | Saved file `16305.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `pigments` | Saved file `pigments-0.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `proserpina-s-quest` | Saved file `proserpinasquest-3.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `solais` | Saved file `d16solais-0.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `spaceman-8` | Saved file `37216.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `ufo-swamp-odyssey` | Saved file `ufo-0.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `vegan-on-a-desert-island` | Starter-pack Package `cartridges/voadi.solarus` | Package declaration |
| `vitreous` | Saved file `vitreous-0.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `whiplash-taxi-co` | Saved file `mot_taxi-7.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `witchbeat` | Saved file `51894.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |
| `worm-nom-nom` | Saved file `23208.p8.png` under `/srv/korri/starter-pack/cartridges`; Package declaration also present | Non-missing location and Package |

## Separate product extension

The coordinator's `guess: true` decision broadens artwork further: a known
library identity does not require any currently installed declaration, cache
or file. That behavior is **not** the explanation for this live incident:
`ee2dc075c` already admits the captured games. The new library-lobby tests
remove locations, cache and declarations on purpose, while retaining the
exact captured `2048gb` and `air-delivery` library records. ADR 0032 records
that new rule and the retained missing-folder guard.
