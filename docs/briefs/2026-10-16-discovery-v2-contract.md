# Discovery v2 build contract

Status: build contract for the first slice, written 2026-10-16. Every builder works to this file. When code and this file disagree, this file wins until the consolidator changes both.

Authority: `CONTEXT.md` (Content and discovery), ADRs 0001 to 0005, `docs/research/content-identification-prior-art.md`, the 2026-10-16 design interview (cited as "interview"), and decisions recorded during the workflow run (cited as "run decision"). Legacy citations name `legacy` branch files.

No compatibility: `discovery.fileReleases`, `FileReleaseDiscoveryClaim`, `sessionControls`, `catalog/games.yaml`, `catalog/releases.yaml`, `app.discovery.registerReceipt`, `app.discovery.removeLocation`, and `korrid catalog import|remove` are removed with no alias, fallback read or dual write (AGENTS.md). The Mini V2 data moves once, through `korrid catalog migrate`.

## 1. Plugin contract

New and changed top-level exports. Export names are single lowercase words (interview). New inner multi-word keys are kebab-case; existing inner keys keep their names (run decision).

```ts
export const systems = {
  nes: {
    id: "nes",                       // existing, must equal the key
    title: "NES",                    // existing
    aliases: ["famicom"],            // existing
    content: {                       // new, optional
      extensions: ["nes"],           // lowercase, no dot; compound allowed ("p8.png")
      layout: ["**/{title}.{ext}"],  // ordered patterns; this is the default when absent
      evidence: {
        bytes: [{ offset: 0, hex: "4e45531a" }], // any entry matching counts
        names: ["^.*\\(Unl\\)"],                 // regex over the file name only
      },
      header: [
        { size: 16, bytes: { offset: 0, hex: "4e45531a" } },
        // or { size: 512, remainder: { divisor: 1024, value: 512 } }
      ],
    },
  },
}

export const groups = {               // new
  gameboy: { id: "gameboy", title: "Game Boy", systems: ["gb", "gbc"] },
}

export const games = {                // new: declared games (ADR 0003)
  jak1: {
    title: "Jak and Daxter: The Precursor Legacy",
    releases: {
      prepared: {
        hashes: ["sha256:…"],         // non-empty; "sha256:<64 hex>" or "sha1:<40 hex>"
        file: "opengoal/jak1/out/jak1/iso/GAME.CGO", // optional, relative to an account root
      },
      original: { system: "ps2", hashes: ["sha1:…"] }, // optional system
    },
  },
}

export const controls = { … }         // was sessionControls; record shape unchanged
```

Rules:

| Rule | Grounding |
| --- | --- |
| A `discovery` export is a plugin error (unknown export). | ADR 0001; brief 2026-10-02 Option A |
| `content.extensions` match the file name case-insensitively; the longest matching suffix wins within one system. | `d6fdfbaae` compound suffix behaviour |
| Extensions `zip`, `7z`, `rar`, `gz`, `tar` are a plugin error. | ADR 0005 "Archives are not supported" |
| `layout` tokens: `{title}`, `{disc}`, `{version}`, `{region}`, `{ext}`, `*` (inside one path segment), `**` (zero or more whole segments). Any other `{…}` is a plugin error. A pattern ending in `/` (a folder release) is a plugin error in this build. | interview (captures from legacy `{name}` templates); folder releases wait for the entry-point picker (run decision) |
| `{ext}` matches one of the system's extensions. `{title}` is required exactly once per pattern. | interview |
| `header` rules: each has `size` and exactly one of `bytes` or `remainder`; the first rule that applies removes `size` bytes before hashing. | clrmamepro detectors, research note |
| `evidence.bytes` and `header.bytes` read at most the first 64 KiB. `hex` is lowercase, even length, at most 64 bytes. | bounded reads (run decision) |
| Rules for one system id combine across enabled plugins: extensions, evidence and header rules are unions; layouts concatenate in plugin-id order with duplicates removed; title and header precedence follow plugin-id order. | brief 2026-10-02 "rules for one system ID combine"; precedence is a run decision |
| Groups combine by id across plugins (union of members, title by plugin-id order). A group id equal to any system id is a registry error. A member that no enabled plugin declares is ignored. | interview; ADR 0001 |
| Declared game keys and release keys are local ids (legacy playable-id segment syntax). The game's provider identity is `{ provider: <plugin id>, ref: <game key> }`. | ADR 0002; legacy `config/playable-id.ts` |
| A declared release `system` is optional. A release with no system is routed only by runner `releases` hashes, as today. | run decision; personal-plugin checks assert "no invented prepared-game system" |
| Content rules do not need an enabled runner: a system is identified even when nothing on the device can play it. | brief 2026-10-02 "identify without a runner" |

