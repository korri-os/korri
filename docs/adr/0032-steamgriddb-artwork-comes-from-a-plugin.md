---
status: proposed (ready for review)
---

# SteamGridDB artwork comes from a plugin, not from korrid

**Owner decisions, 2026-10-23:**

- Verbatim: "It should be a plug-in in the plug-in repo. It should not be
  baked into the main application."
- Per-plugin state lives "In korrid's private folder, one subfolder per
  plugin, named by the plugin's id".
- Publish the plugin without asking SteamGridDB first (section 7).

**Coordinator decisions, 2026-10-23:**

- korrid serves cover bytes on a `GET` route, with the capability in a
  request header (section 6).
- The names stay `game-assets.candidates` and `@korri:steamgriddb`.

The proposal: korrid loses its built-in SteamGridDB fetcher. A plugin in
`korri-os/plugins` finds cover art for a game and returns image candidates.
korrid makes every request, adds the person's API key itself, downloads the
image, checks it and stores it. The plugin never sees the key and performs no
effects (AGENTS.md, "Plugin boundaries"). The API key becomes a plugin secret,
the general mechanism in
[ADR 0033](0033-korrid-holds-plugin-secrets-and-plugins-never-see-them.md).
The built-in title rewrite is dropped. One clean cut removes the Rust fetcher,
and a one-off migration moves each device's real data.

No open owner questions.

## What exists today

Read on main at `a2151a47e` unless a line says otherwise. None of the cited
files changed since `ad3da260f`, where this ADR was first written.

| Part | Where | What it does |
|---|---|---|
| Fetcher | `services/korrid/src/enrichment/steamgriddb.rs` | `run` (:235) reads the key, skips games with a recorded attempt, and calls `enrich_one` (:387) for at most 8 games (:23). It searches `search/autocomplete/{title}` (:601, `bearer_auth` :612). It accepts one exact, verified name match (`exact_verified_match`, :701). It lists `grids/game/{id}` (:619, `bearer_auth` :630) with `dimensions=512x512`, `types=static` and four styles (:631–635). It drops `nsfw`, `humor` and `epilepsy` tags and keeps the top score (`choose_grid`, :728). |
| Title rewrite | `steamgriddb.rs` :447, `discovery/reconcile.rs` :724 | Writes SteamGridDB's name into `catalog/library.yaml`. This changes the game's `release_fingerprint` (:744–751). |
| Retries | `steamgriddb.rs` :516 | Retries 429 and 5xx twice, with `Retry-After` up to 60 s. A 401 stops the pass and records nothing. |
| Attempts | `steamgriddb.rs` :21–22, :802 | `<private>/steamgriddb-enrichment/attempts.json`. The key is a SHA-256 of playable id, release id, release fingerprint and ROM identity. A rescan clears `transient` entries (`discovery/coordinator.rs` :540). A key change clears every entry except `assigned` (`lib.rs` :3447, :3476). |
| Image download | `enrichment/steamgriddb/asset_download.rs` (not a top-level file) | HTTPS only, a public-address check with a 2 s DNS deadline, no redirects, 10 s timeout, 5 MiB cap. It has no host list: any public HTTPS host passes. |
| Storage | `services/korrid/src/game_assets.rs` | `<private>/game-assets/blobs/<sha256>.<ext>` and `<private>/game-assets/assignments.json`, one assignment per playable id. `validate_image` (:197) admits static PNG, JPEG and WebP only, square, at most 4096 px, at most 5 MiB. `assign_tile` (:128) writes role `"tile"` and `SteamGridDbProvenance { provider: "steamgriddb", game_id, grid_id, sha256, width, height, format }`. |
| Trigger | `discovery/coordinator.rs` :275, :546, :633 | After each successful discovery pass, under the `Enriching` phase. Brain mode and the device korrid both build it (ADR 0017, `host/mod.rs` :528). |
| Key | `config/settings.rs` :29, :169–244 | `<private>/steamgriddb.credential`. The snapshot carries only `SecretSettingStatus` (`Configured`, `NotConfigured`). |
| Key RPCs | `lib.rs` :1378–1381, :3435–3497, `authorization.rs` :246–247 | `system.settings.steamgriddbCredential.set { token }` and `.clear {}`, owner-device only. **Both answer `OperationUnsupported` in host mode** (:3459–3464, :3490–3495), and host mode does not list its editable id (:3765–3773). Every device runs host mode (ADR 0017), so on a device no person can set the key through the portal. |
| Settings field | `clients/portal/src/surface/settings-model.ts` :141–160 | "SteamGridDB API key" row, `sensitiveText`, 256 characters, "Clear saved key". Client methods are in `client.ts` :124–125 and :428–445. Dispatch is in `use-launchables.ts` :517–521. |
| Cover id | `launcher/mod.rs` :122, `launcher/types.rs` :19, `contracts/generated/korrid.ts` :657 | `LocalGame.coverAssetId` is the `tile` asset id. |
| Cover URL | `contracts/surface/korri-surface.ts` :85 | `SurfaceGame.coverArtUrl`. No portal code sets it. `localGameAssetUrl` left with the Android bridge in `b272477c3` (2026-09-16). |
| Blob serving | `lib.rs` :4299–4301 | The local router serves only `POST /rpc`, and CORS allows only `POST`. Nothing serves an asset blob. Linux shows no covers. |

