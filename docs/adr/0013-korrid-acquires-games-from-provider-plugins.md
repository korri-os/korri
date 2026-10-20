# korrid acquires games from provider plugins

**Owner decisions, 2026-10-19.**

1. Provider network access: **declared hosts approved at install**. A provider
   plugin names the exact hosts it may reach. The manifest and approval report
   carry the list, the approval digest binds it, and korrid refuses every other
   host. The SSRF and redirect policy in `services/korrid/src/acquisition/policy.rs`
   still applies on every hop.
2. Provider scope: **PICO-8 BBS, Homebrew Hub (Game Boy), RetroBrews (GBA, NES)**.
   itch.io is out (owner: "skip itchio").

ADR 0012 left source acquisition out. This decision adds it: a provider plugin
finds games on one site and says where their files are; korrid makes every
HTTP request, downloads the game, places it in a paired source, and lets
discovery import it. Plugins stay effect-free (AGENTS.md, plugin boundaries).
`services/korrid/SCRIPTING.md`, "Acquisition providers", is the exact wire;
`contracts/generated/korrid.ts` is its Typeshare output.

## Provider matrix

| Legacy provider | Disposition | Runner on main | Evidence |
|---|---|---|---|
| `@korri:pico8` (live BBS search) | Build | `@korri:fake08` (`pico8`) | legacy `product/plugins/pico8/src/plugin.ts`; BBS markup and cart path changed, parser fix needed |
| `@korri:homebrewhub` (curated entries) | Build | `@korri:gambatte` (`gb`) | legacy `product/plugins/acquisition-fixtures/index.ts`; `2048gb` live, dead entries dropped |
| `@korri:retrobrews` (curated entries) | Build | `@korri:mgba` (`gba`), `@korri:fceumm` (`nes`) | same file; `anguna.gba`, `ambushed.nes` live |
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
- korrid downloads a resolved game once, places it in a paired source without
  clobbering, seeds discovery's hash cache, rescans, and reports
  `Imported { gameId }` only if the library then holds the placed release.

## Grounding and deltas

