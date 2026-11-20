# How a plugin provides content from a remote server without effects

Research for issue #560, under map #559 (media content beside games).
Date: 2026-11-20. Korri source read at `main` 28b33b44b.

## Answer

Main already has the shape: **the plugin returns the HTTP requests it needs,
korrid performs them, and korrid calls the plugin again with the responses**
(the "request and response turns" of ADR 0016, also used by artwork in
ADR 0032). A Jellyfin, pyxis or ossicle plugin should use the same shape. It
is the only candidate that keeps plugins effect-free and keeps server
knowledge out of korrid. The owner already refused the two alternatives:
an effectful plugin HTTP service (ADR 0016) and a fetcher built into korrid
(ADR 0032).

The turns do not work for these servers as they are. Six gaps block them.
Each gap needs its own decision:

1. **Server address.** `hosts` is a fixed list of DNS names in `plugin.nix`,
   approved at install. A media server address is the person's own, and is
   known only at setup.
2. **Private addresses.** korrid refuses every private, CGNAT and loopback
   address. A home Jellyfin server or a server on the tailnet
   (`100.64.0.0/10`) is unreachable.
3. **Authentication form.** korrid sends the plugin secret only as
   `Authorization: Bearer`. Jellyfin accepts `Authorization: MediaBrowser
   Token="…"` and does not accept `Bearer`.
4. **JSON bodies.** A turn can send only GET or a form POST. Pyxis and
   ossicle are `POST /rpc` with JSON bodies.
5. **Catalog size.** One operation has at most 4 calls and 4 MiB of input,
   and every call must parse within 1000 ms. A library of thousands of
   items does not fit in one operation. Paging must be a loop that korrid
   drives, one page for each operation, with a cursor that korrid holds.
6. **Kept state.** Claims live in memory only. A catalog needs korrid to keep
   the items it received, and to refresh them.

The legacy synthesis (`legacy:docs/research/plugin-architecture/synthesis-2026-05-31.md`)
gave plugins an `HttpClient` capability. Main reversed that, so that part of
the synthesis does not apply. Its `ContentSource` idea (one source returns
several item kinds) still applies to the content model, not to the fetch.

## Constraints

| Constraint | Origin |
|---|---|
| Plugin code performs no effects; korrid performs them | stated: `AGENTS.md` (Federation, Standing decisions), `SCRIPTING.md` "The sandbox has no I/O" |
| Plugins are TS/JS source in QuickJS, with fixed limits | stated: `SCRIPTING.md` "Interpreter limits" |
| The plugin never sees a secret | stated: ADR 0034 |
| Jellyfin, pyxis and ossicle plugins are official `@korri` client plugins | stated: map #559 |
| Catalogs of thousands of items, with paging | stated: issue #560 |
| No schema invented ahead of a real case | stated: `AGENTS.md` schema guard. This document proposes shapes, not field names |
| Slow handhelds (RG353M class) must also work | inferred from the 2026-10-23 body-bound decision |

## What exists on main

All of these are **verified** against the files at 28b33b44b.

### The turn protocol

`services/korrid/SCRIPTING.md`, "Request and response turns" (line 699 on):

- A handler returns either its result or `{ _tag: "HttpRequests", requests }`.
  korrid performs the requests and calls the handler again with every
  response so far (`responses`). There is no continuation state: the
  handler replays from the start each call.
- Bounds for each operation (line 777; `acquisition/turns.rs:167-170`): at
  most 4 handler calls, 8 requests per turn, 512 KiB body text per response
  (`acquisition/download.rs:57`), 4 MiB serialized input in total. Each call
  also has the 1000 ms deadline and the 16 MiB heap.
- Measured cost (line 785): QuickJS parsing on the aarch64 builder fuji is
  about 340 ms per MiB. The 1000 ms deadline is near 2.9 MiB. RG353M is
  estimated, not measured, at 4 to 6 times slower: near 0.5 to 0.7 MiB.
- Requests: GET, or POST with form fields only (`application/x-www-form-urlencoded`,
  8192 characters). No JSON body.
- korrid refuses `cookie`, `set-cookie`, `authorization`,
  `proxy-authorization` and `host` request headers from a plugin.
- A transport failure is never shown to the plugin (`ProviderUnreachable`).
  An HTTP error status is (`ok: false`).

Operations that use it today: `claims.search`, `claims.details`,
`artifact.resolve-download` (acquisition providers, ADR 0016) and
`game-assets.candidates` (artwork, ADR 0032).

### Where korrid may connect

- `plugin.nix` `hosts` (SCRIPTING.md line 588 on): lowercase DNS names,
  "no wildcard, no port, no scheme, no IP literal" (line 609), at most 128.
  The approval digest binds the list. korrid refuses any request or redirect
  hop whose host is not exactly in the list (line 612;
  `acquisition/policy.rs` `declared`).