Legacy baseline (`git show legacy:<path>`):

- `product/platform/library/config/records/game-asset.ts`: `GameAssetRecord`,
  and `GameAssetSource { provider, id?, url? }` with `provider` one of
  `korri`, `manual`, `rocknix`, `steamgriddb`. `url` must be http(s) with no
  credentials, query or fragment.
- `game-asset-assignment.ts`: `GameAssetRole` is `tile`, `banner`, `poster`,
  `hero`, `logo`, `screenshot`.
- `product/platform/library/game-assets/candidate-cache.ts`:
  `GameAssetCandidate { candidateId, gameId, role, width, height, source }`.
  Its producer was `tools/importers/steamgriddb/fetch-korri-steamgriddb-art`,
  which wrote the SteamGridDB image id as `source.id` (`parseManifestLine`
  reads `imageId`). It mapped 1x1 to `tile`, 92x43 to `banner`, 2x3 to
  `poster`.
- `product/apps/portal/api/game-assets/list-candidates.rpc.ts`:
  `app.game-assets.candidates.list` answers `{ candidates }`.
- `product/platform/protocol/acquisition/plugin.ts`: `PluginMetadata` has
  `credentialRequired: boolean`.
- `product/platform/acquisition/plugin-runtime.ts` :73–81: plugins read
  credentials from `process.env` and saw the value.
- `vite.config.mjs`: the dev proxy added `Authorization: Bearer` for
  `www.steamgriddb.com` only and proxied images from `cdn2.steamgriddb.com`
  with no key.

SteamGridDB: the API takes a per-user key as `Authorization: Bearer <token>`
(`https://www.steamgriddb.com/static/openapi.yml`, `securitySchemes`). The
OpenAPI `Grid` schema has `id`, `score`, `style`, `url`, `thumb`, `tags` and
`author`, and no width or height.

## Decision

### 1. An artwork operation for plugins

A plugin declares the operation by carrying a callable handler named
`game-assets.candidates`. Evaluation checks that the handler exists and
never calls it, as for `discovery.discover`. The plugin does not export
`providers`: it is not an acquisition provider.

The handler follows ADR 0016's request and response turns. It returns
`{ _tag: "HttpRequests", requests }` or its result. Every ADR 0016 rule
applies: GET only, the plugin's declared `hosts`, the refused headers, at most
4 calls, 8 requests per turn, 512 KiB per response, 4 MiB of input, 1000 ms
and 16 MiB per call.

- **Input:** `{ title, responses }`. `title` is the game's library title, the
  only game fact the built-in fetcher uses. The playable id, release
  identity, system and paths stay in korrid.
- **Result:** the legacy `{ candidates }` answer. Each candidate is legacy
  `GameAssetCandidate` with the deltas below:
  `{ role, width?, height?, source: { id?, url } }`.
- **Order:** the plugin lists candidates in its order of preference. korrid
  uses the first one, as `choose_grid` does today. An empty list means no
  match.
- **Matching stays in the plugin.** The exact verified match, the tag and
  style filters and the score order move into the plugin unchanged. korrid
  does not know SteamGridDB.

For each game, korrid does this:

1. It skips a game with a recorded attempt or a matching `tile` assignment.
   It does not call the plugin for it.
2. It runs the turns and adds the plugin's secret (section 3).
3. It checks the first candidate's `url`: HTTPS, a host in the plugin's
   `hosts`, a public address (`acquisition/policy.rs` `validate` and
   `declared`). It downloads with no redirects, a 10 s timeout and a 5 MiB
   cap, as `asset_download.rs` does today. It never sends the secret on this
   download.
4. It validates the bytes with `game_assets::validate_image`, unchanged.
5. Under the write lock, it checks that the game is still current
   (`current_owned_discovery_game`), then writes the blob and the assignment.
6. It records the attempt.

### 2. When korrid calls it

