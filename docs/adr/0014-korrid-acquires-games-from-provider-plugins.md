---
status: proposed
---

# korrid acquires games from provider plugins

**Owner decisions, 2026-10-19.**

1. Provider network access: **declared hosts approved at install**. A provider
   plugin names the exact hosts it may reach. The manifest and approval report
   carry the list, the approval digest binds it, and korrid refuses every other
   host. The SSRF and redirect policy in `services/korrid/src/acquisition/policy.rs`
   still applies on every hop.
2. Provider scope: **PICO-8 BBS, Homebrew Hub (Game Boy), RetroBrews (GBA, NES)**.
   itch.io is out (owner: "skip itchio").

**Owner decision A, 2026-10-20.** This ADR builds on
[ADR 0013](0013-declared-games-can-come-from-artifacts.md) (declared games can
come from artifacts). Homebrew Hub and RetroBrews become ADR 0013 declared
games with pinned artifacts; they export no `providers`. PICO-8 BBS keeps
search, and its downloaded cart stays in korrid's cache: korrid never writes
into a person's source. Placement into sources, `sourceId`, `ChooseSource`,
`Staged`, `AcquisitionNoSource`, hash-cache seeding and rescan-to-import are
withdrawn.

**Owner answers, 2026-10-20 (all option a).**

- Q1: the cart's ready folder is
  `cache/<plugin short>/<provider name>/<sha256 hex>/<URL basename>`.
- Q2: the acquire job writes the game (claim title), release (`sha256:`
  identity, system from the claim) and provider link `{ provider, ref: claim
  id }` once, after publishing the ready folder, under discovery's
  publication lock.
- Q3: the resolver finds a release with no saved location and no declared
  artifact by its hash in the owning provider's cache; the runner gets
  `contentPath`. No saved location.
- Q4: ADR 0013's Remove download also removes a cart's ready folder; the game
  stays in history; saves stay.
- Q5: cart jobs join ADR 0013's one-at-a-time queue and deep-sleep block;
  they do not continue after a korrid restart.
- Q6: a removed cart is downloaded again only through Get in the store (same
  game through the provider link).
- Coordinator rulings: a waiting store job reports `Queued` (no payload,
  mirroring ADR 0013's `ArtifactState::Queued`); `acquire` checks the
  provider link and its ready folder before it requires a held claim, so Get
  on an owned cart answers `Imported` with no network.

This decision amends ADR 0012, which said "Source acquisition (search,
claims, resolve-download, browser hand-off, provider health) is OUT". It adds
search, claims and resolve-download for search providers: a provider plugin finds games on one site and says where their
files are; korrid makes every HTTP request, downloads the cart into its cache,
and writes the library record. Plugins stay effect-free (AGENTS.md, plugin
boundaries).
`services/korrid/SCRIPTING.md`, "Acquisition providers", is the exact wire;
`contracts/generated/korrid.ts` is its Typeshare output.

## Provider matrix

| Legacy provider | Disposition | Runner on main | Evidence |
|---|---|---|---|
| `@korri:pico8` (live BBS search) | Build as a search provider (this ADR) | `@korri:fake08` (`pico8`) | legacy `product/plugins/pico8/src/plugin.ts`; BBS markup and cart path changed, parser fix needed |
| `@korri:homebrewhub` (curated entries) | Build as ADR 0013 declared games (decision A) | `@korri:gambatte` (`gb`) | legacy `product/plugins/acquisition-fixtures/index.ts`; `2048gb` live, dead entries dropped |
| `@korri:retrobrews` (curated entries) | Build as ADR 0013 declared games (decision A) | `@korri:mgba` (`gba`), `@korri:fceumm` (`nes`) | same file; `anguna.gba`, `ambushed.nes` live |
| `@korri:itchio`, `@korri:community-catalog`, itch.io wrappers | Excluded: owner decision | — | owner, 2026-10-19 |
| `@korri:portmaster` | Excluded: no PortMaster runner or installer on main | — | owner |
| `@korri:chip8archive`, `tic80gallery`, `wasm4gallery`, `puzzlescript` | Excluded: no runner on main | — | owner |
| `@korri:smwcentral`, `@korri:smbxgame` | Excluded: Cloudflare 403; SMW needs a copyrighted base ROM | — | live probes |
| `@korri:levelsharesquare` | Excluded: upstream dead | — | live probes |
| `@korri:mega-man-maker`, link-only fangame plugins | Excluded: level files or external links, no main runner | — | live probes |
| `gmloader` | Excluded: Android removed 2026-09-16 | — | AGENTS.md |

## Decision

- A plugin that exports `providers` declares acquisition providers and carries
  `claims.search`, `claims.details` and `artifact.resolve-download` handlers.
  Only explicitly exported records are acquisition providers, also when a
  record's id equals the plugin's own id.
- A handler never performs I/O. It returns its legacy result or
  `{ _tag: "HttpRequests", requests }`; korrid fetches them and calls it again
  with every response so far (request/response turns, replay, no opaque state).