Registry queries (Rust, `plugin.rs`):

```rust
impl PluginRegistry {
    pub fn content_rules(&self, system: &str) -> Option<&ContentRules>;      // merged
    pub fn group(&self, id: &str) -> Option<&GroupRecord>;                   // merged
    pub fn pairing_target(&self, id: &str) -> Option<PairingTarget<'_>>;      // system or group
    pub fn declared_games(&self) -> &BTreeMap<String, DeclaredGame>;         // key "<plugin id>/<game key>"
    pub fn declared_hash(&self, hash: &str) -> Vec<DeclaredReleaseRef<'_>>;  // sha256: or sha1:
}
```

`file_release_discovery_claims*` are deleted.

## 2. Device data (`device.yaml`)

`storage` keeps the legacy shape `{ root, path? }` (legacy `records/storage.ts`). New section `sources`, from legacy `docs/brainstorms/2026-06-08-004-retroarch-policy-minimal-v1.example.yaml` (`title`, `kind`, `storage`), plus the one new key `system`:

```yaml
storage:
  gba:
    root: /var/lib/korri/games/gba
sources:
  gba:
    title: Game Boy Advance   # optional
    kind: [files]             # discovery scans only sources whose kind has "files"
    storage: gba
    system: gba               # a system id or a group id (ADR 0001)
```

New section `provider-links`, legacy shape unchanged (legacy `records/provider-link.ts`): `{ provider, playable, release?, refs: [{ kind, value, scope?, targetPart? }] }`. Discovery writes one link per catalog game joined to a declared game: key `<plugin id>/<game key>`, `provider` = plugin id, `playable` = catalog game id, `refs: [{ kind: provider-item-id, value: <game key> }]`.

`locations` keeps the main shape (keyed by release key; `File`, `FileSet`, `Executable`, `Url`, `ProviderRef`). `File.discovery` gains `missing-since` (RFC 3339), set when the file is gone at a scan and cleared when it returns (run decision; sibling of the existing `first-seen-at`). A multi-part release uses `FileSet` with one part per track or disc, in order.

| Delta | Why |
| --- | --- |
| `FileSet` gains optional `discovery` with the same `first-seen-at` and `missing-since` fields as `File`; discovery-owned sets carry it. | Multi-part locations need the same missing and unpair ownership rules as single files (run decision). |
| Discovery includes a `.cue` or `.m3u` descriptor as a `files` part with `role: descriptor`, followed by content parts with `role: track` (cue) or `role: disc` (multi-disc), in order. The descriptor does not enter the identity. Routes launch the descriptor when present, else the first part, and validate every part. | Multi-track launches need the descriptor; the existing `files[].role` seam carries it without another path schema (run decision). |

Prepared files of declared games get one discovery-owned storage per account root, id `account-<user>` (today `account-default` → `<root>/users/default`; account root from `launcher/linux_plugin.rs`) (run decision).

Private state in `<private>/game-discovery/`: `hash-cache.json` (gains `sha1` per entry), `ownership.json`, and a new `attention.json`:

```json
{ "assigned": [{ "source": "gameboy", "pairing": "gameboy\n<canonical source root>", "path": "x.gb", "identity": "sha256:…", "system": "gbc" }],
  "ignored":  [{ "source": "gba", "pairing": "gba\n/var/lib/korri/games/gba", "path": "x.gba", "identity": "sha256:…" },
               { "source": "pc", "pairing": "pc\n<canonical source root>", "kind": "system-unavailable" }] }
```

A file entry stops applying when the identity at that path changes ("ignore lasts until the bytes change", interview). Every assigned or ignored entry stores `pairing`, the selected system/group id, a newline, and the canonical source root. A decision whose pairing differs from the current source does not apply and is dropped at the next save, including after direct edits to `device.yaml`. Account-file attention uses an empty system component and its canonical account root, because an account has no paired system.

## 3. Catalog (`catalog/library.yaml`)