The trigger does not move. After each successful discovery pass, korrid runs
the artwork pass in the `Enriching` phase, wherever discovery runs (ADR 0017).
It asks every enabled plugin that has the handler and, if the plugin declares
a secret, has that secret set. It asks at most 8 games per plugin per pass.
If several plugins have the handler, korrid asks them in plugin id order and
stops for a game once it has a tile. Nothing on main tests this order,
because there is one real case.

Attempt tracking keeps today's record (`AttemptRecord`, the same key) per
plugin. The outcomes become:

| Event | Outcome | Cleared by |
|---|---|---|
| Image stored | `assigned` | never |
| Empty candidate list | `no-match` | a secret change |
| `ProviderUnreachable`, `ProviderFailed`, download failure | `transient` | a rescan |
| `validate_image` refuses the bytes | `asset-rejected` | a secret change |
| The game changed during the pass | not recorded | |

A 401 response to a request that carried the secret stops the pass and
records nothing, as `RequestFailure::Unauthorized` does today. korrid records
an attempt only if the secret did not change during the pass, as
`record_attempt_if_credential_current` does today.

`ambiguous`, `unverified` and `permanent` go: the plugin decides why it found
nothing. korrid does not retry 429 or 5xx. The plugin sees them as
`ok: false` and fails, and the game becomes `transient`.

The attempts file is `<private>/<plugin id>/attempts.json`, in the plugin's
folder that the owner chose. `attempts.json` is main's `ATTEMPTS_FILE`
(`steamgriddb.rs` :21–22). For this plugin that is
`<private>/@korri:steamgriddb/attempts.json`.

### 3. The API key is a plugin secret

The key uses the plugin secret of
[ADR 0033](0033-korrid-holds-plugin-secrets-and-plugins-never-see-them.md).
That ADR decides who reads it (korrid only), how it is stored, how the
person enters it, how the approval report shows it, and the generic set and
clear RPCs that replace `system.settings.steamgriddbCredential.set` and
`.clear`. This plugin adds only these facts:

- `plugin.nix` declares one secret for `www.steamgriddb.com`. That host and
  the image host are in `hosts`. The image host never gets the key.
- korrid adds the key as `Authorization: Bearer`, the form in SteamGridDB's
  OpenAPI `securitySchemes`.
- A set or a clear of this plugin's secret also clears its attempts except
  `assigned`, as a key change does today (`lib.rs` :3447, :3476).
- The "SteamGridDB API key" row leaves the "Metadata" group
  (`settings-model.ts` :141–160). The plugin's own row takes its place, with
  the same `sensitiveText` interaction, 256 characters and "Clear saved key".

### 4. Title rewriting is dropped

The plugin cannot propose a title. Reasons:

- The rewrite happens only when SteamGridDB's name and the local title are
  equal after `normalized_match_name` (`discovery/title.rs` :48,
  `exact_verified_match`). So it changes only case, punctuation and
  decorations that `fallback_title` already strips.
- Legacy `GameAssetCandidate` has no title field. A title in the result
  would be new schema with no grounding.
- It is a background write into `catalog/library.yaml` that changes the
  release fingerprint. ADR 0016 writes a plugin's title only once, when a
  person presses Get.

`update_owned_discovery_title` goes with the fetcher. Its only callers are
the fetcher and its own test. Titles already rewritten stay: they are library
data.

### 5. The clean cut

One korrid change deletes:

- `services/korrid/src/enrichment/` (`mod.rs`, `steamgriddb.rs`,
  `steamgriddb/asset_download.rs`, `steamgriddb/tests.rs`) and
  `pub mod enrichment` (`lib.rs` :32). The download checks move into the
  artwork pass (section 1, step 3).
- `config/settings.rs`: `STEAMGRIDDB_CREDENTIAL_FILE_NAME`, the set, read and
  clear functions, and `SensitiveSettings.steam_grid_db_credential`.
- `lib.rs`: `SettingsSnapshot.steam_grid_db_credential`, the two request
  types, the four `RpcRequest` and `RpcResponse` variants, their handlers and
  the `steamgriddb-credential` editable id. `authorization.rs` :246–247 and
  :326–327, `portal_access.rs` :84–85.
- `game_assets.rs`: `SteamGridDbProvenance` and
  `AssetCandidate.game_id` and `grid_id`. The provenance becomes legacy
  `GameAssetSource { provider, id?, url? }`, with `provider` set to the
  owning plugin id. It keeps main's `sha256`, `width`, `height` and `format`.
