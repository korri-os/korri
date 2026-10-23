---
status: proposed
---

# korrid unpacks archived provider downloads

**Owner decisions: none yet (proposed 2026-10-23).** Every choice in this ADR
is an owner question. The list is under "Owner questions". Where the proposer
prefers one option, the text says "lean" and marks it as an opinion.

The proposal: korrid takes a game from a ROM-site provider to a playable file.
It unpacks an archive download and keeps the one content file inside it. It
can send the game download as a form POST. It names the ready file from the
plugin's `filename` or the archive entry, not from the URL. The archive is
never a release. The release is the content file, identified by its bytes as
[ADR 0005](0005-release-identity.md) says.

## Problem

[ADR 0016](0016-korrid-acquires-games-from-provider-plugins.md) lets korrid
acquire a game from a provider plugin. Today korrid refuses every game from a
ROM-site provider. Three rules cause this:

1. **An archive is not a game file.** A system cannot declare `zip`, `7z`,
   `rar`, `gz` or `tar` (`services/korrid/src/plugin.rs` `validate_content`,
   lines 1548–1567, from ADR 0005 "Archives are not supported in the first
   version" and the discovery v2 brief). The acquire job checks the name
   against the claim system's extensions before it downloads
   (`acquisition/game.rs` lines 598–611). A `.zip` fails as `NotSystemFile`
   with no download (`acquisition/provider_tests.rs` lines 772–777).
2. **The game download is a GET.** `DownloadEngine::send` builds every hop
   with `client.get` (`acquisition/download.rs` line 176). WoWROMs returns the
   game only to a form POST.
3. **The file name is the URL's last segment** (`policy::basename`,
   `game.rs` line 599, ADR 0016 owner answer Q1). Three of the four sites put
   a token or nothing there.

## Evidence

Live evidence, gathered 2026-10-23 by other agents. Sources:
`/tmp/korri-provider-probe/REPORT.md` and its `results/*.json`,
`/tmp/korri-wowroms/README.md`, its `fixtures/` and `plugin/`. The probes read
at most 4 KiB of each game file.

| Site | Formats seen (magic bytes) | Last segment of `FinalDownload.url` | Plugin `filename` | `Content-Disposition` | Method |
|---|---|---|---|---|---|
| coolrom.com (moved from coolrom.com.au) | `.zip` x2 (`504b0304`), `.7z` x1 (`377abcaf271c`) | empty: the URL ends in `/` after a signed token and an expiry time | correct (`Sonic the Hedgehog.zip`) | correct | GET, link expires after about 300 s |
| romhustler.org | `.7z` (`377abcaf271c`), `.rar` (`526172211a07`) | empty: the URL ends in `/` after a token | wrong: the plugin says `.zip` for both | the only correct name | GET |
| retrostic.com | `.zip` | `<name>.zip` | correct | absent | GET |
| wowroms.com | `.zip`, first entry is the content file (`Super Mario World (USA).sfc`) | opaque token (`7bf4e0908e024c5`) | form field `file` (`Super Mario World (USA).zip`) | not recorded | POST `application/x-www-form-urlencoded`, body `emuid`, `id`, `file` |

Other facts from the same evidence:

- No probe checked whether each archive holds only one file.
- WoWROMs: a GET of the file URL returns a page of PHP notices. The POST needs
  no cookie, Referer or User-Agent. The link step before it works as a GET.
  The file host comes from a pool of five: `dfr.wowroms.com`,
  `filesdw.wowroms.com`, `slave7t.wowroms.com`, `trt.wowroms.com`,
  `wowroms-dw.com` (55 samples).
- retrostic: 2 of 5 sampled files are broken. One is a 172-byte nginx
  "400 Bad Request" page sent as `200 application/zip`. Its first bytes are
  `<html>`, so `html_lead` (`download.rs` line 519) refuses it today. This is
  verified by reading the code, not by a test.
- coolrom: search returns 0 results for SNES and GBA. The site no longer
  lists Nintendo games.
- romhustler: 2 of 5 sampled games resolve. The other 3 answer "Sorry, this
  game is restricted." and become `NonFinalDownload`.
