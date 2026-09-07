# Config cascade, discussion notes

Date: 2026-09-06. Status: discussion complete for a first slice. No code.

Worked examples live outside the repo at `aso:~/config-cascade-examples/`.
`v3/` is the current full shape. `minted/` shows the game-id merge across
two devices. `full-cascade/` is legacy's ten-layer fold transcribed from
its own tests. Read `v3/README.md` first.

## What Simon is trying to get, in plain words

Some facts are true regardless. A game's name. Which system it belongs to.

Some facts belong to a piece of hardware. 3x scale makes sense on one device
and 1x on another.

Some things are a person's opinion. A shader.

Content facts travel with the content. Hardware facts stay with the hardware.
Opinions travel with the person. User configs are isolated from each other.

Config files can be anywhere, in any number. An SD card will go into more than
one machine. A machine can hold more than one card. Nobody controls how many
files or where. There are sensible defaults and anything can overwrite them.

## What is true on `main` today

- Two fixed files under the machine's storage root: `config.yaml` and
  `library.yaml`. No other path is read.
- The legacy schema is fully decoded, including `users`, `presets`,
  `inherit`, `byLauncher`, `preferences`, `env`. `classify_snapshot_support`
  rejects any snapshot that uses them. Only plugin enablement runs layered.
- Discovery (`discovery/reconcile.rs` line 962) already writes a full legacy
  `library.<slug>` record: title, release id, system, `target`
  (storage, path, first-seen-at), `identity` sha256, `launch.use`,
  `launch.runtime`. It is the one producer.
- Launchers and runtimes on Bandai come from bundled plugins
  (`plugin_policy.rs`), not from `config.yaml`. Selection is by
  `launchers.<id>.systems`.
- On Zao both YAML files are `{}`. Games come from Nix-generated `host.toml`.
  A dead legacy file sits at `/var/lib/korri/config/local.korri.yaml`.
- proseQL deep-merges any number of files into one tree by record id.
  `main` gives it one root and two files.

## Two separate merges

| | Files into one tree | Tree into one launch |
|---|---|---|
| Question | Which file wins for the same record | Which layer wins for one launch |
| Exists | proseQL, switched off | Schema only, no code |
| Order key | Root order | Legacy's fixed layer list |
| Same-key rule | Later root replaces a scalar, deep-merges a map | Per field: maps merge, lists concat, scalars last-win |

## Decided

### Three homes, one owner each

| Home | Owner | Roams | Holds |
|---|---|---|---|
| `device.yaml` | this machine | no | `host`, `storage`, `launchers`, `runtimes`, `hooks`, `profiles` (hardware bundles), `locations` |
| `catalog/` | nobody | with content | `games`, `releases`, `aliases`, `systems`, `providers`, `retired` |
| `users/<npub>/` | one person | yes | `library.yaml` (game ids + launch opinions), `me.yaml` (legacy `UserPayload`) |

Only `device.yaml` may carry commands. A person's inline hooks are dropped
with a diagnostic unless `host.hooks.trust-removable`. `hooks.use` naming a
device profile is allowed (open: is that a command).

### Identity

- Release key is always `sha256:<hex64>` (legacy `ArtifactId`), or a
  provider ref `@ns:name/ref`. Any other digest prefix as a key is a decode
  error. Other digests are `digests` aliases on the record. A CRC-only writer
  uses `catalog.aliases`.
- Multi-file releases: key is
  `sha256(json(sorted_by_crc32([{size, crc32}] for files where role != index)))`.
  No path in the manifest, so two devices agree. CHD: key from the header
  data-sha1. `releases.<sha>.identity: file | manifest | chd | provider`
  records the derivation.
- Game id is a ULID minted by discovery into `catalog/games.yaml` the first
  time a release has no game. Shared by every person on the device.
- Merge rule when two catalogs meet: two games that share a release sha are
  one game. Older ULID keeps the id. Release lists union. Younger id goes in
  `catalog.games.retired` (append-only). Person trees are rewritten from
  `retired` on ingest. Same-field conflict: older wins, diagnostic.
- Two dumps nobody grouped stay two games. Legacy reached the same limit.
- Romhacks are `kind: patch` with `facets.compatibility.expectedBaseDigests`
  (legacy `ArtifactKind`), not full ROMs.

### Ownership and opinions

- A person's `library.yaml` is a map keyed by game id. The value is what
  legacy put on `library.<slug>`: `launch` (use, runtime, input, settings,
  overrides), `contains.<sub>`, `releases.<sha>`, inheritable fields.
  Empty value means "I have it, nothing to say".