- `discovery::reconcile::update_owned_discovery_title`.
- Portal: `setSteamGridDbCredential` and `clearSteamGridDbCredential`
  (`client.ts`, the real and in-memory clients), the settings row and the
  dispatch branch. `contracts/generated/` is regenerated with
  `nix run .#korrid-check`.

The plugin lands in `korri-os/plugins` as `plugins/steamgriddb/`
(`@korri:steamgriddb`), with `plugin.ts`, `plugin.nix`, tests and a README.
It ships with no key.

**One-off migration.** It runs once on each device when the cut is deployed.
It is an operational step, not korrid code. A device with none of these files
needs nothing.

1. `<private>/steamgriddb.credential`: write its trimmed content as the
   `@korri:steamgriddb` plugin secret, in `<private>/@korri:steamgriddb/`.
   The file is `<private>/@korri:steamgriddb/credential`. The folder is
   mode 0700 and the file 0600, both owned by `korrid` (ADR 0033). Then
   delete the old file.
2. `<private>/steamgriddb-enrichment/`: delete it. It records past lookups,
   not user data, and its outcome names change.
3. `<private>/game-assets/assignments.json`: rewrite each `source` from
   `{ provider: "steamgriddb", game_id, grid_id, sha256, width, height,
   format }` to `{ provider: "@korri:steamgriddb", id: "<grid_id>", sha256,
   width, height, format }`. The legacy importer stored the image id as
   `source.id`. `game_id` goes, as in legacy. No `url` is written, because
   main never stored one.
4. `<private>/game-assets/blobs/` does not change. Asset ids do not change,
   so `coverAssetId` values do not change.

Check after the migration: korrid starts, the local games list reports the
same `coverAssetId` values as before, and the settings snapshot shows the
plugin's secret as `Configured` if the old file existed.

### 6. Covers on Linux are a prerequisite

Showing covers is a separate slice that lands first. It does not depend on who
fetches the art: today's built-in covers are also invisible. This ADR needs it
because nobody can confirm the plugin on a device without it.

The portal cannot put the capability in an image URL (AGENTS.md: never in a
URL). So the portal must fetch the bytes from its own korrid with the bearer
capability and turn them into an object URL for `coverArtUrl`. A `blob:` URL
is same-origin, so boxbuster can draw it to a WebGL canvas.

**Coordinator decision, 2026-10-23:** korrid serves the bytes on a `GET`
route of the local router. The portal sends the capability in the
`Authorization` request header, never in the URL. The CORS layer
(`lib.rs` :4295–4298) then allows `GET` as well as `POST`. It already allows
the `Authorization` header. Reason: the image bytes stay binary, without a
base64 copy a third larger that a handheld must parse as JSON. The route's
path belongs to the section 6 slice. This ADR does not choose it.

### 7. Terms of use

SteamGridDB's Terms of Use (effective 2019-07-31, read 2026-10-23 from the
site's `Terms.jsx` bundle) say a user must not:

- "access the Service for any reason other than your personal,
  non-commercial use solely as permitted by the normal functionality of the
  Service"
- "make any automated use of the Site, the Service or the related systems"
- "distribute any part or parts of the Site or the Service without our
  explicit written permission"

The plugin ships with no key. Each person creates and enters their own. The
plugin README quotes these terms. The plugin fetches one image per game for
that person's device and never redistributes images. A background lookup
after each discovery pass is still automated use as written.

**Owner decision, 2026-10-23:** publish the plugin in `korri-os/plugins`
without asking SteamGridDB first. The cost: after publication, SteamGridDB
can ask for the plugin's withdrawal or revoke the keys that people use with
it.

## Grounding and deltas