- The two PSX samples are disc games: coolrom `.7z`, 460,762,350 bytes, and
  romhustler `.rar`, 543,960,550 bytes. Disc games are out of scope.
- The legacy parking-lot item `01KXC8EAAD647X5PBTYMP06T6E` (2026-07-12) said
  romhustler guest downloads were account-gated, coolrom needed a cookie jar,
  and WoWROMs needed a POST after a JavaScript countdown. The 2026-10-23
  evidence replaces the first two: 2 of 5 romhustler games download as a
  guest, and coolrom sent no `Set-Cookie`. It confirms the POST.

## Scope

- The first consumers are the private `@simonwjackson` provider plugins.
  ADR 0016 owner decision 2 lists Korri's own providers (PICO-8 BBS, Homebrew
  Hub, RetroBrews). **This ADR adds no ROM-site provider to Korri and does not
  change that list.**
- The mechanism is generic. Any provider plugin with declared hosts gets it,
  Korri's own providers included.
- Plugins stay effect-free. korrid makes every request, downloads, unpacks
  and writes the record (AGENTS.md "Federation" and "Plugin boundaries"). The
  unpack runs on the device whose korrid runs the store
  ([ADR 0017](0017-the-device-korrid-runs-discovery-and-the-store.md)).

## Provider matrix

| Provider | Change 1 (unpack) | Change 2 (POST) | Change 3 (file name) | Result with all three |
|---|---|---|---|---|
| coolrom | zip covers 2 of 3 samples, 7z the third | not needed | needed: empty last segment | zip games. 7z games only if 7z is chosen. |
| romhustler | needs 7z and rar | not needed | needed: empty last segment, wrong plugin `filename` | only if 7z and rar are chosen. 3 of 5 games stay restricted. |
| retrostic | zip | not needed | not needed | zip games. Broken site files still fail. |
| WoWROMs | zip | needed | needed: opaque last segment | zip games |

Today's code refuses all four. With change 1 alone, only retrostic works:
`policy::basename` refuses an empty last segment (`policy.rs` lines 131–138),
and an opaque one fails the extension check.

## Proposal

### Change 1: korrid unpacks an archive download (main change)

The acquire job does these steps:

1. It downloads as today: declared hosts, the SSRF and redirect policy, one
   attempt, the 2 GiB cap, and the web page and empty-body refusal.
2. It decides from the first bytes whether the download is an archive, and
   which format. The name alone is not enough: romhustler sends `.7z` and
   `.rar` bytes under a `.zip` name.
3. It reads the entry list and selects the content file (Q1c).
4. It streams that entry into a `.tmp-*` work folder in the provider cache,
   counts the bytes it writes and computes SHA-256 and SHA-1 while it writes.
5. The content file must pass the system content rules, as a plain download
   does today: its name ends in a system extension, and a matching header
   rule leaves content.
6. It publishes the ready folder for the content file, deletes the archive,
   and writes the release with the content file's hash.

A download whose bytes are not an archive continues as today.

Before the download, korrid accepts a name that ends in a system extension or
in the extension of a chosen archive format. It refuses any other name before
the download, as today.

**Identity.** The archive is never a release, and korrid records no archive
hash. The release is the content file, so the same content from a `.zip` on
one site and a `.7z` on another joins one game. A copy that discovery finds in
a source joins by hash, as today. The discovery v2 rule does not change: a
system still cannot declare an archive extension. Archive handling belongs to
the acquire job, not to system content rules.

**Q1a. Where the unpacker runs.**

| Option | Cost | Grounding |
|---|---|---|
| A. In korrid's process | korrid parses untrusted bytes in the process that holds the device key and writes the library. Each format adds code to korrid. | legacy `product/platform/archive/zip.ts`: a zip reader with intake limits, used by the `gmloader` and `portmaster` installers, never by acquisition |
| B. ADR 0012 `unpack` program, shipped by the provider plugin | Provider plugins are source-only today (`nix/product/PUBLISHED-PLUGINS.md`: "`plugin.ts` and the generated manifest, no references"). Each one then needs a prebuilt native closure per architecture. ADR 0012 `unpack` is a field of `ArtifactRecord` (`plugin.rs` lines 210–221) and the unit name comes from the artifact id (`acquisition/unit.rs` lines 10–14). A provider has no artifact record, so its declaration is new schema and is unresolved. Each ROM-site plugin carries its own unpacker or depends on another plugin. | [ADR 0012](0012-korrid-acquires-declared-artifacts.md), SCRIPTING.md "Declared artifacts" |
| C. A korrid-owned program in the ADR 0012 sandbox unit | korrid's closure grows. The unit needs a name rule for provider jobs, which is unresolved. It departs from ADR 0012's split, where the plugin owns the format. | `unit.rs` `unit_arguments`, lines 87–135: no network, one writable folder, no capabilities |