- `acquisition/policy.rs:34` `public_address` refuses `0/8`, `10/8`,
  `127/8`, `100.64/10`, `169.254/16`, `172.16/12`, `192.168/16`, IPv6
  loopback, link-local, ULA, NAT64 and IPv4-mapped addresses. It checks DNS
  results too (`download.rs:250-271`). Provider traffic uses
  `AddressPolicy::Public` (`download.rs:39`); only tests may use loopback.

So a plugin cannot reach a server that the person names at setup, and
cannot reach a LAN or tailnet address at all.

### Secrets

ADR 0034, SCRIPTING.md "Plugin secret" (line 618 on): one secret for each
plugin, entered by the person, kept by korrid, never given to plugin code.
korrid adds it as `Authorization: Bearer <secret>`, only over https, only to
the one `credentialHost` (line 644: "Bearer is the only form"). Today only
artwork turns carry it (line 669); acquisition turns do not.

### State

Claims and jobs live in memory only (line 937). Only an acquired game becomes
a library record. Nothing on main keeps a remote catalog.

### Chunked calls already exist

`discovery.discover` (Steam, ADR 0011/0083) is called "one file per call"
(SCRIPTING.md line 1524): korrid drives the loop and the plugin parses one
unit at a time. That is the main precedent for korrid-driven paging.

### Proposed but unbuilt

`docs/briefs/2026-09-15-plugin-model/OPERATIONS.md:44` lists
`catalog.list | Cursor/filter | Static or runtime catalog page`. That draft
gave handlers a `services` object for HTTP (line 26), which ADR 0016 later
replaced with turns. The operation name is a proposal, not a decision.

### Rejected options on record

| Option | Where rejected |
|---|---|
| Effectful plugin HTTP service (legacy `services.http`) | ADR 0016 "Considered Options" |
| Opaque continuation state between turns | ADR 0016 "Considered Options" (it is a second state format that korrid must bound and trust) |
| Keep the fetcher in korrid (built-in adapter) | ADR 0032 "Considered options": "Rejected by the owner decision" |
| Give the secret to the plugin | ADR 0034 |

## What the three servers need

| Need | Jellyfin | Pyxis | Ossicle |
|---|---|---|---|
| Address | Person's own server | Tailnet only; LAN `/rpc` answers 404 (pyxis operations notes) | Tailnet; "the tailnet gates it" (ossicle README) |
| Auth | `Authorization: MediaBrowser Token="…"`; `ApiKey` query; `X-Emby-Token` only with legacy auth on | Bearer token (`rpc/transport.rs:48`) | None |
| Catalog call | `GET /Items` or `/Users/{userId}/Items` | `POST /rpc`, JSON tagged unions | `POST /rpc`, JSON tagged unions |
| Paging | `startIndex` + `limit`, `enableTotalRecordCount`, `fields`, `minDateLastSaved` | opaque `cursor` (`RpcGroupPage`) | `limit` window (`LibraryEpisodesRequest`) |
| Media bytes | Stream URLs on the same server | `/stream/:trackId` | `GET /audio/:episodeId` with Range |