- `releases.<sha>.prefer: true` picks a dump when several are located.
- Which app opens a release is a preference: `library.<game>.launch.use`,
  else `me.yaml: launch.app`, else `host.launch.app`, else the launcher whose
  `systems` contains the release's system. No `bySystem` block; legacy's
  `systems` on the launcher is the mechanism.
- Where bytes are is a device fact: `device.yaml: locations.<sha>[]`, legacy
  target shape minus `kind`, written by discovery. A game is launchable when
  any release has a complete location.
- `me.yaml: byDevice.<device-key>` holds a roaming person's opinions about
  one device. `host.key` is the target. Folds above device defaults, below
  the person's per-game layer.
- `profiles` split by owner: hardware bundles on the device, opinion bundles
  as `presets` on the person.

### Names

Legacy names throughout. New names, each with a reason, are listed in
`v3/README.md` under "Genuinely new". Dropped: `provider-links`, `sources`,
`launcher`/`core` aliases, `library.<slug>` as a map of full records.

### Cards (agreed, not yet shown)

- A card is found by a fixed entry directory name. Only YAML inside it is
  read. Nothing below it. Any file name inside it. Depth cap on the search.
- Content does not have to live under the entry directory.
- Root order: machine first, cards after by an on-card id, roots within a
  card by path. Same scalar set by two roots: later wins, diagnostic.
- A card carries `catalog/` and optionally `users/<npub>/`. `device.yaml`
  sections on a card are dropped with a diagnostic.
- A Nix-rendered read-only root can sit ahead of `device.yaml`.

### Cleanup

- Delete `/var/lib/korri/config/local.korri.yaml` on Zao.
- Fold `host.toml` into the YAML graph. Own slice.

## Minimum layer to build first

One producer, one consumer, no cascade, no users, no cards.

1. Discovery writes `catalog/releases.yaml` (record minus `target` and
   `launch`) and mints a ULID into `catalog/games.yaml` with `title`.
2. Discovery writes `target` into `device.yaml: locations.<sha>[]`.
3. `launch.use` and `launch.runtime` are dropped from what discovery writes.
   Plugins already declare `systems` on launchers and `supports.systems` on
   runtimes.
4. `config.yaml` becomes `device.yaml`. Sections unchanged.
5. Catalog snapshot reads games, releases, locations, launcher by system.
   Replaces `resolver.rs`'s walk over `library.<slug>.releases[]`.

Migration: Bandai deletes `library.yaml`, renames `config.yaml`, rescans.
Zao deletes two `{}` files and the dead legacy file, renames. Play stats on
Bandai key by the old slug: reset.

Defaults if unstated: mint ULID (not slug). `retired` added with the merge
slice, not now.

Files after the minimum layer on Bandai:

```
/storage/emulated/0/korri/
  device.yaml          host, storage, locations
  catalog/games.yaml   games: <ulid>: { title, releases: [sha] }
  catalog/releases.yaml releases: <sha>: { game, system }
```

## Open, do not block the minimum layer

| Question | Where it lands |
|---|---|
| Who is launching | per-person isolation handoff (`.pi-web/handoffs/4a326ecb-…`) |
| Where a person's tree syncs from | relay slice |
| Card entry directory name; card ordering id | cards slice |
| Who edits `catalog/games.yaml` by hand | cards or users slice |
| Contained playables on a multi-release game | when a real case exists |
| `inherit: false` on a person record dropping device facts | fold slice |
| `hooks.use` from a person: is it a command | fold slice |
| Runtime fallback when nobody names one | fold slice |
| Executable and URL launchers | when a real case exists |
| Typed diagnostic list | before the fold slice |
| `favorites` and `hidden` union after merge | merge slice |
| `storage.<id>.path` for saves and states | isolation handoff |
| Whether "not executable in this slice" becomes a user message | fold slice |

## Grounding

- `services/korrid/src/config/{mod,snapshot,settings,storage,resolver}.rs`
- `services/korrid/src/discovery/reconcile.rs` lines 940-1010 (the producer)
- `services/korrid/src/plugin_policy.rs`
- `legacy:product/platform/library/config/cascade-resolver.ts`,
  `inheritable-fields.ts`, `records/*.ts`
- `legacy:product/platform/protocol/artifact/artifact.ts`
- `legacy:docs/research/game-library-entity-resolution-deduplication.md` §5
- `legacy:docs/briefs/2026-05-21-korri-config-cascade-brief.md`
- `docs/research/legacy-readable-schema-port.md`
- `.pi-web/handoffs/47c1ef68-9eec-4628-8149-7263315351f3.md`
- `aso:~/config-cascade-examples/{v3,minted,full-cascade}/`