One file, top-level `library:` map, the legacy library item shape (legacy `records/library-item.ts`, `docs/research/legacy-readable-schema-port.md`). It replaces `catalog/games.yaml` and `catalog/releases.yaml` (run decision: file name from legacy `library.yaml`, kept under main's `catalog/` folder).

```yaml
library:
  super-mario-world:                  # readable id (legacy playable-id segment)
    title: Super Mario World
    version-of: …                     # legacy, optional, not written by discovery
    relation: …                       # legacy, optional
    contains: …                       # legacy, optional
    display: …                        # legacy, optional
    metadata: …                       # legacy, optional
    runner: … / families: … / runners: …   # main's existing GamePayload fields
    releases:                         # ordered, oldest first, non-empty
      - id: snes                      # local id, unique in the item
        system: snes                  # optional only for declared releases with no system
        identity: { kind: hash, value: "sha256:…" }
        display: { version: "1.1", region: "USA" }   # captures, when present
```

Deltas from legacy, each named:

| Delta | Why |
| --- | --- |
| No `target` on releases; paths stay in `device.yaml` `locations`. | main split since `d4640c7b7`; device paths are device data |
| `identity.kind` adds `parts` (`value` = `sha256:` of SHA-256 over the ordered part values joined by `\n`, plus `parts: [...]`) and `chd` (`value: "sha1:<rawsha1>"`) next to legacy `hash`. | ADR 0005 (tracks, multi-disc sets, CHD) |
| `system` optional for declared releases with no system. | section 1 |
| Ids are readable: slug of the title (lowercase, `[a-z0-9-]`), `-2`, `-3`… on a clash. Release ids: the system id (declared releases: their key), suffixed on a clash. | interview; legacy playable-id syntax replaces main's ULIDs |
| `collections`, `userData` are not carried. | AGENTS.md "bring in as little as possible" |

Release keys (`locations` keys, play-log `release_id`): the identity `value`; provider routes keep `@ns:name/ref`. `ConfigSnapshot` replaces `games` + `releases` with `library` (ordered items with ordered releases) and gains `sources` and `provider_links`.

Catalog snapshot `GameIdentity` (`contracts/generated/korrid.ts`), in order (ADR 0002):

1. `provider { provider, ref }` when a `provider-links` entry names the game.
2. Else `hash { value }` = the identity value of the game's first release.
3. Else none.

Play history is untouched: `PlayLogStore` stays keyed by `(user_id, game_id)`.

## 4. Behaviour

| Case | Behaviour | Grounding |
| --- | --- | --- |
| What is scanned | Only sources with kind `files` whose `system` is a registered system or group. Nothing else, ever. | ADR 0001 |
| Overlap | `pair` refuses a root equal to, inside, or containing another source root (canonical paths). Overlaps found at scan time (hand-edited files): neither source is scanned; both get an `overlap` attention item. | ADR 0001 |
| Plugin data | `pair` refuses a root equal to, inside, or containing `<root>/users` or the private state root. Traversal never enters them. | ADR 0003 |
| Unknown system | A source whose system no enabled plugin declares is not scanned. Its catalog entries stay (unavailable). Attention item `system-unavailable`. | interview |
| File not claimed | A file whose name matches no member extension is skipped silently. | interview (Mini V2 `.o`, `README.md`) |
| Layout | Patterns are tried in order against the path relative to the source root; first match wins. A claimed file matching no layout is an `unmatched` attention item. | interview |
| Group source | Each file is matched against every member. Evidence ranks: declared hash > `evidence.bytes` > `evidence.names` > extension. One member at the best level wins. Two at the same level: `clash` attention item listing the candidates; nothing is cataloged until assigned. | ADR 0001; brief 2026-10-02 ranking |
| Single-system source | The extension and layout decide; evidence is not required. | ADR 0001 (pairing is the evidence) |
| Identity | Single file: SHA-256 and SHA-1 in one streaming read, after the header rule. `.cue`: its `FILE` tracks in order, each hashed; the cue is not hashed; its tracks are not separate units. `.m3u`: its listed discs in order form one unit; the m3u is not hashed. `.chd`: v5 header `rawsha1` at 0x40; other versions are `unreadable`. | ADR 0005; research note |
| Multi-disc | Units in one folder with the same `{title}`, `{version}` and `{region}` captures and a `{disc}` capture form one `parts` release, ordered by disc (numeric when all numeric). | ADR 0005 |
| New game | A unit with an unknown identity at a new path makes a new game titled from `{title}` (else the `discovery/title.rs` fallback). No folding by title. | ADR 0002 ("stays a separate game until someone links it") |
| Same bytes elsewhere | A unit whose identity already exists adds a location to that release. | minimum-config-layer "copies add locations, not games" |
| Changed bytes | A unit with a new identity at a path that held another release appends a new release to that game; the old release keeps its catalog entry and loses that location (unavailable history). | ADR 0005; CONTEXT "replaced release stays … unavailable" |
| Missing | A location whose file is gone gets `missing-since`; the release stays. | interview |
| Unpair | Removes the source, its discovery-owned storage, its discovery-owned locations, releases left with no location that were only ever found in that source, and games left with no release. Play logs stay. | interview |
| Declared game | For each enabled declared release with `file`, korrid hashes `<account root>/<file>`; a matching hash makes it available there (location under `account-<user>`). A mismatch is a `declared-mismatch` attention item. A discovered release whose SHA-256 or SHA-1 equals a declared hash joins that declared game; the catalog game then gets its `provider-links` entry. A declared game with no available release is not cataloged. | ADR 0003 |
| When | On pairing change, at start-up, and on `rescan`. No file watching. | interview |
| Attention | Per device, visible to every person on the device. Kinds: `clash`, `unmatched`, `unreadable`, `overlap`, `system-unavailable`, `declared-mismatch`. `assign` applies to `clash` only and must name one candidate. `ignore` applies to every kind. | interview |
| Rescan idempotence | An unchanged rescan writes no document bytes. | minimum-config-layer |

## 5. Rust seams

Module boundaries. These are the integrated public identification and publication seams:

```rust
// plugin.rs (contract-types)
pub struct ContentRecord { pub extensions: Vec<String>, pub layout: Option<Vec<String>>, pub evidence: Option<EvidenceRecord>, pub header: Option<Vec<HeaderRule>> }
pub struct ContentRules { pub system: String, pub extensions: Vec<String>, pub layouts: Vec<String>, pub evidence: EvidenceRecord, pub header: Vec<HeaderRule> }
pub struct GroupRecord { pub id: String, pub title: Option<String>, pub systems: Vec<String> }
pub struct DeclaredGame { pub provider: String, pub key: String, pub title: String, pub releases: BTreeMap<String, DeclaredRelease> }
pub struct DeclaredRelease { pub system: Option<String>, pub hashes: Vec<String>, pub file: Option<String> }
pub enum PairingTarget<'a> { System(&'a ContentRules), Group { group: &'a GroupRecord, members: Vec<&'a ContentRules> } }

// discovery/identify/ (identify)
pub fn identify_source(source: &SourceInput<'_>, registry: &PluginRegistry, decisions: &AttentionDecisions, cache: &mut HashCache, budget: &TraversalBudget) -> SourceScan;
pub fn resolve_declared(accounts: &[AccountRoot], registry: &PluginRegistry, cache: &mut HashCache) -> DeclaredScan;
pub fn identity_of(path: &Path, rules: &ContentRules, cache: &mut HashCache) -> Result<ContentIdentity, IdentifyError>;

// discovery/sources.rs, attention.rs, scanner.rs, coordinator.rs (sources)
pub fn validate_pairing(request: &PairingRequest<'_>, existing: &[SourceView], registry: &PluginRegistry) -> Result<(), PairingError>;
pub fn scan(sources: &[SourceView], accounts: &[AccountRoot], excluded_roots: &[PathBuf], registry: &PluginRegistry, decisions: &AttentionDecisions, cache: &mut HashCache, budget: &TraversalBudget) -> ScanReport;

// discovery/reconcile*, config/library.rs (catalog)
pub fn reconcile(report: &ScanReport, current: &ConfigSnapshot, now: &str) -> CatalogChange; // pure
// publish stays the existing conflict-checked atomic write path

// catalog_migrate.rs (migration)
pub fn plan(readable_root: &Path, private_root: &Path, registry: &PluginRegistry) -> Result<MigrationPlan, MigrationError>;
pub fn apply(plan: &MigrationPlan, readable_root: &Path, private_root: &Path, registry: &PluginRegistry) -> Result<MigrationReport, MigrationError>;
```

`FoundUnit.parts` retains nested descriptors as launch dependencies. `FoundUnit.tracks` records paths named by cue `FILE` records, so the publisher assigns track roles without guessing from file extensions (ADR 0005).

Declared prepared files report one status each: found (with identity), absent, mismatch, or unreadable. Only absent sets `missing-since`; mismatch and unreadable leave the existing location unchanged and report attention.

`identify::HashCache` owns both digests. `SourceInput.excluded_roots` applies to traversal and descriptor references. `ScanReport` carries completed scans, skipped source ids, declared findings and games, account roots, unpaired source ids, prior release origins, and `paths: BTreeMap<String, PathRecord { identity, files }>`. A truncated or partly unreadable traversal is skipped, so reconciliation leaves its catalog records unchanged.

The catalog `DiscoveryCoordinator` owns `pair`, `change`, `unpair`, `rescan`, `run`, and conflict-checked writes. `source_views()` is read-only and can run while its shared `write_lock()` is held. Change and unpair clear decisions before scanning and save the cleared decisions after successful publication. Migration uses the same evidence ranking as identification and carries titles through that publisher without changing ownership records.

The shared data types (`SourceInput`, `SourceView`, `AccountRoot`, `FoundUnit`, `ContentIdentity`, `SourceScan`, `DeclaredScan`, `DeclaredFile`, `DeclaredFileStatus`, `ScanReport`, `AttentionFinding`, `AttentionKind`, `AttentionDecisions`) live in `discovery/identify/types.rs` and `discovery/attention.rs`, committed by the Foundation stage. Change their fields only through the consolidator.

## 6. RPC

Brain mode only. (Amended by ADR 0017: a device korrid with an installed library root also runs these on its own library.) Responses use the existing `{ _tag: Ok|Err, payload }` outcome style.

| `_tag` | Request | Response | Who |
| --- | --- | --- | --- |
| `app.discovery.snapshot` | `{}` | `DiscoverySnapshotOutcome` | any local caller |
| `app.discovery.pair` | `{ receipt, system }` | `DiscoverySnapshotOutcome` | owner (`brain_caller_is_owner`) |
| `app.discovery.change` | `{ source, system }` | `DiscoverySnapshotOutcome` | owner |
| `app.discovery.unpair` | `{ source }` | `DiscoverySnapshotOutcome` | owner |
| `app.discovery.rescan` | `{}` | `DiscoverySnapshotOutcome` | any local caller |
| `app.attention.list` | `{}` | `AttentionOutcome` | any local caller |
| `app.attention.assign` | `{ item, system }` | `AttentionOutcome` | any local caller |
| `app.attention.ignore` | `{ item }` | `AttentionOutcome` | any local caller |

“Any local caller” means no owner check, not bypassing portal permission profiles. Attention list is readable where the discovery snapshot is readable. Rescan, assign and ignore require `Full`; pair, change and unpair also require `brain_caller_is_owner`.

```rust
pub struct DiscoverySnapshot { pub generation: String, pub state: DiscoveryState, pub sources: Vec<DiscoverySource>, pub attention: u32, pub diagnostics: Vec<DiscoveryDiagnostic> }
pub struct DiscoverySource { pub id: String, pub label: String, pub system: String, pub status: DiscoverySourceStatus } // label = title, else system or group title
pub enum DiscoverySourceStatus { Ready {}, Overlap {}, SystemUnavailable {} }   // tagged like DiscoveryState
pub struct AttentionList { pub items: Vec<AttentionItem> }
pub struct AttentionItem { pub id: String, pub kind: String, pub source: String, pub path: Option<String>, pub candidates: Vec<String>, pub message: String }
```

`AttentionItem.id` is the first 16 hex characters of SHA-256 over `kind\nsource\npath`. `change` keeps the folder and replaces the system: it behaves as `unpair` then `pair` of the same root. Errors: `NotOwner`, `SourceOverlap`, `PluginData`, `UnknownSystem`, `UnknownSource`, `UnknownItem`, `NotACandidate`, plus the existing receipt errors. The portal client (`clients/portal/src/korrid/client.ts`, `surface/use-launchables.ts`) moves to `sources` and `unpair`; it adds no pairing screen (Pico screens are later).

Historical CLI contract (2026-10-16): offline `korrid catalog pair <directory> <system>`, `korrid catalog unpair <source>`, `korrid catalog rescan`, `korrid catalog migrate [--apply] [--plan <file>]`.

Superseded by ADR 0031: use live `korri library folders add`, `remove` and
`rescan` with korrid running. Only `korrid catalog migrate` remains a hidden
offline operator helper. See `services/korrid/CATALOG-IMPORT.md`. The persisted
schema in this brief is unchanged by that wire/command terminology cut.

## 7. Migration (`korrid catalog migrate`)

Dry run by default; `--apply` acts. It reads `device.yaml`, `catalog/games.yaml`, `catalog/releases.yaml` and `<private>/game-discovery/` of the old layout and writes a plan (JSON, plus a readable summary) with: every kept game (old id, new id, title, files), every dropped entry and why (`plugin-data`, `unclaimed`, `duplicate`), every file move (from, to), every new source, and play-log renames.

- Each kept file moves to `<root>/games/<system or group id>/<path relative to its old storage>` (run decision: folder names never decide a system, so the name is cosmetic). Its system is chosen by the new rules over every enabled system; a file with no single system and no group holding all its candidates is listed as `unresolved` and left in place.
- Entries under `<root>/users/` are dropped; prepared files that match a declared release hash move to that release's `file` path in the same account root.
- A `.cue` owns every `FILE` target. An `.m3u` owns its recursive cue, playlist, and content dependencies. Migration uses identify's parser and bounds, preserves their source-relative paths, and preflights the whole unit. Missing, unreadable, cyclic, escaping, or unsafe dependencies leave the unit unresolved with no partial moves.
- Dry run identifies the planned post-move file layout and runs pure reconciliation in memory. It records the resulting new game ids and play-log targets without writing catalog, device, cache, or log state. Apply recomputes the plan before moving data and refuses changed ids or targets. The real published catalog must also match the approved ids before log renames.
- Titles of kept games carry over. Play logs move from the old game id to the new one (`<private>/play-log`), matched through the old and new location of the same file.
- Apply order: move files, write `sources` and `storage`, remove the old catalog files and old discovery-owned storage, run one scan and reconcile, rename play logs, write the report next to the plan. The operator makes backups first. Any error stops before the next step, and the report lists what was done so far, including the move list for undoing it.

For a declared prepared file, migration moves the whole native installation. It strips the longest common trailing path segments from the old account-relative file and the declared file. The remaining prefixes are the old and new installation directories. An already aligned file needs no move. An empty old prefix, an existing destination, or overlapping installation claims are unresolved; apply refuses unresolved plans before moving data. Directory moves preserve companion files, native configuration, patches, and saves.

## 8. Ownership

| Builder | Owns | Must not touch | Adds tests for |
| --- | --- | --- | --- |
| `contract-types` | `services/korrid/src/plugin.rs`, plugin tests and fixtures, `services/korrid/SCRIPTING.md`, test fixtures that declare plugins | `discovery/`, `config/`, `lib.rs` RPC | export parsing, every plugin error in section 1, combining rules and groups, `declared_hash` |
| `identify` | `services/korrid/src/discovery/identify/` (not `types.rs` fields) | everything else | layouts, ranking, clash, header skip, SHA-1, cue, m3u, CHD, multi-disc, `.o`/`README.md` skipped, bounded reads, budgets |
| `sources` | `discovery/scanner.rs`, `discovery/coordinator.rs`, `discovery/sources.rs`, `discovery/attention.rs` logic, discovery and attention RPC in `lib.rs`, `catalog_cli.rs` pair/unpair/rescan, portal client discovery calls | `identify/`, `reconcile*`, `config/` | overlap, plugin-data refusal, owner-only, attention decisions, triggers, portal client |
| `catalog` | `discovery/reconcile*`, `config/` (new `config/library.rs`), `config/linux_routes.rs`, `config/resolver.rs`, catalog snapshot and `GameIdentity`, `clients/portal/src/launchables/fold-games.ts` if needed | `identify/`, `scanner.rs`, `plugin.rs` | history, missing, unpair removal, declared join, provider links, ids, idempotent rescan, route resolution |
| `core-plugins` | `korri-plugins`: `plugins/libretro/*`, `plugins/solarus`, `plugins/retroarch` if needed, checks, examples | `korri` | generated content for every system, header rules (NES, Lynx, 7800, SNES, FDS), the four groups, no archive extensions |
| `personal-plugins` | `korri-personal-plugins`: `plugins/*/plugin.ts`, `nix/*-check.ts`, READMEs | `korri` | no `discovery`; `controls`; declared games for OpenGOAL (Jak 1, Jak II with its Renegade release, Jak 3) and Fallout CE 1 and 2; content rules for systems they declare |
| `migration` | `services/korrid/src/catalog_migrate.rs`, its CLI dispatch line, its tests | other modules (call their public functions) | dry run, apply, drop rules, play-log renames, unresolved files |

Every builder: commit on its own branch, run only focused checks (`cargo test -p korrid <filter>` or the repository's focused check), never a VM test, never compile on a device, ask no person. A gap this file does not answer goes to the coordinator, who decides by the ADRs.

## 9. Out of scope

Links (ADR 0004), the PC entry-point picker, folder releases, add-ons, reading inside discs (ISO 9660, serials), archives, and Pico screens for pairing and attention.