| Element | Comes from | Delta and why |
|---|---|---|
| `providers` export, `ProviderRecord { id, title }` | `services/korrid/src/plugin.rs` | An explicit record needs a non-empty title; the three handlers are checked, never called (like `discovery.discover`). |
| Handler names | legacy `product/plugins/AGENTS.md` operation list | `claims.parse-url`, `provider.validate` and `artifact.acquire` are not carried: no consumer in scope, and `artifact.acquire` needs plugin-side bytes. |
| Handler input | legacy `SearchRequest.query`, `DetailsRequest`, `ResolveDownloadRequest`, `context.provider`; main's `ReleaseDiscoveryInput` (host-read text in) | Adds `providerId` and `responses`. `platforms`, `size` and `artifactFormat` are not carried. |
| `PluginHttpRequest { url, query?, headers?, timeoutMs? }` | legacy `request(url, PluginHttpRequestOptions)` (`product/platform/plugin/services.ts`), legacy `withQuery` | The call becomes a returned value. GET only: `method` and `body` are not carried (no in-scope provider posts). No default timeout is invented (legacy had none); connect timeout and stall detection still apply. `cookie`, `set-cookie`, `authorization`, `proxy-authorization` and `host` headers are refused (no auth provider in scope). |
| `PluginHttpResponse { status, ok, url, headers, text }` | legacy `PluginHttpResponse` | `text()` becomes `text`; `setCookies` and `bytes` are not carried. Headers lower-cased, repeats joined with `", "`; UTF-8 with replacement (legacy `TextDecoder`). korrid follows redirects, so no 3xx reaches a plugin. |
| Tag `HttpRequests` | — | New. Results keep their legacy shapes: an array for search, `_tag`-tagged objects otherwise. |
| `ProviderClaim`, `ProviderClaimDetails`, `ArtifactAcquisitionHint`, `DownloadResolution`, `DownloadChoice` | legacy `claim.ts`, `download-resolution.ts`, `artifact.ts` | Dropped: `ref`, `playable`, `facets` (their only legacy consumer authored library entries; discovery owns library records on main) and `FinalDownload.requestHeaders` (no in-scope provider forwards Referer or Cookie). `SafeText`, `SafeUrl`, `SafeFileName`, `SEMANTIC_ID` and `SAFE_EXTENSION` are kept in korrid's admission. A claim id is 1 to 2048 characters (legacy `ProviderRef.value` bound). Details must name the asked id. |
| Unacquirable claim | legacy Shift "ungetable" guard | `artifact.system` absent or not an enabled system: shown, not acquired (`AcquisitionSystemUnknown`). |
| `hosts` | owner decision 1; `ports` in `plugin.nix`, `firewall.rs` `Ports` bounds | Builder omits it when empty, like `requires`, so existing approvals do not change. Exact lowercase DNS names, no wildcard, port, scheme, trailing dot or IP literal, at most 128, and only with a `providers` export. Report field `hosts`; `EnabledPackage.hosts`. |
| RPC methods | legacy `product/apps/portal/api/acquisition/*.rpc.ts` | `acquire-cancel` is new (legacy had no cancel). `search` needs exactly one provider id. `providers` answers `AcquisitionProvider { id, title, pluginId }`, replacing legacy `PluginMetadata` (`legalRisk`, `credentialRequired`, `module` have no main producer). |
| `AcquireArtifactRequest { providerId, id, fileName?, sourceId? }` | legacy `AcquireArtifactRequest` | `url`, `size`, `artifactFormat`, `system`, `title`, `artUrl` dropped: korrid takes them from the claim it holds, so the portal cannot substitute a URL. `sourceId` is new. |
| `AcquireJobStatus` | legacy `AcquireJobStatus` | `stagedPath`, `placedPath` dropped: paths never leave korrid; the game id is the handle. `state` becomes a tagged union. |
| `AcquireJobState` | legacy `acquiring/staged/imported/failed`; ADR 0012 progress fields | `ChooseSource { sources: { id, label }[] }` (coordinator decision; `DiscoverySource` field names), `Resolving`, `Downloading`, `Placing`, `Imported { gameId }`, `Staged { message }`, `Failed { reason }`, `Cancelled`. `NonFinalDownload` and `FailedDownload` become `Failed` with codes `DownloadNotFinal` and `DownloadResolutionFailed`. |
| Repeated `acquire` | legacy `activeByClaim` | Also returns an `Imported` job, so a placed game is not downloaded twice in one process. |
| Claim and job stores | legacy `claim-store-live.ts`, `acquire-jobs.ts` (in memory) | None persisted. |
| Staging | ADR 0012 cache grammar | `<storage-root>/cache/<plugin short name>/<provider name>/.downloads/`, 0700, random `.acquire-*` names, removed on failure, cancel and startup. |
| 2 GiB cap | legacy `MAX_FALLBACK_DOWNLOAD_BYTES` | ADR 0010 dropped caps only for pinned, hash-verified payloads. |
| Web page and empty refusal | legacy `rejectNonArtifactPayload` | HTML content type or HTML lead tag, and empty bodies. The legacy magic-byte table is not carried. |
| Content rules | `SystemRecord.content` | The file name must end in a system extension (longest suffix); a matching header rule must leave content. |
| Placement | legacy `artifact-placement.ts` | Target: eligible paired file sources (system or system group) instead of a preferred storage id; the first layout's literal directory instead of `<system>/`. Identical SHA-256 reused; otherwise `name (n).ext`, 20 attempts. `ext` is the matched system extension, not the last dot, so `cart (1).p8.png` stays a PICO-8 cart. Temp, fsync, `link(2)` (never replaces a name); containment against the canonical root. Content already located in that source is reused instead of copied under another name. |
| Hash-cache seed | legacy identityPolicy "skip" reused the staging digest; `discovery/identify/hash.rs` `HashCache` | Seeded only when no header rule strips bytes; then the rescan reads no file bytes for it. |
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
  `AcquisitionUnknownJob`; startup removes staging files; Retry downloads from
  zero. A game download never resumes: with no expected hash, a resumed range
  cannot be proven to be the same bytes.
- After a restart, "already in library" is decided by bytes: a repeated Get
  downloads again, then reuses the identical file.
- `NonFinalDownload.choices` cannot reach the person: the job becomes
  `Failed { DownloadNotFinal }`. `fileName` on `acquire` is passed to
  `artifact.resolve-download` as legacy did.
- Import waits at most 120 s for the library rescan; a slower scan reports
  `Staged` although the game may appear later.
- A system whose header rule strips bytes costs one file hash on the next scan.
- Ports in provider URLs are not restricted; only the host name is declared.
- A plugin whose artifact key equals a provider's name shares that
  `.downloads` directory; publishing the artifact removes it, which would fail
  an in-flight game download.
- Device evidence is still owed; the Mini V2 was unreachable during this work.