ADR 0012 rejected "unpack natively in Rust" for runtimes, because those
formats are many and plugin-specific (squashfs, zstd, xz, a VDF edit). Game
archives from these sites are three formats with one job: extract one file.
That reason does not carry over. The out-of-process sandbox reason does.

Lean (opinion): A for zip only, because zip needs no new crate (Q1b). B or C
for 7z and rar, because the readers found for them are C or C++ code (Q1b).
Cost: two code paths if 7z or rar comes later.

**Q1b. Which formats.**

| Formats | Unlocks | Cost |
|---|---|---|
| zip (stored and deflate) | retrostic, WoWROMs, coolrom zip games | `flate2` and `miniz_oxide` are already in `Cargo.lock` through `png` and `oxc-browserslist`. `crc32fast` is a direct dependency. A reader ported from legacy `zip.ts` needs no new crate. The `zip` crate is the other choice, as a new direct dependency. |
| zip and 7z | adds coolrom 7z games and romhustler 7z games | A 7z reader (LZMA, LZMA2, filters). No crate was chosen or built for this ADR. LZMA dictionary size is set by the archiver and can need a large amount of memory on a handheld. |
| zip, 7z and rar | adds romhustler rar games | This ADR found no pure Rust RAR reader (not searched in depth). The `unrar` crate wraps the C++ UnRAR source under the UnRAR licence. libarchive, through `compress-tools`, reads all three but is a C library. Both parse untrusted input in C or C++, which favours Q1a B or C. |

**Q1c. Which entry is the content file.** Directories, symbolic links, hard
links, device entries and encrypted entries are never the content file.

| Option | Cost |
|---|---|
| E1. The archive holds exactly one file. Otherwise korrid refuses it. | Simplest. An archive with a readme fails. |
| E2. Exactly one entry ends in a system extension. korrid drops the other entries. | Accepts readme and `.nfo` files. Drops any other file, which matters only for multi-file games, which are out of scope. Zero or two candidates fail. |
| E3. The first entry. | Matches the WoWROMs samples. Entry order is the archiver's choice. A readme first fails as `NotSystemFile`. |

An entry name with folders (`Game/Game.sfc`) is a separate question. Legacy
`safeZipPath` keeps a normalized relative path and refuses `..`, absolute
paths, drive letters and NUL. korrid needs one file name, not a path. It can
refuse an entry with a folder, or take the entry's last segment. Unresolved.

**Q1d. Limits.** These are mechanisms. The values marked unresolved need the
owner.

- **Download size:** the existing 2 GiB cap (`MAX_GAME_BYTES`,
  `download.rs` line 43, legacy `MAX_FALLBACK_DOWNLOAD_BYTES`) still applies
  to the archive.
- **Expanded size:** korrid counts the bytes it writes and stops at the cap.
  It never trusts a size that the archive declares. The value is unresolved.
  Candidates from existing code: the same 2 GiB, so a ready folder never
  holds more than a plain download can. Or legacy gmloader
  `DEFAULT_MAX_EXPANDED_BYTES = 4_000_000_000`.
- **Entry count:** legacy `ZipReadLimits.maxEntries` (gmloader default
  20,000). The value is unresolved.
- **Path traversal:** korrid never uses an entry path as a filesystem path.
  It writes one file into a `.tmp-*` folder that it made. The name passes
  legacy `SafeFileName` (`claims::safe_file_name`) and does not start with
  `.`, as `publish_acquired_game` requires (`placement.rs` line 280). It
  opens the file with `O_NOFOLLOW`, as the staging file is opened today.