- `plugin.nix` gains `hosts`. korrid refuses every request and redirect hop
  outside them, for provider turns and the game download alike.
- Local `/rpc` gains `app.acquisition.providers`, `.search`, `.details`,
  `.acquire`, `.acquire-status` and `.acquire-cancel`, portal capability only,
  on the local brain (it alone runs discovery).
- korrid downloads a resolved cart once, in ADR 0013's queue, publishes it in
  its cache, writes the game, release and provider link once, and reports
  `Imported { gameId }`. The route reaches the cart by hash and passes it as
  `contentPath`; Remove download removes the ready folder.

## Grounding and deltas

| Element | Comes from | Delta and why |
|---|---|---|
| `providers` export, `ProviderRecord { id, title }` | `services/korrid/src/plugin.rs` | An explicit record needs a non-empty title; the three handlers are checked, never called (like `discovery.discover`). |
| Handler names | legacy `product/plugins/AGENTS.md` operation list | `claims.parse-url`, `provider.validate` and `artifact.acquire` are not carried: no consumer in scope, and `artifact.acquire` needs plugin-side bytes. |
| Handler input | legacy `SearchRequest.query`, `DetailsRequest`, `ResolveDownloadRequest`, `context.provider`; main's `ReleaseDiscoveryInput` (host-read text in) | Adds `providerId` and `responses`. `platforms`, `size` and `artifactFormat` are not carried. |
| `PluginHttpRequest { url, query?, headers?, timeoutMs? }` | legacy `request(url, PluginHttpRequestOptions)` (`product/platform/plugin/services.ts`), legacy `withQuery` | The call becomes a returned value. GET only: `method` and `body` are not carried (no in-scope provider posts). No default timeout is invented (legacy had none); connect timeout and stall detection still apply. `cookie`, `set-cookie`, `authorization`, `proxy-authorization` and `host` headers are refused (no auth provider in scope). |
| `PluginHttpResponse { status, ok, url, headers, text }` | legacy `PluginHttpResponse` | `text()` becomes `text`; `setCookies` and `bytes` are not carried. Headers lower-cased, repeats joined with `", "`; UTF-8 with replacement (legacy `TextDecoder`). korrid follows redirects, so no 3xx reaches a plugin. |
| Tag `HttpRequests` | — | New. Results keep their legacy shapes: an array for search, `_tag`-tagged objects otherwise. |
| `ProviderClaim`, `ProviderClaimDetails`, `ArtifactAcquisitionHint`, `DownloadResolution`, `DownloadChoice` | legacy `claim.ts`, `download-resolution.ts`, `artifact.ts` | Dropped: `ref`, `playable`, `facets`. Their only legacy consumer authored library entries from plugin-supplied hints; on main the acquire job writes one fixed record (claim title, the downloaded bytes' hash, the claim's system, a provider link), so no plugin-authored library data is needed and `FinalDownload.requestHeaders` (no in-scope provider forwards Referer or Cookie). `SafeText`, `SafeUrl`, `SafeFileName`, `SEMANTIC_ID` and `SAFE_EXTENSION` are kept in korrid's admission. A claim id is 1 to 2048 characters (legacy `ProviderRef.value` bound). Details must name the asked id. |
| Unacquirable claim | legacy Shift "ungetable" guard | `artifact.system` absent or not an enabled system: shown, not acquired (`AcquisitionSystemUnknown`). |
| `hosts` | owner decision 1; `ports` in `plugin.nix`, `firewall.rs` `Ports` bounds | Builder omits it when empty, like `requires`, so existing approvals do not change. Exact lowercase DNS names, no wildcard, port, scheme, trailing dot or IP literal, at most 128, and only with a `providers` export. Report field `hosts`; `EnabledPackage.hosts`. |
| RPC methods | legacy `product/apps/portal/api/acquisition/*.rpc.ts` | `acquire-cancel` is new (legacy had no cancel). `search` needs exactly one provider id. `providers` answers `AcquisitionProvider { id, title, pluginId }`, replacing legacy `PluginMetadata` (`legalRisk`, `credentialRequired`, `module` have no main producer). |
| `AcquireArtifactRequest { providerId, id, fileName? }` | legacy `AcquireArtifactRequest` | `url`, `size`, `artifactFormat`, `system`, `title`, `artUrl` dropped: korrid takes them from the claim it holds, so the portal cannot substitute a URL. |
| `AcquireJobStatus` | legacy `AcquireJobStatus` | `stagedPath`, `placedPath` dropped: paths never leave korrid; the game id is the handle. `state` becomes a tagged union. |
| `AcquireJobState` | legacy `acquiring/staged/imported/failed`; ADR 0012 progress fields; ADR 0013 `Queued` | `Queued`, `Resolving`, `Downloading`, `Placing`, `Imported { gameId }`, `Failed { reason }`, `Cancelled`. `staged` has no meaning: nothing is left unrecognized. `NonFinalDownload` and `FailedDownload` become `Failed` with codes `DownloadNotFinal` and `DownloadResolutionFailed`. |
| Repeated `acquire` | legacy `activeByClaim`; ADR 0002 provider identity | A live job answers again. A provider link `<provider>/<claim id>` whose game's cart is ready answers `Imported` before any claim is required, also after a restart. |
| Claim and job stores | legacy `claim-store-live.ts`, `acquire-jobs.ts` (in memory) | None persisted. |
| Staging | ADR 0012 cache grammar | `<storage-root>/cache/<plugin short name>/<provider name>/.downloads/`, 0700, random `.acquire-*` names, removed on failure, cancel and startup. |
| 2 GiB cap | legacy `MAX_FALLBACK_DOWNLOAD_BYTES` | ADR 0010 dropped caps only for pinned, hash-verified payloads. |
| Web page and empty refusal | legacy `rejectNonArtifactPayload` | HTML content type or HTML lead tag, and empty bodies. The legacy magic-byte table is not carried. |
| Content rules | `SystemRecord.content` | The file name must end in a system extension (longest suffix); a matching header rule must leave content. |
| Ready folder `cache/<plugin>/<provider>/<sha256 hex>/<URL basename>` | ADR 0012 cache grammar, ready-directory rule and URL-basename rule; owner Q1 | Departs from legacy `artifact-placement.ts` (copy into a storage), as ADR 0013 does. The version segment is the content hash, so identical bytes share one folder. |
| Game, release and provider link | ADR 0013 records (`reconcile_artifact_releases`, `declared_link`); ADR 0002 provider identity; legacy `applyImportMetadata` (claim title, staged sha256); owner Q2 | Written by the acquire job once, under the discovery publication lock, not by a scan. No location. Reconcile keeps them: a release with no location and no origins is never removed. |
| Route `cache_target` | ADR 0013 `contentPath` and its `artifact_target` case; owner Q3 | A third no-location case: the linked acquisition provider's cache, found by the release hash. |
| Remove download | ADR 0013 fifth and sixth rounds; owner Q4 | Also removes the cart folder, unless another game names the same hash; refuses while the game runs or a store job for it works. |
| Queue and sleep | ADR 0013 fifth round; owner Q5 | A store job takes one turn in the same line and is never paused for a launch. It does not continue after a restart. |
| Expected digest | legacy `expectedDigests` on `PluginAcquireOutput` | No in-scope provider supplies a digest and no result type carries one, so nothing is verified against one. SHA-256 is byte identity, not authentication. |
| Failure copy | `launcher/redaction.rs` posture, ADR 0012 closed vocabulary | Fixed plain copy; no URL, path, query, header value, HTTP status or plugin text. Diagnostics go to `log::debug!`, which the build compiles out. |