Sources: Jellyfin
[`AuthorizationContext.cs`](https://github.com/jellyfin/jellyfin/blob/ed76cb36e5df4b5e5608f5e9d311edf145126679/Jellyfin.Server.Implementations/Security/AuthorizationContext.cs)
(lines 84-110 read `Token`, `X-Emby-Token`, `ApiKey`, `api_key`; lines 256-259
accept only the `MediaBrowser` scheme, or `Emby` with legacy auth) and
[`ItemsController.cs`](https://github.com/jellyfin/jellyfin/blob/ed76cb36e5df4b5e5608f5e9d311edf145126679/Jellyfin.Api/Controllers/ItemsController.cs)
(lines 171-260: `startIndex`, `limit`, `fields`, `minDateLastSaved`,
`enableTotalRecordCount`). Pyxis: `~/code/github/simonwjackson/pyxis`
`contracts/generated/pyxis.ts` (`RpcGroupPage.cursor`),
`services/pyxis/src/rpc/transport.rs:48`, `services/pyxis/src/api.rs:158`.
Ossicle: `~/code/sandbox/ossicle` `README.md` (`POST /rpc`, no auth, tailnet),
`contracts/generated/ossicle.ts` (`LibraryEpisodesRequest.limit`).

Gaps 1 to 4 in the answer come from this table. All three servers fail gap 2
(tailnet or LAN). Jellyfin fails gap 3. Pyxis and ossicle fail gap 4.

## Candidate shapes

| Shape | What the plugin does | What korrid does | Effects stay in korrid | Cost |
|---|---|---|---|---|
| **A. Turns, as on main** | Builds requests, parses responses, returns items | Fetches, enforces hosts, adds secret | Yes | Gaps 1-6 above. Replay makes a call's parse cost grow with every earlier response in the operation |
| **A2. Turns plus korrid-driven pages** (recommended) | Same as A, for one page; returns items and the next page's cursor | Holds the cursor between operations, stores the items, calls again for the next page, refreshes later | Yes | korrid must keep a catalog store and a cursor. The cursor is plugin-made text that korrid only carries back. That is near the "opaque continuation" ADR 0016 rejected, but between operations, not between turns. Needs an owner decision |
| **B. Built-in adapters** | Nothing, or names the adapter | Speaks Jellyfin, pyxis, ossicle itself, in Rust | Yes | Rejected for artwork (ADR 0032). Every server becomes korrid code; outside authors cannot add one. Fastest parse (Rust, no QuickJS bound) |
| **C. Plugin parses only, korrid pages** | Declares a fixed request template and a mapping from response to items | Runs the template, the paging and the mapping | Yes | The mapping is a new data language. It moves server logic into a Korri-specific format, against the "keep `plugin.ts` thin" rule. Kodi XML scrapers took this road and later added Python scrapers |
| **D. Effectful plugin with scoped fetch** (Extism, Deno, Figma style) | Calls `fetch` to allowed hosts | Enforces the allow list in the host call | No | Breaks the standing rule that plugins perform no effects. ADR 0016 rejected it |
| **E. Server speaks Korri's shape** | Nothing, or a small mapping | Fetches a Korri-shaped endpoint | Yes | Works only for servers the owner controls (pyxis, ossicle). Not for Jellyfin. Couples those servers to a Korri treaty |

### Cost at thousands of items

This is **arithmetic from the measured bounds**, not a measurement on a real
library. I did not measure the size of a Jellyfin item. Take `s` KiB as the
size of one item in a response.

- One response holds at most 512 KiB: `512 / s` items. At `s = 2`, 256 items.
- One call must parse its whole input inside 1000 ms. On fuji that is near
  2.9 MiB; on RG353M near 0.5 to 0.7 MiB (estimate). At `s = 2`, RG353M
  parses about 250 to 350 items in one call.
- Shape A replays: the fourth call of an operation gets every response of the
  first three turns. With 8 requests in a turn, one turn can already ask for
  4 MiB, the total input bound. So one operation fits about `4096 / s`
  items at most (2048 at `s = 2`) and its last call already passes the
  RG353M deadline.
- A2 keeps each call to one page. 10,000 items at 250 a page is 40
  operations. Each starts a fresh QuickJS runtime. The startup cost of a
  runtime is not measured here.
- B has no QuickJS cost. C has no QuickJS cost for parsing, but a mapping
  language with its own limits.
- Sparse fields cut `s`: Jellyfin `fields` and `enableImages` select what an
  item carries; `minDateLastSaved` allows a refresh of changed items only.
  Pyxis pages by cursor already. Ossicle asks for a window (`limit`).

## Prior art

| System | Who fetches | Where the allow list comes from | Paging / continuation | Lesson for Korri |
|---|---|---|---|---|
| Kodi XML scrapers | Kodi. The scraper returns `<url function="…">`; Kodi fetches it and passes the body to the named function as `$$1` | Scraper XML | Chained functions (`<chain>`, `<url>`) | Same shape as korrid turns. Kodi also runs Python scrapers that fetch themselves (`m_isPython`), so the pure form did not stay the only form |
| Envoy proxy-wasm | Host. `proxy_http_call` names an **upstream** the operator configured; the host calls `proxy_on_http_call_response` with the result | Operator configuration, not the plugin | Callback per call | The destination is a name that the operator binds, not an address the plugin writes. Fits gap 1: the plugin names "the person's server", korrid binds it |
| Extism | Host function `http_request`, called synchronously by the plugin | `Manifest.allowed_hosts`, glob patterns, set by the **embedding host** at load | None built in | The allow list is a load-time grant from the host, so it can include a host the user chose. Plugin calls are imperative: an effect, which Korri forbids |
| Deno | Runtime `fetch` | `--allow-net=host` at launch | None | Same lesson: the person or the embedder grants hosts at run time |
| Chrome extensions | Extension code | `host_permissions`, or `optional_host_permissions` with `https://*/*` and `permissions.request()` for hosts found at run time | None | A precedent for a host that the person approves after install, with a prompt |
| Home Assistant | Integration code, through the shared `async_get_clientsession` | Config flow: the person types `CONF_URL`, user name, password; the config entry keeps them | The Jellyfin media source loads all movies of a library at once | A per-instance setup step that stores the server address and the account is normal for a media client. No sandbox; Korri cannot copy the fetch model |

Sources: Kodi
[`xbmc/addons/Scraper.cpp`](https://github.com/xbmc/xbmc/blob/8673a33bf790fa5a79b8d9492f479d823dc5c065/xbmc/addons/Scraper.cpp)
(lines 240-300 `Run` and chaining; lines 322-345 `InternalRun` fetches each URL into
a parser parameter; lines 1033-1044 `CreateSearchUrl` then `GetSearchResults`;
line 174 `m_isPython`). proxy-wasm
[ABI v0.2.1](https://github.com/proxy-wasm/spec/blob/bf6919b429b2710dba581110a30a4639aa69fcda/abi-versions/v0.2.1/README.md)
(`proxy_http_call`, `proxy_on_http_call_response`, line 1175 on; "`BAD_ARGUMENT`
for unknown `upstream`"). Extism
[`manifest/src/lib.rs`](https://github.com/extism/extism/blob/051b765bb6abe4fe95d5e4f489f03ad7b137edbc/manifest/src/lib.rs)
(line 273-276 `allowed_hosts`: "if this is empty then no hosts may be accessed.
Wildcards may be used") and
[`runtime/src/pdk.rs`](https://github.com/extism/extism/blob/051b765bb6abe4fe95d5e4f489f03ad7b137edbc/runtime/src/pdk.rs)
(line 173 `http_request`, lines 214-233 glob check). Deno
[Security and permissions](https://docs.deno.com/runtime/fundamentals/security/)
(`--allow-net=example.com`). Chrome
[`chrome.permissions`](https://developer.chrome.com/docs/extensions/reference/api/permissions)
(optional host permissions for hosts discovered at run time). Home Assistant
[Config flow handler docs](https://developers.home-assistant.io/docs/config_entries_config_flow_handler/)
(`CONF_HOST` in the user step, `async_create_entry`),
[`components/jellyfin/config_flow.py`](https://github.com/home-assistant/core/blob/445744e80c0054127c6207c8fb1b50fdf00a5ba3/homeassistant/components/jellyfin/config_flow.py)
(lines 22-24 URL, user name, password) and
[`components/jellyfin/media_source.py`](https://github.com/home-assistant/core/blob/445744e80c0054127c6207c8fb1b50fdf00a5ba3/homeassistant/components/jellyfin/media_source.py)
(lines 343-359 "Return all movies in the movie library"). The Kodi wiki pages
were blocked by a Cloudflare check; the source file was read instead.

## Recommendation (opinion)

Use shape A2: the existing turns, one page for each operation, with korrid
holding the cursor and the received items. Cost: korrid gains a catalog store
and a refresh policy, and a page cursor that the plugin makes and korrid
only carries. It does not solve gaps 1 to 4; each needs its own owner
decision, and gaps 1 and 2 change the security policy that ADR 0016 approved.

What could still go wrong:

- A per-page QuickJS runtime may be too slow for 10,000 items on RG353M. This
  is not measured.
- Opening private and tailnet addresses weakens the SSRF guard for every
  plugin that gets it. The narrowest form is likely "the one address the
  person entered for this plugin", bound like `credentialHost`. That is an
  inference, not a design.
- Media bytes (streams) are a separate path. The player, not the plugin,
  fetches them, and Jellyfin needs the token on those requests too. This
  document does not cover playback.

## Questions for the map

1. How does a person give a client plugin its server address, and how does
   that address join the approved `hosts` (gap 1)?
2. Which private ranges may a client plugin reach (LAN, tailnet
   `100.64/10`, MagicDNS names), and how is that bound to one plugin (gap 2)?
3. Which secret forms does korrid send beyond `Bearer` (Jellyfin
   `MediaBrowser Token="…"`), and does a client plugin need a user name and
   password login that korrid performs (gap 3)?
4. Do turns admit a JSON request body for `POST /rpc` servers (gap 4)?
5. Where does korrid keep a remote catalog, how does it page and refresh it,
   and who holds the cursor (gaps 5 and 6)? This joins the open catalog
   question in #559.
6. How does a player get authenticated stream URLs without the plugin seeing
   the secret?

## Checks

- Every Korri claim above was read in the file named, at 28b33b44b.
- Every outside claim was read in the linked source file or page, except the
  Kodi wiki (blocked). No claim rests on a search summary.
- Not checked: Jellyfin item size in bytes; QuickJS runtime start cost;
  whether ossicle's same-origin check refuses a request with no `Origin`
  header.