- **Magic bytes:** korrid brings back the zip, 7z and rar rows of legacy
  `ARTIFACT_MAGIC` (`artifact-acquisition.ts`). Zip is `50 4b 03 04`,
  `50 4b 05 06` or `50 4b 07 08`. 7z is `37 7a bc af 27 1c`. Rar is
  `52 61 72 21 1a 07`. ADR 0016 dropped this table. Bytes in a format that
  was not chosen fail with the existing `Unpack` copy ("The downloaded files
  could not be unpacked."). A download that korrid accepted by an archive
  name, with bytes in no archive format, fails as `NotSystemFile`.
- **Disk:** the archive and the content file exist together until
  publication, so peak use is about twice the content size.

**Q1e. Job state while korrid unpacks.** Keep `Placing`: no wire change,
and `Placing` cannot be cancelled today. Or add `Unpacking`, the ADR 0012
artifact state name: a change to `AcquireJobState` and its Typeshare output.
Lean (opinion): keep `Placing` for zip. A 2 GiB 7z on a handheld can take
minutes, which argues for `Unpacking` if 7z is chosen.

### Change 2: a form POST on `FinalDownload`

`FinalDownload` can carry the legacy `PluginHttpRequestOptions` fields
`method` and `body`. korrid sends the game download with them. Provider turns
stay GET-only, because the WoWROMs link step works as a GET.

Grounding: legacy `product/platform/plugin/services.ts` declares
`method?: string` ("HTTP method. Defaults to GET.") and
`body?: string | Uint8Array | URLSearchParams` ("Request body for
POST/PUT/PATCH."). Legacy `plugin-runtime.ts` sent them through
`fetchWithValidatedRedirects`. The legacy parking-lot item
`01KXC8EAAD647X5PBTYMP06T6E` names this gap: "PluginHttpRequestOptions
supports method + body (POST) so wowroms can submit its download form".
Legacy carried the two fields on request options, not on `FinalDownload`.
Moving them to `FinalDownload` is the delta.

This amends two ADR 0016 "Grounding and deltas" rows:

- `PluginHttpRequest { url, query?, headers?, timeoutMs? }`: "GET only:
  `method` and `body` are not carried (no in-scope provider posts)". This
  stays true for provider turns. It no longer holds for the game download.
- `ProviderClaim`, `ProviderClaimDetails`, `ArtifactAcquisitionHint`,
  `DownloadResolution`, `DownloadChoice`: `FinalDownload` gains `method` and
  `body`. `FinalDownload.requestHeaders` stays dropped unless Q2a picks (a).

**Q2a. Wire shape: unresolved.** JSON carries neither `Uint8Array` nor
`URLSearchParams`. WoWROMs needs `Content-Type:
application/x-www-form-urlencoded`, and `FinalDownload` carries no headers.
Two candidates come from legacy:

- (a) `body` as a string, plus legacy `FinalDownload.requestHeaders` brought
  back for `content-type` only.
- (b) `body` in the legacy `URLSearchParams` form. JSON can carry that only
  as an object of string values. korrid encodes it and sets the form content
  type, as `fetch` does for a `URLSearchParams` body.

The WoWROMs port in `/tmp/korri-wowroms/plugin/plugin.ts` uses a field named
`form`. That name has no legacy grounding, and this ADR does not propose it.
Legacy accepted any method string. Whether korrid accepts only POST besides
the GET default is part of Q2a.

**Security effect.**

- **At most one POST per acquire.** `fetch_game` makes one attempt and never
  retries or resumes a store download (ADR 0016), so korrid never repeats a
  POST.
- **Only to declared hosts.** `policy::declared` checks the URL and every
  redirect hop, as today. The SSRF policy applies to every hop.
- **Still no cookie or authorization.** `cookie`, `set-cookie`,
  `authorization`, `proxy-authorization` and `host` stay refused. No header
  reaches the game download except a content type, as Q2a decides.
- **Body bound.** The body is plugin text. The existing bound for plugin text
  is legacy `SafeText`, 8192 characters (`claims.rs` line 13). The value is
  unresolved. Failure copy never contains the body, and diagnostics stay at
  `log::debug!`, which the build compiles out.
- **Approval.** `plugin.nix` declares `hosts`, not methods. An approved plugin
  can then POST to its hosts, and the approval report does not show it. Q2c
  asks whether POST needs its own declaration.
- **Side effects.** A POST can change state on the server. The scope is the
  hosts that the person approved at install.

**Q2b. A redirect after a POST.** No probe recorded a 3xx after the WoWROMs
POST. The behaviour is unknown. `send` applies one `configure` closure to
every hop (`download.rs` lines 163–195).

| Option | Cost | Grounding |
|---|---|---|
| (i) Send the same method and body on every hop | The body goes to every hop, but each hop is a declared host. Smallest code change. | legacy `safe-fetch.ts`: `fetchImpl(current, { ...init, redirect: "manual" })` keeps `method` and `body` |
| (ii) 301, 302 and 303 become a GET with no body. 307 and 308 send the POST again. | Matches browsers. More code and tests. | HTTP semantics (RFC 9110 section 15.4) |
| (iii) Refuse any 3xx after a POST | A site that redirects its POST to a file host fails. | none: built only for the observed case |

### Change 3: the name of the ready file

This amends ADR 0016 owner answer Q1, which names the ready file
`<URL basename>`. **This is an owner decision.** It also reverses the comment
on `policy::basename` (`policy.rs` line 106): "Names come from the declared
URL, not a server-controlled redirect or header."

With change 1, two names exist:

- The **content file name** names the ready file. For an archive it is the
  selected entry's name (Q1c). For a plain download it is the outer name.
- The **outer name** passes the check before the download. For an archive it
  names nothing in the cache.

The ready folder becomes
`cache/<plugin short>/<provider name>/<sha256 hex of the content file>/<content file name>`.
Only the last segment's source changes. The rest of ADR 0016 Q1 stays.

**Q3a. Where the outer name comes from.**

| Option | Unlocks | Cost | Grounding |
|---|---|---|---|
| N1. `FinalDownload.filename`, then the URL's last segment | coolrom, romhustler (as an archive name), WoWROMs | `filename` is plugin text. It is already on the wire and admitted as `SafeFileName` (`claims.rs` lines 430–434), but no code reads it today. | legacy `resolution.filename ?? request.fileName ?? downloadUrlBasename(resolution.url)` (`artifact-acquisition.ts`), without the middle term |
| N2. N1 with the acquire request's `fileName` between them, as legacy did | same | The portal can then choose the name. ADR 0016 keeps the URL and claim fields out of the portal's reach. | same legacy line, complete |
| N3. N1, and a `Content-Disposition` name from the response can replace it | romhustler's real name for a plain download | A server-controlled name. Reverses the `policy.rs` rule. Legacy never read this header (`git grep` on `legacy` finds it only in a `tools/lab-ai` lock file). Needs RFC 6266 parsing, `filename*` included. It arrives after the download, so it cannot help the check before the download. | romhustler evidence only |

Lean (opinion): N1. With magic bytes and the entry name, romhustler needs no
`Content-Disposition`. korrid learns 7z or rar from the bytes and the content
file name from the entry. `Content-Disposition` helps only a plain,
non-archive download with an opaque URL and a wrong plugin `filename`. The
evidence has no such case. Cost: if such a case appears, it fails as
`NotSystemFile` until the plugin sends a correct `filename`.

`safe_file_name` allows control characters. `policy::basename` refuses them.
If `filename` names a file, korrid must apply the stricter of the two rules.

## Grounding and deltas

| Element | Comes from | Delta and why |
|---|---|---|
| Archive is a transport wrapper, never a release | ADR 0005 identity rule (content bytes). ADR 0013 second round: release hashes are "hashes of its main content file" (`data.pck`, "not the ZIP"). | Amends ADR 0005 "Archives are not supported in the first version" for provider downloads only. Discovery still refuses archives. |
| Zip reader, its limits `maxEntries`, `maxCompressedBytes`, `maxUncompressedBytes`, and `safeZipPath` | legacy `product/platform/archive/zip.ts`. Legacy gmloader defaults: 20,000 entries, 2,000,000,000 and 4,000,000,000 bytes. | Legacy read whole archives into memory. korrid streams one entry. Values unresolved (Q1d). |
| Magic-byte table | legacy `ARTIFACT_MAGIC` in `artifact-acquisition.ts` | ADR 0016 dropped it. The zip, 7z and rar rows come back to decide the format. The `gz` and `png` rows stay out. |
| 2 GiB download cap | legacy `MAX_FALLBACK_DOWNLOAD_BYTES`, ADR 0016 | Unchanged. An expanded-size cap is new (Q1d). |
| Content rules | `SystemRecord.content`, ADR 0016 "Content rules" row | Applied to the content file after unpacking, not to the archive. |
| Unpack program | ADR 0012 `unpack`, `unit.rs` | Option only (Q1a B, C). A provider-level declaration is unresolved schema. |
| `FinalDownload.method`, `FinalDownload.body` | legacy `PluginHttpRequestOptions.method`, `.body` | Moved from request options to `FinalDownload`. JSON shape unresolved (Q2a). |
| `FinalDownload.requestHeaders` | legacy `FinalDownload.requestHeaders` | Stays dropped, unless Q2a (a) brings it back for `content-type` only. |
| Redirect after POST | legacy `fetchWithValidatedRedirects` | Q2b. |
| Outer name order | legacy `resolution.filename ?? request.fileName ?? downloadUrlBasename(...)` | N1 drops `request.fileName` (Q3a). |
| Ready file name | ADR 0016 owner answer Q1 (`<URL basename>`) | Becomes the content file name. Owner decision. |
| Failure copy | `acquisition/types.rs` `Failure`, code `AcquisitionFailed` | No new wire code. Existing `Unpack`, `NotSystemFile` and `TooLarge` copy covers the new failures. `TooLarge` says "larger than 2 GiB", so a different expanded cap needs new copy. |

## Owner questions

1. **Q1a.** Where does the unpacker run: in korrid (A), a plugin program (B),
   or a korrid-owned program in the sandbox unit (C)?
2. **Q1b.** Which formats: zip only, zip and 7z, or zip, 7z and rar?
3. **Q1c.** Which entry is the content file: E1, E2 or E3? Is an entry with
   folders refused, or is its last segment taken?
4. **Q1d.** The expanded-size cap and the entry-count bound.
5. **Q1e.** Report unpacking as `Placing`, or add `Unpacking`?
6. **Q2a.** The JSON shape of `body`, how korrid sets the content type, and
   which methods korrid accepts.
7. **Q2b.** Redirect after a POST: (i), (ii) or (iii)?
8. **Q2c.** Does a plugin that sends a POST need to say so in `plugin.nix`
   and the approval report, or are approved `hosts` enough?
9. **Q2d.** The bound on `body`.
10. **Q3a.** The outer name: N1, N2 or N3? This amends ADR 0016 Q1.
11. **Q4 (found while writing this ADR).** ADR 0005 removes a header from a
    file release's identity. Discovery does this
    (`discovery/identify/hash.rs` lines 62–74 and 103–126). The ADR 0016
    acquire job does not. It records the SHA-256 of all downloaded bytes
    (`download.rs` `fetch_game`, `game.rs` lines 707–721) and uses that hash
    as the ready-folder name (`placement.rs` lines 274–307), which the route
    looks up by release hash (`game.rs` lines 120–131). For a file with a
    header rule (the discovery v2 brief names NES, Lynx, 7800, SNES and FDS),
    the store's release and a discovered copy of the same file get different
    identities. Must the content file's identity follow ADR 0005 and remove
    the header? If yes, the ready-folder key changes for those systems, and
    existing ready folders keyed by the whole-file hash need a one-off
    operational step or a new download.

## Considered options

- **Keep the archive whole and let the runner open it (legacy).** Legacy
  RetroArch discovery claimed `.zip` and `.7z` for cart systems
  (`product/plugins/retroarch/src/discovery.ts` lines 92–144), and RetroArch
  opened them. With this option, the release identity is the archive's
  bytes. The same game from two sites then makes two releases, and neither
  joins a discovered copy. ADR 0005 and the discovery v2 brief refuse this.
- **The plugin fetches the bytes (`artifact.acquire`).** ADR 0016 dropped this
  operation, because it needs I/O in the sandbox.
- **Change 1 without change 3.** Only retrostic works (provider matrix).
- **Do nothing.** ROM-site providers stay refused.
- **This proposal.** korrid owns the download, the unpack and the record. The
  plugin only says where the file is and how to ask for it.

## Consequences and stated limits

- **Out of scope:** multi-disc sets, cue/bin pairs, `.m3u`, CHD, any game
  that needs more than one file, nested archives, split and multi-volume
  archives (`.7z.001`, `.part1.rar`), encrypted archives, `gz` and `tar`,
  arcade sets where the zip is the content, and `kind: "patch"` claims.
  The PSX samples stay unavailable.
- **Discovery is unchanged.** An archive in a source is still not a game.
- **Signed links.** A store job resolves only when its queue turn starts
  (`game.rs` lines 556–577), so coolrom's link is fresh when the download
  starts. Whether coolrom stops a transfer that runs past about 300 s is
  unknown.
- **Host pools.** WoWROMs picks its file host from a pool. A host outside the
  plugin's `hosts` fails as `UndeclaredHost` until the plugin is updated and
  approved again.
- **Content.** These sites distribute games that the person does not
  necessarily own. Korri adds no such provider. The private plugins and their
  use are the owner's matter. Legacy `PluginMetadata.legalRisk` is not
  carried (ADR 0016).
- **Not checked on a device.** No change in this ADR has run on hardware.

## What an implementation changes

No code changes with this ADR. An implementation touches these places:

| File | Lines | Change |
|---|---|---|
| `services/korrid/src/acquisition/game.rs` | 598–616 | Outer name (Q3a) and the check before the download (system or archive extension) |
| same | 636–650 | Pass `method` and `body` to `fetch_game` |
| same | 660–728 | Unpack, then content rules, publication and record on the content file |
| `services/korrid/src/acquisition/download.rs` | 163–195 (`send`, `client.get` at 176) | Method and body, redirect rule (Q2b) |
| same | 259–366 (`fetch_game`) | Keep the lead bytes for magic-byte detection |
| same | 43 | Expanded-size cap beside `MAX_GAME_BYTES` (Q1d) |
| `services/korrid/src/acquisition/claims.rs` | 1–5, 187–207, 424–436 | `FinalDownload` fields and admission (Q2a, Q2d). Stricter `filename` check. |
| `services/korrid/src/acquisition/policy.rs` | 106–140 | The name rule and its comment (Q3a) |
| `services/korrid/src/acquisition/placement.rs` | 246–266, 268–307 | Remove leftover unpack work at startup. Publish the content file, not the staged download. |
| `services/korrid/src/acquisition/types.rs` | 265–303 | Failure copy. `AcquireJobState`, if Q1e adds `Unpacking`. |
| `services/korrid/src/acquisition/unit.rs` | 10–14, 87–184 | Only for Q1a B or C: unit name and arguments for a provider job |
| `services/korrid/src/plugin.rs` | 210–221 | Only for Q1a B: a provider-level unpack declaration |
| `services/korrid/src/discovery/reconcile.rs` | about 431 | Only if Q4 changes the identity key |
| new module under `services/korrid/src/acquisition/` | new | The archive reader (Q1a A) |
| `services/korrid/Cargo.toml` | `[dependencies]` | Only if Q1b needs a new crate |
| `services/korrid/src/acquisition/provider_tests.rs` | 134, 176, 772–777 | The `zip` claim fixture serves ROM bytes as `game.zip` and expects a refusal before the download. It becomes archive tests: zip, magic mismatch, one entry, limits, traversal |
| `services/korrid/src/acquisition/game_download_tests.rs`, `turns_tests.rs` (215) | new tests | POST, redirect after POST, `filename` naming |
| `services/korrid/SCRIPTING.md` | 515–516, 541–544, 576–582, 688–701 | "Request and response turns", "Results", "A provider download stays in korrid's cache" |
| `contracts/generated/korrid.ts` | whole file | Regenerated by `nix run .#korrid-check`, never by hand |
| `docs/adr/0005-release-identity.md`, `docs/adr/0016-korrid-acquires-games-from-provider-plugins.md` | whole file | Amendment notes, on acceptance |
| `CONTEXT.md` | whole file | A glossary entry for the archive as a transport wrapper, if the owner wants the term |