| Element | Comes from | Delta and why |
|---|---|---|
| Handler `game-assets.candidates` | legacy RPC `app.game-assets.candidates.list` | New as a plugin operation. `app.` and `.list` dropped because it is not a portal RPC. Name kept by coordinator decision. |
| Plugin id `@korri:steamgriddb` | official `@korri:<name>` ids such as `@korri:pico8`, legacy provider `steamgriddb` | None. Name kept by coordinator decision. |
| Input `{ title, responses }` | `enrich_one` uses `game.title` only, ADR 0016 `responses` | No playable id, release or path: no consumer in the plugin. |
| Result `{ candidates }` | legacy `ListGameAssetCandidatesResponse` | None. No new `_tag`: ADR 0016 treats anything but `HttpRequests` as the result. |
| `candidateId` | legacy `GameAssetCandidate` | Dropped. It named a file in legacy's candidate cache. main names an asset by its bytes. |
| `gameId` | legacy `GameAssetCandidate` | Dropped. korrid asked about one game. |
| `role` | legacy `GameAssetRole` | Only `tile` is admitted. Its only consumer is `matching_tile_asset_ids`, which feeds `coverAssetId`. The other five roles join when a consumer exists. |
| `width`, `height` | legacy `GameAssetCandidate` (required) | Optional, as main's `AssetCandidate.declared_width` and `declared_height`. The SteamGridDB `Grid` schema has no size. korrid still refuses a mismatch (`InconsistentDimensions`). |
| `source.provider` | legacy `GameAssetSource` | Not in the plugin result. korrid writes the owning plugin id, so a plugin cannot claim another source. The legacy closed list becomes the plugin id. |
| `source.id` | legacy `GameAssetSource.id`, the legacy importer's `imageId` | None. |
| `source.url` | legacy `GameAssetSource.url` (optional, no query or fragment) | Required, because korrid downloads it. HTTPS only (main `asset_download.rs`). The host must be in `hosts`. |
| Stored provenance | main `SteamGridDbProvenance`, legacy `GameAssetSource` | `game_id` and `grid_id` become `id`, `provider` becomes the plugin id, and `url` is optional. `sha256`, `width`, `height` and `format` stay. |
| Attempt record and key | main `AttemptRecord`, `attempt_key`, `ATTEMPTS_FILE` | Kept, one set per plugin, in the plugin's folder (owner decision). Three outcomes go (section 2). |
| Secret declaration, value, RPCs, status | ADR 0033 grounding table | See ADR 0033. |
| Bearer header | SteamGridDB OpenAPI, legacy `vite.config.mjs` proxy | None. |

## Amendments to ADR 0016

[ADR 0016](0016-korrid-acquires-games-from-provider-plugins.md) is amended in
these places. `services/korrid/SCRIPTING.md` gets the same edits and a new
artwork section beside "Acquisition providers".

| ADR 0016 place | Today | Amended |
|---|---|---|
| Decision, third bullet (`plugin.nix` gains `hosts`) | "for provider turns and the game download alike" | Also artwork turns and the artwork image download. |
| Grounding row `PluginHttpRequest { url, query?, headers?, timeoutMs? }` | "`authorization` … refused (no auth provider in scope)" | The plugin still cannot set `authorization`. korrid adds it from the plugin secret (ADR 0033). |
| Grounding row `hosts` | "and only with a `providers` export" | Or with a `game-assets.candidates` handler. The check is `plugin-host/src/package.rs` :972–978 ("hosts require a providers contribution"). |
| Grounding row RPC methods | "`credentialRequired` … have no main producer" | The secret declaration is now its producer. It lives in the manifest and settings, not in `AcquisitionProvider`. |
| Turn bounds paragraph | provider operations | Applies unchanged to `game-assets.candidates`. |

## Considered options

- **Keep the fetcher in korrid.** Rejected by the owner decision.
- **The plugin receives the key** (legacy `credentials.get`). Rejected in
  ADR 0033.
- **The plugin downloads and returns image bytes** (legacy
  `artifact.acquire`). Rejected: plugins perform no I/O, and ADR 0016 already
  dropped plugin-side bytes.
- **Keep the title rewrite through a `title` field in the result.**
  Rejected in section 4. It can come back as its own decision if the owner
  wants it.
- **Translate `attempts.json` instead of deleting it.** Rejected: the cost
  of deleting it is one new lookup for each game without a cover, 8 per pass.

## Consequences

- korrid has no SteamGridDB code. A second artwork source is a plugin.
- The image download now has a host list. Today any public HTTPS host passes.
  The SteamGridDB image host is `cdn2.steamgriddb.com`. Legacy
  `product/surfaces/web/shift/dev-game-media.ts` :5–9 says its image URLs
  "were resolved once via the SteamGridDB API", and every one is on
  `cdn2.steamgriddb.com` with no query (:29, :31 and on). That record is
  from legacy, not from a current API response. Check one current response
  before release, or every download fails as an undeclared host.
- A 429 no longer waits and retries in the same pass. The game waits for the
  next rescan.
- The `ambiguous` and `unverified` diagnostics go. A person sees only that
  no cover was found.
- Titles keep their `fallback_title` form. For example, a local
  "Wario Land 4" no longer becomes a SteamGridDB form such as
  "Wario-Land 4™" (both normalize to `wario land 4`, `title.rs` tests).
- On devices, the key becomes settable from the portal for the first time.
  The migration probably finds no key file on most devices. I did not check
  any device.
- Covers stay invisible until the section 6 prerequisite lands.

## Owner questions

No open owner questions. ADR 0033 records the answers that change this ADR:
the key is the person's and follows the play-log choice at an identity
switch, and the migrated file is named `credential`.