Turn bounds: at most 4 handler calls per operation, 8 requests per turn,
4 MiB of body text per response, 4 MiB of serialized handler input in total,
and the existing 1000 ms deadline and 16 MiB heap per call. The input bound is
new: the contract's per-response bound needs more than the 512 KiB launch input
bound, and the heap must hold the input. A 3.5 MiB page fits, as tested.

## Considered Options

- **Effectful plugin HTTP service (legacy `services.http`).** It keeps legacy
  plugin code unchanged but gives the sandbox network access, which main
  forbids.
- **Opaque continuation state between turns.** It saves replaying handler
  work, but adds a second state format that korrid must bound and trust.
- **This decision.** Replay with all responses keeps handlers pure and
  deterministic; the cost is at most four short evaluations per operation.

## Consequences and stated limits

- A korrid restart forgets claims and jobs. `acquire-status` answers
  `AcquisitionUnknownJob`; startup removes staging files and `.tmp-*`
  folders; Retry downloads from zero. A store download never resumes: with no
  expected hash and no saved URL, a resumed range cannot be proven to be the
  same bytes.
- Game content is reconstructible, as in ADR 0013: deleting the cache, or the
  provider removing the cart, loses it on that device. Saves stay.
- A store job is not paused for a launch; a launch's own download waits for
  it. Carts are small.
- `NonFinalDownload.choices` cannot reach the person: the job becomes
  `Failed { DownloadNotFinal }`. `fileName` on `acquire` is passed to
  `artifact.resolve-download` as legacy did.
- Ports in provider URLs are not restricted; only the host name is declared.
- If korrid stops after publishing a ready folder and before writing its
  library record, the folder has no release that names it, so Remove download
  cannot reach it. Startup recovery logs one line per such folder and deletes
  nothing. The next Get of the same game records it (the folder is reused);
  otherwise the folder stays until the cache is deleted.
- Declaration evaluation refuses a plugin whose artifact key equals one of
  its provider names, so ADR 0012 artifact caches and ADR 0014 provider
  caches never share `cache/<plugin>/<key>/`.
- Device evidence is still owed; the Mini V2 was unreachable during this work.
