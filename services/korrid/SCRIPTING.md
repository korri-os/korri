# Plugin scripting in korrid

korrid can transpile and run TypeScript or JavaScript **at runtime**, on every
device Korri targets. A plugin is a self-contained ES module with named exports;
korrid performs any resulting effects itself. Completion values, default exports,
`namespace` exports, and the old `contributes` wrapper are rejected.

Plugin source is evaluated at runtime rather than compiled into native code.
The Linux administrator CLI in `plugin-host/` imports independently packaged
services and games. It uses this same interpreter and requires exact-package
approval before native execution. Linux discovery and launching read only the
committed enabled selection projection. They never invoke the administrator CLI.

## Source authority and snapshot checkpoint

This contract is extracted from the current producers and consumers. It does
not register the research probe's graph or authorize a Nix closure as code.

| Producer or consumer | Source authority and snapshot use |
|---|---|
| `plugin-host/builder.nix` | Trusted composition supplies `publisher` and a closed source root. The builder copies its admitted TypeScript and metadata files and records them in `manifest.json`; `plugin.ts` is the entry point. `packages`, `files`, `services` and `requires` are not JavaScript source grants. |
| `plugin-host/src/package.rs` | The namespace comes from the generated manifest after verification against the device's bound full key. Nix's signed fingerprint binds the exact output path, `narHash`, `narSize` and sorted references. Reference membership alone does not register executable source. Admission snapshots the registered source before evaluating it. |
| `Report.approval` | The existing approval digest covers policy, package, provenance, declaration, effective unit, complete `plugin.ts` bytes and manifest bytes. Evaluation and source approval must consume the **same** snapshot, not reopen source for hashing. No new receipt field or preparation cache is needed for the current single-file producer. |
| `plugin-host/src/host.rs::publish_registry` and `plugin_installation.rs` | The owner-protected registry projects the existing Report fields `id`, `package`, `files`, `requires`. `package` selects the previously approved immutable output; `id` supplies the approved namespace. The reader does not verify a second signature or infer source grants from native files or required packages. |
| `PluginRegistry::from_installed` | Read only the registered `plugin.ts` through bounded snapshot admission, then evaluate those bytes and verify the resulting ID. Do not use an independent unbounded source read. The filesystem adapter relies on the administrator's immutable-output authority; a temporary directory in a test is not installation authority. |
| `plugin_policy.rs` | Reads installed packages through `plugin_installation.rs`. No plugin source is compiled into korrid. |
| `services/korrid/tests/fixtures/script-packages/retroarch/package.json` and `bun.lock` | These core-owned fixtures pin the native package sources used by evaluator tests (`effect` is `4.0.1`; `fast-check` is listed directly because Effect 4.0.1 no longer depends on it). The plugin repository owns the production RetroArch source. These fixture packages are not registered runtime resolution inputs. |

The first U1 slice supplies an in-memory, closed byte snapshot, not a resolver.
Admission selects every identity explicitly. Source and resolution metadata,
when registered, use the same byte accounting; JSON is not an unmetered side
channel. Charge aggregate bytes, entry storage, identity storage and path
traversal before reads or owned allocations. Check regular-file sizes before
allocating content. Never use an unbounded `read_to_string` to construct a
snapshot. Completed snapshots expose only their retained bytes, without a
filesystem callback. Changing or deleting a file after admission cannot change
those bytes. This does not make a mutable directory an atomic or trusted
package; production still depends on immutable, verified Nix outputs.

Identities are normalized, package-relative paths under one exact directory.
Root ancestors must not be symlinks. Resolve selected relative symlink aliases
with bounded traversal before assigning identity; the canonical target must
also be explicitly selected. Reject escapes, loops, special files and missing
inputs. A link to another store output is not an implicit source grant. The
current single-file producer consequently still requires a regular `plugin.ts`.
Absolute symlinks also fail, including links back into the same output. No
recursive directory discovery or ancestor `node_modules` search is allowed.

**Approved source-tree contract, not yet implemented:** on 2026-09-11 the user
approved packaging a plugin's sources together. The existing builder `source`
input will carry the plugin entry, its helpers, native npm metadata, the lock and
the locked dependency sources as one tree copied into the signed output. The
signed output and its bound fingerprint remain the only authority. Admission
enumerates that packaged subtree with bounded traversal, entry, name and byte
accounting, accepts regular source files only, and keeps rejecting escapes,
loops, special files and cross-output links. Enumeration replaces the earlier
single-file rule for this producer because the plan forbids a second dependency
manifest; nothing outside the packaged subtree becomes importable. Approval must
cover every retained source, canonical identity and resolution input through the
same snapshot evaluation consumed. Administrator, installed-registry and
runtime-user launch consumers use that one admission path; no independent source
read remains. Native artifacts, required packages and closure membership still
grant no imports, and no new manifest or registry field is approved.

This contract is recorded, not delivered. Until it ships, installed registration
still selects a single `plugin.ts` and grants no helper or dependency imports.

The existing 128 KiB source ceiling remains the first slice's aggregate content
ceiling, not a measured Effect graph limit. Bounded snapshot storage is not a
bound on Oxc's parse/transform allocations or cancellation. A wall-clock check
between parses cannot interrupt one parse. Hard preparation memory and cancellation guarantees are deferred by the user's
2026-09-11 decision in
`work/items/active/01M26Z12QCQ708KHWXSDK80GAC-effect-plugin-validation-runtime/work.md`.
Preparation stays inside korrid. A pathological parse can stall or crash the
process, even with approved source. Keep the source-admission checks; no helper
process, parser fork or hard preparation watchdog is implied. QuickJS stays at
16 MiB / 1000 ms. This decision adds no platform APIs, timers, policy integration,
device operations or deployment.

### First U1 slice: implemented interface and verification

`script::source::SourceSnapshot` has two admission adapters:
`from_memory(sources, limits)` and `from_directory(root, names, limits)`.
`bytes`, `text` and `identity` only query retained entries; they cannot resolve
or read anything. `plugin(source)` and `package_plugin(package)` select the
current single-file contract. `script::eval_plugin_snapshot` consumes that
snapshot. `eval_plugin_ts`, bundled registration, installed registration and
administrator declaration evaluation use this seam. The administrator passes
that same source snapshot to its existing approval digest.

`SnapshotLimits` is a Rust call argument, not persisted configuration. Its
`bytes` bounds aggregate content, `entries` bounds entry slots and per-entry
reference-counted buffer headers, `path_bytes` bounds cumulative name/scratch
reservations, and `steps` bounds filesystem traversal. The shipped single-file
adapters use 131,072 content bytes, one entry, and the platform `PATH_MAX` for
both path reservations and traversal steps (4,096 on the tested Linux host).
Tests vary each limit independently. Aliases share a canonical buffer;
empty files still consume entry slots. Each symlink read reserves `PATH_MAX`
bytes before the syscall. This conservative accounting may reject a graph long
before its content ceiling. It is not a measured full-graph retention policy.

Directory traversal uses descriptor-relative `openat` with `O_NOFOLLOW` and
`O_NONBLOCK`; it does not canonicalize an unbounded path through libc. The
snapshot checks regular-file length, reserves it, then reads exactly that many
bytes into fixed-capacity storage. A final length check detects size changes
without reading an unreserved sentinel byte. This bounds requested source
storage, plus fixed descriptor/stack overhead. It does not bound allocator
overhead, process RSS or Oxc allocations. The accepted in-process preparation
risk does not remove these admission checks. Preparation measurements must
remain separate from the QuickJS execution budget; they are not enforceable
whole-process memory or cancellation guarantees.
The runtime-user launch process's separate source-path read is not migrated by
this slice; U4 must carry source admission through that consumer as well.

Development and test builds optimize `rquickjs-sys` at level 3, the release
default. The Rust tests keep their normal debug profile. The package graph tests
must meet the same 250 ms VM deadline as release builds. An unoptimized C
interpreter can spend that entire budget compiling a valid graph. This costs
more compilation work when that C dependency rebuilds. It does not change the
guards or test parallelism. It also does not reserve CPU time: host contention
can still exhaust the wall deadline even when the VM uses less than 250 ms of
CPU time.

Native package tests use the two existing frozen fixture locks. The normal
`korrid-check` task provisions them before the Rust tests. For focused package
tests, run `services/korrid/script-fixtures-setup.sh` first, or
`KORRI_SCRIPT_FIXTURES_IN_SHELL=1 bash services/korrid/script-fixtures-setup.sh`
inside the existing devshell. Installs are frozen with install scripts disabled,
and they need cached downloads or network access on the build machine. Missing
fixtures fail; they are never silently skipped.

Focused verification uses the existing Rust toolchain, with Cargo locks frozen:

```sh
cargo test --offline --locked --manifest-path services/korrid/Cargo.toml --test script_sources
TMPDIR=/dev/shm cargo test --offline --locked --manifest-path services/korrid/Cargo.toml --test script_sources
cargo test --offline --locked --manifest-path services/korrid/Cargo.toml --lib script::
cargo test --offline --locked --manifest-path services/korrid/plugin-host/Cargo.toml --lib --test declaration --test game_declaration
```

Verified on the build machine: 13 source tests pass on both `/tmp` and tmpfs;
6 plugin-policy tests, 27 registry tests, 8 interpreter tests and 3 focused
installed-launch tests passed in the initial slice. The corrected full installed
launch test file now passes four tests and leaves the built-payload test ignored.
The administrator run passes 30 tests.
Both crates pass all-targets `cargo check` and `cargo fmt --check`. Strict
all-targets Clippy passes for the administrator host.

The source contract suite includes filesystem and tmpfs admission, exact and
aggregate limits, sparse oversized files, metadata accounting, canonical alias
sharing, symlink cycles/escapes, special files, missing entries, frozen bytes and
installed-registry rejection. A real Linux inotify access watch proves size
rejection and unselected-target rejection happen without a content read; a
positive-control read proves that the watch is active. The parent-component
regression first failed with `alias.ts -> redirect/../plugin.ts` and
`redirect -> real/deep`: lexical collapse selected the wrong `plugin.ts`.
The corrected traversal resolves directory symlinks before `..`.

**Stale regression assertion corrected separately:** the installed launch suite's
`real_linux_plugins_declare_immutable_file_keys_and_return_the_existing_retroarch_command`
required duplicate `video_vsync` assignments, contradicting the native parser
fix. It also failed on an untouched archive of `c751233c`. The assertion now
requires exactly one winning assignment, and this command passes:

```sh
cargo test --offline --locked --manifest-path services/korrid/Cargo.toml --test installed_game_launch real_linux_plugins_declare_immutable_file_keys_and_return_the_existing_retroarch_command
```

Use a separate Cargo target directory for baseline archives; reusing one caused
a stale baseline rlib to hide the new snapshot API. Cleaning only the local
`korrid` package artifacts and rebuilding restored the source suite. Strict
whole-korrid Clippy also reports existing errors outside the changed files,
including `discovery/scanner.rs:184` and async test locks in `src/lib.rs`.
Those unrelated Clippy findings are not included in this source-admission change.

## Shape

```
plugin.ts  ──(oxc: transpile in-process)──▶  JS  ──(QuickJS: evaluate)──▶  declaration JSON
```

| Layer | Crate | Why this one |
|---|---|---|
| TS → JS | `oxc` | Full transpile, not just type-stripping — `enum`, decorators, parameter properties all work |
| Execute | `rquickjs` (QuickJS) | A small in-process interpreter |

## Named exports and publisher identity

`script::eval_plugin_ts(source)` transpiles and evaluates a module. It returns
JSON containing its data exports, not the JavaScript completion value.
`name` must be a non-empty string. Optional identity exports are `title` and
`description`. Data export names come from the current producers: `providers`,
`systems`, `groups`, `games`, `families`, `runners`, `artifacts`, `transports`,
`controls`, and the constrained `discovery` list described below. Native
launcher/runtime fields follow the instance/kind model described below.
`sessionControls` is an unknown export and fails admission. `services` names
native units packaged by `plugin.nix`. The Linux host selects those units; the data registry validates the names but does not
activate them or change its existing records. `daemons` and the old `contributes`
wrapper are rejected. No TypeScript export encodes systemd directives. Native `LimitRTPRIO=1..20` is a bounded, explicit privileged approval request; the plugin-host [native-unit reference](plugin-host/README.md) defines its strict scalar grammar and limits. It does not promote a service or grant `CAP_SYS_NICE`.

`plugin::load_plugin_source(namespace, source)` and
`decode_plugin_declaration(namespace, json)` receive publisher identity from
the caller. Nothing infers it from a filename or trusts a namespace inside
plugin source.
External daemon packages instead carry a generated `manifest.json` with
`publisher.namespace`. The host verifies the selected NAR against the device's
bound full public key before using that claim. See `plugin-host/README.md`.

An optional `handlers` export is a map of operation name to handler function.
The host names the operation it wants; a plugin never chooses which of its
handlers runs. Evaluation checks the shape — a plain object whose every value
is callable — but never calls a handler. A plugin may carry a handler this host
does not know: admission does not judge the names, and dispatch reports the
unimplemented operation instead of returning an empty success.

`script::call_plugin_operation_ts(source, operation, input)` invokes one handler
in a fresh interpreter with bounded JSON input and output. `LAUNCH_PREPARE` is
the operation that turns a selected runner and target into a launch plan; a
native runner is admitted only when it carries that handler. Module evaluation
and invocation share one deadline, memory limit, and output budget. A handler
must return JSON synchronously; a promise, function, non-finite number, or
undefined is rejected.
The evaluator performs no effects. `launcher/plugin_launch.rs` defines the
Rust/Typeshare `PluginLaunchInput` and `PluginLaunchOutput` treaties. Input
contains the selected full launcher/kind/runtime IDs, program and core file paths,
existing account root, the cache root (the storage root's `cache` directory),
kind-owned manifest files under their bare keys, and each exactly required
plugin's files under `<pluginId>/<key>`
(ADR 0012 records artifact acquisition; ADR 0010 records the original file
keying). Ready artifact paths arrive separately in `artifacts`. A file route supplies
`contentPath`; a provider location supplies `ref` instead, named after
`GameProviderIdentity.ref`. Neither path invents a file for provider content.
Output
preserves legacy `command`, `args`, `env`, `envUnset`, and `cwd`, plus the
existing directory and provisioned-file declarations.

`settings.describe` returns the runner's own schema fragment and the revision
it is valid for. `settings.validate` reports what this build cannot apply.
`launcher/typed_settings.rs` calls the second one before a launch starts and
turns its diagnostics into the existing `LaunchWarning` wire type, adding the
runner id and build, which are korrid's facts and not the plugin's to claim.
korrid keeps no key table and reads no settings metadata file: a runner that
implements neither operation says nothing, which is silence and not an error,
and authored values are never rewritten or deleted.

The `default` on each `settings.describe.schema.properties` entry is the
plugin's display-only base value. korrid passes only authored settings to
`launch.prepare` and `launch.compose`. Settings are shared unless their property
marks `x-korri-scope: device`; other marks produce a diagnostic. The snapshot
carries the schema and revision so the editor can show titles, descriptions,
enums and numeric bounds. Setting writes must satisfy the installed runner's
schema (any installed member for a family place). Installed modifiers also
appear before any place enables them. Their optional settings schema follows
the same rule; a modifier with no describe handler has no schema.

ADRs 0028 and 0029 define one place cascade for runner settings, modifiers,
limits, environment, arguments, working directory and hooks. The plugin's
`launch.prepare` environment is the base: place values override it per key and
null unsets a key. Final arguments are place prepend, plugin arguments, then
place append. Place cwd overrides plugin cwd. These changes happen once before
modifier composition, in korrid's private handoff, never in plugin input.
The portal cannot author environment, arguments, cwd, hooks or raw config.
One-launch requests carry only the restricted `launch: LaunchOverride` shape;
plugin `PluginLaunchInput.overrides` remains the authored runner settings and
file-only native config consumed by the callback.

`runtime.resolve` asks the runner whether it can start this target now, and what
is missing if not. `launcher/runtime.rs` calls it before a launch starts, with
the same `PluginLaunchInput` that `launch.prepare` receives, because both
operations ask about the same runner and the same target. A runner whose runtime
is its own package declares no handler and proceeds; a runner that needs
something else refuses in its own words, and korrid spawns nothing. The refusal
reaches the person through the existing `LocalRouteUnavailable` failure.

`launcher/linux_plugin.rs` builds a private preparation command. Before starting
its game unit, korrid passes it through `prepare_command`, which evaluates
`launch.prepare` and each enabled modifier's `launch.compose` in the effect-free
sandbox. The unit receives `korrid plugin-launch SOURCE INPUT_JSON
COMPOSITION_JSON PREPARED_JSON` and executes that prepared plan as the runtime
user. It does not evaluate plugin source again. In a private PID namespace it
stays as PID 1 and supervises the composed command as its child; otherwise it
replaces itself with that command. It refuses root. Callback-selected files,
environment and cwd are applied only during execution, never during preparation.
Approval authorizes output; runners and modifiers add no path or argument policy.

Plugin authors and sandbox fixtures can run both stages through the supported CLI:

```sh
korrid plugin-run SOURCE INPUT_JSON COMPOSITION_JSON RELEASE_ID
korrid plugin-run --help
```

The arguments are the same source, `PluginLaunchInput`, `LaunchComposition` and
selected release identifier used by the normal preparation command. A fixture
supplies its test release identifier. This entry calls the real `prepare_command`
then the real executor; fixtures must not construct private `PREPARED_JSON`.
Preparation failures apply no declared effects. Run it as an unprivileged user
inside the fixture's sandbox: this CLI does not create a systemd unit, establish
permissions, or perform session save sync. The old three-argument `plugin-launch`
form is not supported.

Native instances use the approved `{ id, kind, program }` fields. A runtime
uses `launcher` and its existing `path` field as a manifest file key. The
instance owns the program key, the runtime owns the core key, and the kind
owns callback source and auxiliary files. The kind's default instance points
to itself.

### Declared artifacts

ADR 0012 supersedes plugin-owned downloads. An effect-free `artifacts` export
is a map keyed like `runners`:

```ts
export const artifacts = {
  rootfs: {
    id: "@korri:fex/rootfs",
    version: "archlinux-2026-01-08-mesa-26.1.2",
    files: [{
      url: "https://rootfs.fex-emu.gg/ArchLinux/2026-01-08/ArchLinux.sqsh",
      sha256: "sha256:cb059973b7953ad9165845529655189b96f9a174b14a6a149c87ec884b0c5e90",
      bytes: 1160000000,
    }],
    unpack: "fex-unpack",
  },
}
```

Each strict record has `id`, `version`, ordered non-empty `files`, optional
`unpack` and optional `title`. Each file has `url`, `sha256` and optional numeric
`bytes`. IDs must match the declaring plugin and map key. Versions must be one
safe non-hidden path segment. SHA-256 values require `sha256:` and 64 hex digits;
admission lowercases the digits and rejects duplicates. `unpack` is a bare key
in that plugin's own installed `files`. Unknown fields and explicit nulls fail.

`RunnerRecord.artifacts` lists the full artifact IDs the runner needs, including
artifacts from other enabled plugins. Every ID must resolve. Omission means an
empty list. A runner does not acquire all artifacts of its package dependencies.

`PluginLaunchInput.artifacts` maps ready artifact IDs to paths. The runner's own
artifacts use bare keys; foreign artifacts use `<pluginId>/<key>`, as `files`
does. The field is required, including for runners with no artifacts (`{}`).
Only the listed artifacts appear. Actual launch refuses with the existing
`RpcFailure` code `ArtifactsNotReady` until all required directories exist.
The caller obtains their IDs and titles from
`app.acquisition.status {gameId, runnerId?}`. An explicit `runnerId` selects the
same route as `app.session.start`; without it, the saved/default route rules
apply. Without `gameId`, status lists all enabled artifacts.

An unpack program runs as `<unpack> <work-dir> <file>...`. Files are verified
inputs in declaration order. The program writes only under `<work-dir>`, exits
0 for success or 1 for failure, and writes diagnostics to stderr. There is no
stdout contract. It does not download or rename the ready directory. korrid
runs it in a transient systemd unit with no network, no privilege escalation,
and the contribution directory as its only writable filesystem path. Its name is
`korri-acquire-<plugin-short-name>-<key>.service`. Recovery holds `.lock`, stops
an orphaned named unit, then removes stale `.tmp-*` directories. It never binds
the unit to a daemon service name, so a standalone korrid uses the same path. Direct
spawn exists only in Rust unit tests, not as a production fallback.

korrid stores partials under
`<storage-root>/cache/<plugin-short-name>/<key>/.downloads/<hex>.part`, verifies
SHA-256 while streaming, and renames verified files to `<hex>`. It holds a
per-contribution in-process exclusion and `.lock` across download and unpack.
After unpacking in `.tmp-*`, korrid removes write permissions, syncs the tree,
atomically renames it to `<version>/`, and removes `.downloads`. Without an
unpack program, the version directory contains verified files named by the
percent-decoded last segment of each declared URL. Unsafe or duplicate names
fail. Ready means that the version directory exists. There is no journal.

The local portal polls `app.acquisition.status`, starts IDs through
`app.acquisition.start`, and cancels one download through
`app.acquisition.cancel`. Downloading reports `bytesDone`, optional
`bytesTotal`, and a rate sampled over the last five seconds. Failure copy never
contains a URL, hash, path, HTTP status or unpack stderr.

ADR 0013 adds a download queue. One job runs at a time; a waiting artifact
reports `Queued`. `app.acquisition.start { artifactIds }` still means that a
launch is waiting: its artifacts move to the front of the queue, in request
order. A background job that is downloading pauses, reports `Queued`, and
resumes next with a `Range` request. Unpacking is never paused.
`app.acquisition.download { gameId }` resolves the game's default route as
`app.acquisition.status` does and appends every artifact that is not ready to
the end of the queue. The download continues when the person leaves the game.

Cancel stops the download, removes the artifact from the queue and deletes its
`.downloads` directory: partial and verified files alike, so a cancelled
download starts again from zero. Unpacking cannot be cancelled. When korrid
queues an artifact it creates `.downloads/<hex of file 1>.part`, with zero bytes
if nothing has arrived. Any entry in `.downloads` with no ready directory
therefore means an interrupted download, including a restart between two files
or during unpacking. At startup korrid queues every such contribution, ordered
by the earliest creation time of an entry in its `.downloads`. That entry is the
file 1 marker, which keeps its creation time when it is renamed to its verified
name. The creation time records when the person asked; the modification time
moves while bytes arrive. Where the filesystem reports no creation time, korrid
uses the modification time, which can put a running download behind a later
one. Process restart forgets in-memory failures and progress. No new state is
saved.

While a job downloads or unpacks, korrid holds a logind `sleep` inhibitor in
`block` mode. The screen may blank, but the device does not suspend. Shutdown
is not blocked. Without logind, korrid logs the failure and continues.

`app.acquisition.remove { gameId }` frees space. It removes the ready
directories of the artifacts that this game's declared releases name. It never
removes an artifact that any runner lists, or one that a release of another
game in this device's library names. It takes each contribution's `.lock`
first and never touches saves or account roots. While the game runs it refuses
with `ActiveSessionConflict`. While one of the game's artifacts is queued,
downloading or unpacking it refuses with `AcquisitionBusy`; the person cancels
first.

Game-scoped results (`app.acquisition.status { gameId }`, `.download` and
`.remove`) set `release: true` on each artifact that one of the game's declared
releases names. Runner artifacts and device-wide status omit the field.

### Acquisition providers

ADR 0016 records the decision. A provider plugin finds games on one site and
says where their files are. It performs no I/O: korrid makes every HTTP request
and every download, and the plugin only reads what korrid fetched. Legacy
names come from `product/platform/protocol/acquisition/*.ts`,
`product/platform/plugin/services.ts` and
`product/apps/portal/api/acquisition/*.rpc.ts` on the `legacy` branch. Every
type named below is a Rust type exported through Typeshare into
`contracts/generated/korrid.ts`; that file is the exact wire.

#### Declaration

```ts
import type {
  PluginDetailsInput, PluginDetailsOutput,
  PluginResolveDownloadInput, PluginResolveDownloadOutput,
  PluginSearchInput, PluginSearchOutput,
} from "korrid"

export const name = "pico8"
export const title = "PICO-8 BBS"
export const providers = {
  "@korri:pico8": { id: "@korri:pico8", title: "PICO-8 BBS" },
}
export const handlers = {
  "claims.search": (input: PluginSearchInput): PluginSearchOutput => { /* ... */ },
  "claims.details": (input: PluginDetailsInput): PluginDetailsOutput => { /* ... */ },
  "artifact.resolve-download": (input: PluginResolveDownloadInput): PluginResolveDownloadOutput => { /* ... */ },
}
```

An explicit `providers` export declares acquisition providers (the existing
`ProviderRecord { id, title }`). Each explicit record needs a non-empty
`title`. A plugin that exports `providers` must carry callable
`claims.search`, `claims.details` and `artifact.resolve-download` handlers;
evaluation checks that they exist and never calls them. The implicit own-id
provider that every plugin receives is not an acquisition provider.
`app.acquisition.providers` lists only the explicit records of enabled
plugins, each with its owning `pluginId`.

#### Declared hosts

`plugin.nix` gains `hosts`, a list of strings beside `ports`:

```nix
{
  hosts = [ "www.lexaloffle.com" ];
}
```

The name follows the existing manifest style (`ports`, `requires`). The
builder copies the list into `manifest.json` as `hosts`, and, like
`requires`, omits the key when the list is empty, so a package without hosts
keeps its manifest bytes and approval. The administrator validates it, shows it
as the report field `hosts` in `inspect`, binds it in the approval digest
through the manifest bytes, and projects it into `enabled-packages` as
`EnabledPackage.hosts` (always present; `[]` when none). korrid reads it only
from that projection.

Each entry is a lowercase DNS name of at least two labels (`[a-z0-9]`, inner
`-`, each label 1 to 63 bytes, 253 bytes in total), no trailing dot, no
wildcard, no port, no scheme, no IP literal. Entries are distinct; at most 128,
the `ports` bound. A non-empty `hosts` needs a `providers` export, as `ports`
needs a service. korrid refuses any request and any redirect hop whose host is
not exactly in the owning plugin's list, both for provider turns and for the
game download. An empty list means no network. The SSRF and redirect policy in
`acquisition/policy.rs` still applies to every hop.

#### Request and response turns

A handler cannot fetch. It returns either its result or the HTTP requests it
needs, and korrid calls it again with the responses. The shape follows
`discovery.discover` (host-read text in, pure result out) and the legacy
`services.http.request(url, PluginHttpRequestOptions)` / `PluginHttpResponse`
pair, with the call turned into a returned value.

| Operation | Input type | Output type |
|---|---|---|
| `claims.search` | `PluginSearchInput { providerId, query, responses }` | `PluginSearchOutput = PluginHttpRequests \| ProviderClaim[]` |
| `claims.details` | `PluginDetailsInput { providerId, id, responses }` | `PluginDetailsOutput = PluginHttpRequests \| ProviderClaimDetails` |
| `artifact.resolve-download` | `PluginResolveDownloadInput { providerId, candidateUrl, fileName?, responses }` | `PluginResolveDownloadOutput = PluginHttpRequests \| DownloadResolution` |

Inputs are the legacy request for the operation plus `responses`.
`providerId` is the provider korrid asks for (legacy `context.provider`).
`query` is legacy `SearchRequest.query`; `platforms` is not carried. `id` is
legacy `DetailsRequest.id`. `candidateUrl` and `fileName` are legacy
`ResolveDownloadRequest` fields: `candidateUrl` is the held claim's `url`, and
`fileName` is the `acquire` request's `fileName`. `size` and
`artifactFormat` are not carried.

**The tag.** An output object whose `_tag` is `"HttpRequests"` asks for more
data. Anything else is the operation's result: a JSON array for
`claims.search`, and an object tagged `ProviderClaimDetails`,
`FinalDownload`, `NonFinalDownload` or `FailedDownload` for the others.

```ts
interface PluginHttpRequests { _tag: "HttpRequests"; requests: PluginHttpRequest[] }

interface PluginHttpRequest {
  url: string                                     // absolute http(s), no credentials
  query?: Record<string, string | number | boolean>  // PluginHttpQueryValue
  headers?: Record<string, string>
  timeoutMs?: number                              // whole request, positive integer milliseconds
  method?: "POST"                                 // absent: GET, same as FinalDownload
  body?: Record<string, string>                    // form fields; needs method "POST"
}

interface PluginHttpResponse {
  status: number
  ok: boolean                       // 200 to 299
  url: string                       // final URL after redirects
  headers: Record<string, string>   // lower-cased names; repeated headers joined with ", "
  text: string                      // body decoded as UTF-8; invalid sequences become U+FFFD
}
```

`url` is the first argument of the legacy `request` call; `query`, `headers`
and `timeoutMs` are the legacy `PluginHttpRequestOptions` fields. `query`
follows legacy `withQuery` (`searchParams.set`). Legacy had no default
timeout, so none is invented: without `timeoutMs` the download policy's
connect timeout and stall detection still end a dead request. `method` and
`body` reuse `FinalDownload`'s form semantics: absent method means GET; only
`"POST"` is admitted, with string-valued form fields. A body without POST,
null fields, other methods, or a form-encoded body above 8192 characters fails
before any request in that turn is sent. korrid uses the existing game-download
sender and `application/x-www-form-urlencoded` encoding, not a second client.
After POST, 301/302/303 become GET without the body; 307/308 repeat the POST.
This narrow addition supports public raw Bitsy attachments on itch.io; it
adds no cookie session, authorization, host wildcard, or resource-limit change
(ADR 0016's provisional public-attachment amendment). korrid sends
`User-Agent: Korri/<korrid version>` on every acquisition request and
redirect hop; a request's own `user-agent` header replaces it. korrid refuses `cookie`,
`set-cookie`, `authorization`, `proxy-authorization` and `host` request
headers.

korrid follows redirects itself (at most 5, no https to http, every hop
host-checked), so a response never has a 3xx status. An HTTP error status is a
response with `ok: false`; the plugin decides what it means. `setCookies` and
`bytes` are not carried.

`responses` holds every response to the plugin's earlier requests in this
operation, in request order across turns. korrid calls the handler again with
all responses so far; there is no opaque continuation state. Bounds per
operation: at most 4 handler calls (the fourth must return a result), 8
requests per turn, 512 KiB of body text per response, and 4 MiB of serialized
handler input in total, measured after JSON escaping (not raw body bytes).
The per-response bound matches the 512 KiB launch input bound; the larger
total allows replay of multiple responses, and the existing 16 MiB interpreter
heap must hold that input. ADR 0016's owner decision of 2026-10-23 lowers the
body bound based on `/tmp/korri-perf/results/summary.md` and
`/tmp/korri-perf/results/report-table.md`: QuickJS parsing on the aarch64
builder fuji costs about 340 ms per MiB, reaching the 1000 ms deadline near
2.9 MiB. RG353M is estimated, not measured, to be
4 to 6 times slower (deadline near 0.5 to 0.7 MiB); observed real pages peaked
at 174 KiB (WoWROMs search), with most at 16 to 50 KiB. A quote-heavy 3.5 MiB
benchmark page became 4.30 MB of serialized input and exceeded the unchanged
4 MiB input bound even before the body bound was lowered.

The existing 1000 ms deadline, 16 MiB heap and output budget apply to each
call. A request for zero requests, exceeding a handler or request bound, a
thrown error, a request to an undeclared host, or an invalid result is a
provider failure (`ProviderFailed`). A transport
failure (DNS, connect, timeout, stall, oversize body, refused redirect) fails
the operation as `ProviderUnreachable`; it is never delivered to the plugin.
Neither crashes korrid.

#### Results

The legacy results are preserved verbatim with these deltas: `ref`, `playable`
and `facets` are dropped (their only consumer authored library entries;
discovery owns library records on main), and `FinalDownload.requestHeaders` is
dropped (no in-scope provider forwards Referer or Cookie). `FinalDownload`
gains `method` and `body` (ADR 0030), from legacy
`PluginHttpRequestOptions.method` and `.body`.

```ts
interface ProviderClaim {
  _tag: "ProviderClaim"
  providerId: string        // must equal the asked provider
  id: string                // 1 to 2048 characters
  title: string             // SafeText: at most 8192 characters
  url: string               // SafeUrl: absolute http(s), no credentials
  platform?: string         // SafeText
  thumbnailUrl?: string     // SafeUrl
  artifact?: ArtifactAcquisitionHint
  fetchedAt?: string
}
interface ProviderClaimDetails {
  _tag: "ProviderClaimDetails"
  providerId: string
  id: string                // must equal the asked id
  title: string
  url: string
  description?: string      // SafeText
  downloadPageUrl?: string  // SafeUrl
  artifact?: ArtifactAcquisitionHint
  fetchedAt?: string
}
interface ArtifactAcquisitionHint {
  kind: "content" | "patch"                  // ArtifactKind
  system?: string        // the Korri system id, e.g. "gb"; decides where the file goes
  format: SemanticFormat  // { id: /^[a-z0-9][a-z0-9._-]{0,63}$/, version?: non-empty }
  file?: ArtifactFile     // { name, extension?, mediaType?, sizeBytes? }
}
type DownloadResolution = FinalDownload | NonFinalDownload | FailedDownload
interface FinalDownload {
  _tag: "FinalDownload"
  providerId: string
  url: string             // SafeUrl; its host must be declared
  filename?: string       // SafeFileName, no control characters; names the download
  contentType?: string
  method?: "POST"         // absent: GET
  body?: Record<string, string>  // form fields; needs method "POST"
}
interface NonFinalDownload {
  _tag: "NonFinalDownload"
  providerId: string
  reason: "interstitial" | "requires-user-action" | "unsupported"
  url?: string
  choices?: DownloadChoice[]  // { id, fileName?, size?, platforms? }
}
interface FailedDownload {
  _tag: "FailedDownload"
  providerId: string
  reason: "provider-error" | "configuration" | "not-found" | "defective-provider"
  message: string           // diagnostics only; never shown to the person
}
```

Unknown fields and explicit `null` values fail admission. `file.name` follows
legacy `SafeFileName` (no `/`, `\`, NUL, `.` or `..`); `file.extension` is
lowercase `[a-z0-9][a-z0-9-]{0,31}`; `sizeBytes` is a positive integer. A
claim whose `artifact.system` is absent or names no enabled system is shown but
cannot be acquired (the legacy "ungetable" guard). No provider result
carries an expected digest: legacy carried `expectedDigests` only on
`artifact.acquire` output, which main does not carry.

`method: "POST"` sends the game download as a form POST. korrid encodes
`body` as `application/x-www-form-urlencoded` and sets that `Content-Type`;
it adds no other header. The encoded body is at most 8192 characters (the
`SafeText` bound). A `body` without `method: "POST"`, or any other `method`
value, fails admission. After a POST, a 301, 302 or 303 redirect is followed
with a GET and no body; a 307 or 308 redirect sends the same POST again. Every
hop must be a declared host, as for a GET. korrid makes one attempt and never
retries a store download, so a POST is sent at most once per hop.

#### RPC

Local `/rpc`, portal capability only (like `app.acquisition.status`), legacy
method names. As for every method, the response is `{ _tag, outcome }`; each
outcome is `{ _tag: "Ok", payload }` or `{ _tag: "Err", payload: RpcFailure }`.
The store runs wherever discovery runs on the device's own library: the
brain's local library, or a device korrid's installed library root (ADR 0017).
A korrid with no installed library answers every method with
`AcquisitionUnavailable`.

| Method | Request type | Outcome type | `Ok` payload |
|---|---|---|---|
| `app.acquisition.providers` | `AcquisitionPluginsPayload {}` | `PluginListOutcome` | `PluginListResponse { providers: AcquisitionProvider[] }`, `AcquisitionProvider { id, title, pluginId }` |
| `app.acquisition.search` | `SearchRequest { query, providerIds }` (exactly one id) | `SearchOutcome` | `SearchResponse { claims: ProviderClaim[] }` |
| `app.acquisition.details` | `DetailsRequest { providerId, id }` | `DetailsOutcome` | `ProviderClaimDetails` |
| `app.acquisition.acquire` | `AcquireArtifactRequest { providerId, id, fileName? }` | `AcquireJobOutcome` | `AcquireJobStatus` |
| `app.acquisition.acquire-status` | `AcquireStatusRequest { jobId }` | `AcquireJobOutcome` | `AcquireJobStatus` |
| `app.acquisition.acquire-cancel` | `AcquireCancelRequest { jobId }` | `AcquireJobOutcome` | `AcquireJobStatus` |

```ts
interface AcquireJobStatus {
  jobId: string
  providerId: string
  id: string
  fileName?: string     // the request's fileName, then the downloaded file's name
  system?: string       // the claim's artifact.system
  message?: string      // the Failed reason message, else absent
  state: AcquireJobState
}
type AcquireJobState =
  | { _tag: "Queued" }      // waiting in ADR 0013's one-at-a-time queue
  | { _tag: "Resolving" }
  | { _tag: "Downloading"; bytesDone: number; bytesTotal?: number; bytesPerSecond?: number }
  | { _tag: "Unpacking" }   // korrid's archive reader runs in its sandbox unit
  | { _tag: "Placing" }     // publishing into the cache and writing the library record
  | { _tag: "Imported"; gameId: string }
  | { _tag: "Failed"; reason: RpcFailure }
  | { _tag: "Cancelled" }
```

`acquire` checks, in this order:

1. A live job for the same claim (`Queued`, `Resolving`, `Downloading`,
   `Unpacking`, `Placing`) answers again (legacy `activeByClaim`).
2. The provider link `<providerId>/<id>` names a game whose download is in the
   cache: a new job answers `Imported { gameId }` at once. This needs no claim
   in memory and no network, so Get on an owned game works after a restart.
3. Otherwise korrid needs a claim that `search` or `details` returned in this
   process (`AcquisitionUnknownClaim`). It takes the URL, title and system
   from that claim, never from the caller; `details` replaces the held
   claim's fields. A claim whose system is unknown answers
   `AcquisitionSystemUnknown`. A new job starts `Queued`.

`acquire-cancel` stops a `Queued`, `Resolving`, `Downloading` or `Unpacking`
job; a queued job leaves the queue, a downloading one loses its staging file,
and an unpacking one stops its unit and loses its work folder and archive
(ADR 0030; an ADR 0012 artifact still cannot be cancelled while it unpacks).
A `Placing` or finished job is returned unchanged. Claims and jobs live in memory only.
After a restart `acquire-status` answers `AcquisitionUnknownJob`, startup
removes leftover staging files and `.tmp-*` directories, and a store download
never continues: korrid holds no expected hash and no saved URL.

`RpcFailure.code` values for `Err`: `AcquisitionUnavailable` (no local
registry or discovery on this device), `AcquisitionCaller` (malformed request,
several or zero provider ids, an unusable `fileName`),
`AcquisitionUnknownProvider`, `AcquisitionUnknownClaim`,
`AcquisitionUnknownJob`, `AcquisitionSystemUnknown`, `ProviderUnreachable`
(transport) and `ProviderFailed` (plugin error, invalid output, exceeded
bound). `Failed.reason.code` values: `DownloadNotFinal` (`NonFinalDownload`),
`DownloadResolutionFailed` (`FailedDownload`), `ProviderUnreachable`,
`ProviderFailed` and `AcquisitionFailed` for download, validation, publication
and library-record failures. Every message is fixed plain copy: none contains
a URL, path, query, header value, HTTP status or plugin-supplied text.

#### A provider download stays in korrid's cache

This builds on ADR 0013 (declared games can come from artifacts): korrid never
writes into a person's source. A provider download has no declaration, so it reuses
ADR 0012/0013's cache grammar, readiness, content-hash identity and
`contentPath`, plus the ADR 0002 provider link (owner answers Q1-Q6, ADR 0016).

- **Queue.** A store job waits in ADR 0013's one-at-a-time queue behind
  earlier downloads and holds its deep-sleep block. It is never paused for a
  launch; a launch's download runs after it.
- **Download.** The file stages under
  `<storage-root>/cache/<plugin short name>/<provider name>/.downloads/`
  (the provider name is the part of the provider id after `:`), private
  (0700), with a random `.acquire-*` name. It follows the declared hosts and
  the SSRF/redirect policy, ADR 0012's connect timeout and stall detection.
  It has no size cap (ADR 0030). Before each write korrid checks that the
  cache's file system keeps at least 5% of its size free for korrid's user
  (`fstatvfs`), counting the bytes it wrote; at the floor, or when a
  `Content-Length` is larger than the space above the floor, the download
  fails with "There is not enough space for this download." It refuses an
  empty body and a web page (legacy `rejectNonArtifactPayload`).
- **File name.** `FinalDownload.filename`, else the `FinalDownload.url`'s last
  segment (ADR 0030). The name must pass `SafeFileName` and contain no control
  character. Before the download it must end in one of the claim system's
  extensions or in `.zip`, `.7z` or `.rar`, or have no extension at all
  (romhustler gives `"Zool"`); any other extension fails before the
  download. A name with no extension and bytes that are not an archive is
  "not a game for this system".
- **Unpacking.** After the download the first bytes decide: zip
  (`50 4b 03 04`, `50 4b 05 06`, `50 4b 07 08`), 7z (`37 7a bc af 27 1c`) or
  rar (`52 61 72 21 1a 07`) is an archive whatever its name. The job becomes
  `Unpacking` and korrid runs its own pure-Rust reader (`korrid
  unpack-archive <work-dir> <file>`) in the ADR 0012 sandbox unit
  `korri-acquire-<plugin short>-<provider name>.service`. The reader checks
  every entry before it writes one and refuses the whole archive for a link,
  device, FIFO or socket, an encrypted entry, an absolute path, a `..`, `.`
  or empty segment, a backslash or control character, a duplicate path, or
  more entries than discovery's traversal budget. It refuses a 7z or rar
  whose dictionary, including a compressed 7z header's, is larger than
  `MemAvailable`, and its unit runs with `MemoryMax=` set to `MemAvailable`
  at launch and `MemorySwapMax=0`. Files are written `0644`,
  folders `0755`, with the same 5% free-space floor. Multi-volume and
  encrypted archives and files the reader cannot read fail with "The
  downloaded files could not be unpacked." A nested archive gives no game and
  is "not a game for this system", as is a split name such as `.7z.001`
  (before the download) and a name that says archive with bytes that are not
  one.
- **Identification.** korrid runs discovery's identification over the work
  folder (a plain download sits there under its name) with the claim
  system's rules and `**/{title}.{ext}` after the system's layouts. The folder
  must give exactly one unit and no attention finding: one ROM, a cue with
  its tracks, an `.m3u` set or one CHD. Files with no system extension are
  ignored; a `README.md` beside a Mega Drive ROM is a second unit and fails.
  The identity is discovery's (ADR 0005): `Hash`, header excluded, `Parts` or
  `Chd`. It is identity, not authentication.
- **Ready folder.** korrid publishes
  `<storage-root>/cache/<plugin>/<provider>/<hex of the release key>/`
  (64 digits for `sha256:`, 40 for a CHD's `sha1:`) through a `.tmp-*`
  directory and an atomic rename, read-only. Ready means the folder exists.
  Identical content shares one folder.
- **Library record.** Once, under discovery's publication lock: the game
  (title from the claim), its release (`identity` `hash`, `parts` or `chd`
  as identified, `system` from the claim) and the provider link
  `provider-links."<providerId>/<claim id>" = { provider, playable, refs:
  [{ kind: provider-item-id, value: <claim id> }] }`. No location is saved.
  Identical bytes from another claim join the same game with a second link.
  When another game already holds the bytes, the link moves to that game;
  otherwise an existing link keeps its game. A copy that discovery finds in a source
  joins the same game by hash.
- **Route.** For a release with no saved location and no declared artifact,
  the resolver looks in the caches of the acquisition providers linked to the
  game for `<hex of the release key>/`. It runs discovery's unit selection
  over that folder without hashing (the same layouts, descriptor ownership
  and disc combination as publication) and the runner gets the one unit's
  primary file (its `.m3u`, `.cue`, first disc or file) as `contentPath`
  (`ResolvedRoute.cache_target`). No ready folder, no route: the
  release is unavailable, like a missing file.
- **Remove download.** `app.acquisition.remove { gameId }` also removes the
  game's download folders, unless another game's release names the same hash. The
  game, release and provider link stay as history. It refuses while the game
  runs (`ActiveSessionConflict`) and while a store job for the game works
  (`AcquisitionBusy`). Saves are never touched. A removed download comes back only
  through Get in the store, into the same game through the provider link.
- **Restart.** Startup stops a provider's unpack unit that is still active,
  then removes staging files and `.tmp-*` folders and logs
  one line per ready folder that no library release names (korrid stopped
  between publishing and recording). Nothing deletes such a folder; the next
  Get of that game records it.

### Native runners for specific content

A native runner can declare `releases` instead of `systems`. This is a
non-empty list of distinct `sha256:<64 lowercase hex characters>` strings.
Each string is an existing library release key. Discovery produces that key
from the whole file in `discovery/scanner.rs`; `config/catalog.rs` already uses
it in a game's `releases`. The user approved this runner field for recompilation
launchers such as Skate 3, which play only specific disc images.

Several entries mean any one accepted dump, not several required files. korrid
matches the recorded release key, resolves its file location, and offers the
runner beside any system-wide runners for the same content. A missing file
still fails with `LocalRomMissing`. `launch.prepare` receives that file through
the existing `contentPath`; no callback input or RPC schema changes.

The registry rejects empty lists, duplicate hashes, malformed hashes, provider
release keys, explicit `null`, and a runner with both `systems` and `releases`.
Only native runners can declare `releases`. Declaration and callback source
remain effect-free; plugins do not search or hash library files.

Hash matching does not scan unclaimed extensions, inspect archives, or rehash
a file at every launch. It uses the library's recorded identity. A source must
be paired with a system or group whose content rules match the input. A group resolves shared extensions by evidence, or asks the
person to resolve a clash. This field adds a launcher route to existing content;
`games` is the separate declaration for plugin-owned games.

The RetroArch callback preserves the existing Linux settings and argv. It
places savestates under `states/<full-runtime-id>/`. Raw configuration retains
legacy `overrides.config.prepend/append`, after Korri's lines; `replace` and
the approved reserved keys fail. Typed configuration cascade, stored route
choices, and chooser UI are not implemented by this slice.

## The sandbox has no I/O

A plugin gets only prepared source from its immutable snapshot, plus the
function-only timers described below. `require`, `process`, `fetch`,
`XMLHttpRequest` and `setInterval` remain undefined. Plugins declare effects;
korrid performs them. Neither source imports nor timers grant filesystem,
network or process access.

Runtime code generation is not available. After korrid compiles the graph and
before any plugin code runs, `eval` is removed and `Function` and its async,
generator and async generator forms throw `EvalError`; `Function.prototype`
and `instanceof Function` are unchanged. quickjs-ng 0.16.2's parser can crash
the process when an allocation fails, and a plugin can fill its heap before it
compiles. A route into the parser that is not on this list stays open.

### In-process module preparation

`eval_plugin_snapshot` and `call_plugin_operation_snapshot` prepare the same closed
graph with Oxc before creating a fresh QuickJS runtime. The single-file helpers
use that same path. Static relative imports and re-exports resolve only against
retained snapshot identities. Explicit `.js` and `.ts` paths are exact. Existing
extensionless TypeScript helper imports select `.ts`, not an ambient package or
index file. QuickJS owns module cycles, cache identity and live bindings.
True type-only imports do not request runtime source.

Preparation follows the emitted static module record and checks dependencies
before any plugin code runs. Native package metadata in the snapshot now selects
ESM, CommonJS and required JSON. Resolution uses only its root `node_modules`
set. Native export conditions retain metadata order: `browser`, the request's
`import` or `require` condition, and `default` are active. Exact and wildcard
exports, blocked paths, nested package types and the pinned browser file mappings
follow the original packages. There is no ambient ancestor search.

CommonJS receives a module-local `require` restricted to admitted literal edges.
All reachable sources are prepared and declared before execution. Evaluation
of a required module remains lazy. Repeated JSON requires share one parsed value;
CommonJS cycles share partial exports. Failed CommonJS entries leave the cache,
while successful dependencies remain. ESM identity remains native to QuickJS.
The evaluator owns all retained JS roots; loaders and callbacks hold weak
references so failures cannot retain the interpreter.

Dynamic imports, dynamic require expressions, `import.meta`, absolute requests,
snapshot escapes, ESM JSON imports and synthesized CommonJS named exports remain
unsupported. Require into top-level-await graphs or mixed require/ESM cycles
fails during preparation. Pure ESM and pure CommonJS cycles remain supported.

QuickJS aborts the process when an ES module that is already evaluating is
evaluated again. A guest callback stored on a global can reach that state, and
static dependency analysis cannot see such an edge. The host therefore marks the
static import graph it hands to the engine for the synchronous extent of that
evaluation, and a require into that marked set fails as an ordinary plugin error.
The engine exposes no module status, so this guard over-approximates: requiring
an ES module of the graph currently evaluating fails even when the engine already
finished that module. All pinned dependency graphs still load.
A caught ESM initializer exception still fails shared completion because QuickJS
also produces an internal unhandled rejection. The host does not suppress guest
rejections to hide that engine behavior.

The resolver closes before module evaluation. Preparation and evaluation never
reopen files. A filesystem snapshot still relies on the caller's existing
immutable-package authority. Installed-package admission still selects only
`plugin.ts`; no source-tree grant, manifest field or registry field is added.

Original pinned sources measured 2,698,372 prepared bytes across 102 modules for
`effect/Schema`, and 625,525 bytes across 49 modules for `whatwg-url`. The aggregate
prepared graph allowance is now 4 MiB. Per-file, input, stack, output and QuickJS
execution limits are unchanged. Larger graphs cost more host preparation storage
and VM compilation time; passing preparation does not guarantee VM acceptance.

This is not full Effect validation. Tests execute real Option, fast-check,
pure-rand, text codecs and lazy dependency JSON. Full Schema and URL graphs are
prepared, but their initialization and policy validation still need the shared
platform APIs. Installed source-tree registration and nested-policy integration
also remain unfinished.

### Shared completion and timers

Every evaluator consumer installs the same `setTimeout` and `clearTimeout`.
Callbacks must be functions. Zero delay is deferred, equal due times retain
registration order, and cancellation accepts numeric ID conversion. Negative or
non-finite delays become zero; delays above 2,147,483,647 ms become 1 ms. Fractional
delays truncate. There are no intervals, strings or browser background-timer
policies. Queued callbacks and arguments stay in the existing limited JS heap;
the research probe's 256-callback limit is not adopted.

The existing 1000 ms deadline starts before context and timer initialization.
It includes module evaluation, output extraction, promise jobs, timer waits,
callbacks and final serialization. Source preparation stays outside it under
the explicitly accepted stall/crash risk.

Module initialization must fulfill before output extraction. Promise jobs can
complete it, but stalled or timer-dependent top-level await fails without
invoking a handler. Declaration output is captured before initialization jobs
and timers drain. Initialization must finish before any handler runs. A
synchronous handler result is captured before its queued work drains. Later mutation cannot change
the captured result, but a later error invalidates it. Returned promises remain
unsupported.

At each exhausted promise-job checkpoint, remaining unhandled rejections fail
before another timer or success. A handler in that checkpoint can clear its
promise's rejection; a later timer cannot. The host tracks retained promise
identities without inspecting error payloads.

Timer state uses a private null-prototype array. Native cleanup truncates that
array while its context is alive, including on interruption. This releases
pending module resolvers without executing callbacks or more JS cleanup.
Rejection references are released before runtime destruction. No queued work
survives into another evaluation.

## Local announcement registry

korrid decodes declarations from installed packages and reads their committed
enabled selection. No plugin source is bundled into the daemon.

An enabled plugin can contribute providers, systems with content rules, groups,
declared games, families, runners, transports, contextual controls, and the closed
first-party discovery list. Disabled plugins contribute no rules, groups,
games or discoveries. Unsupported declaration fields and
explicit `null` values fail rather than disappearing. Contribution keys retain
the contributing plugin's identity; records retain strict IDs for route resolution.

### Content, groups and declared games

These are data exports, never callbacks. Content identification does not need an
enabled runner. Ordinary discovery scans paired sources, never plugin data
directories. ADR 0011 permits only the closed first-party provider-manifest
exception below; it does not permit build-tree scans.

```ts
export const systems = {
  nes: {
    id: "nes", title: "NES", aliases: ["famicom"],
    content: {
      extensions: ["nes"],
      layout: ["**/{title}.{ext}"],
      evidence: {
        bytes: [{ offset: 0, hex: "4e45531a" }],
        names: ["^.*\\(Unl\\)"],
      },
      header: [{ size: 16, bytes: { offset: 0, hex: "4e45531a" } }],
    },
  },
}
export const groups = {
  gameboy: { id: "gameboy", title: "Game Boy", systems: ["gb", "gbc"] },
}
```

`content` is optional. Its extensions are lowercase suffixes without a leading
dot. Compound suffixes such as `p8.png` are allowed. Matching ignores filename
case and chooses the longest matching suffix within one system. Archive suffixes
`zip`, `7z`, `rar`, `gz`, and `tar` are errors.

Layouts are ordered patterns over the path relative to the source. Tokens are
`{title}`, `{disc}`, `{version}`, `{region}`, and `{ext}`. `{title}` must occur
exactly once. `{ext}` matches one of that system's extensions. `*` matches within
one segment, and `**` occupies a whole segment and matches zero or more segments.
Unknown tokens, unmatched braces and trailing `/` folder layouts are errors.
The default layout is `**/{title}.{ext}` when no plugin supplies a layout.

Byte evidence and header byte rules must fit within the first 64 KiB. Hex is
non-empty, lowercase, even length, and at most 64 bytes. A header rule has `size`
and exactly one of `bytes` or `remainder`. For example,
`{ size: 512, remainder: { divisor: 1024, value: 512 } }` removes a copier header
when file size modulo 1024 equals 512. The divisor must be positive and the value
must be less than the divisor. The first applicable header rule wins before hashing.
Name evidence is a regex over the filename, not the path.

Enabled plugins sharing a system ID union extensions, evidence and header rules.
Layouts concatenate in plugin-ID order with duplicates removed. Titles and
header precedence follow that same order. Groups merge by ID, union their system
members, and drop members no enabled plugin declares. A group ID must not also
be a system ID. Group record IDs, like system record IDs, must equal their map keys.

A `games` export declares known games and release hashes (ADR 0003):

```ts
export const games = {
  jak1: {
    title: "Jak and Daxter: The Precursor Legacy",
    releases: {
      prepared: {
        hashes: ["sha256:…"], // replace with the release's full 64 hex digits
        file: "opengoal/jak1/out/jak1/iso/GAME.CGO",
      },
    },
  },
}
```

Game and release keys use legacy local playable-ID segments:
`[a-z0-9][a-z0-9._-]*`. The registry keys a game by `<plugin id>/<game key>` and
sets its provider identity to `{ provider: <plugin id>, ref: <game key> }`.
Each release has non-empty `hashes`, an optional `system`, optional `artifacts`
and an optional `file`. Hash hex digits accept either
case and are stored in lowercase. A release without a system uses
only runners whose `releases` accept its hash. No prepared-game system is invented.
`declared_hash` returns all enabled declared releases that name a hash.

ADR 0013 lets a release come from artifacts:

```ts
export const games = {
  rocknroll: {
    title: "Mega Man Rock N Roll",
    releases: {
      v1: {
        hashes: ["sha256:…"], // the content file, not the ZIP
        artifacts: ["@korri:rocknroll/content"],
        file: { artifact: "@korri:rocknroll/content", path: "data.pck" },
      },
    },
  },
}
```

`file` has one of three forms, and the form says where the path starts:

- A string is relative to an account root, without traversal (ADR 0003).
- `{ artifact, path }` names a file inside one ready artifact directory
  (ADR 0013).
- `{ packageFile, path }` names a key in the declaring plugin's own installed
  manifest `files`, joined with `path` (ADR 0016). It cannot name another
  plugin's files. The registry refuses a key missing from that package's
  `files`. It is ready whenever the plugin is enabled; no download or Confirm
  is needed.

For both object forms, `path` is relative: no leading `/`, no empty or `..`
segment and no control character. Unknown fields are refused. Rust decodes
`file` as an enum, so every reader handles all three forms. Discovery hashes
only the string form under account roots; it never reads an artifact or package
path as an account-root path. Package release hashes are content-file hashes,
proved by the pack's build check, not computed by korrid at runtime.

`artifacts` lists full artifact IDs, with the same rule as
`RunnerRecord.artifacts`: every ID must be a loaded, enabled artifact, which
may belong to another enabled plugin. Omission means none. `file.artifact` must
be one of them. A release with non-empty `artifacts` must use the artifact form
of `file`; an account-root string, package-file object or missing `file` fails.
`hashes` stay required and are hashes of the content file.

A plugin lists its preferred release first. Releases keep the module's
property order through QuickJS, JSON and the Rust registry. ECMAScript puts
array-index keys (`"1"`, `"10"`) before every other key, in numeric order, so a
release key that is a plain number does not keep its place.

Every reconcile pass creates or keeps the game of each enabled release whose
`file` is an object. It uses the existing declared-game records only: the
library item, a release whose identity is the first hash, and the provider link.
No location is saved: the route comes from the declaration and readiness from
the artifact cache or enabled installed package. A new release goes before the first release that the game declares
later, so declared releases keep declaration order. A release that is no longer
declared stays in the game's history and is unavailable. A copy of the content
file that discovery finds in a source joins the same game.

A release with no usable saved location that an enabled plugin declares with an
artifact `file` gets an artifact route. A saved `File` or `FileSet` location is
tried first. The runner matches through its `releases` hashes, so no system is
needed. `contentPath` is the artifact's ready directory joined with
`file.path`. `PluginLaunchInput.artifacts` still holds only the runner's
artifacts. Launch refuses with `ArtifactsNotReady` until the release's and the
runner's artifacts are all ready, and `app.acquisition.status { gameId }` lists
both, release artifacts first, without duplicates.

`controls` keeps the previous session-control record shape. Only its export name
changed; there is no alias for the old name.

Session controls are declaration-only and attach to one runner or transport
contribution owned by the same plugin. Their strict record contains a stable ID,
label and optional description, one of `command`, `toggle`, `choice`, or bounded
`range`, presentation flags, and one opaque allowlisted effect ID. Choice options
and range metadata are validated while the plugin is loaded. An effect is a
closed identifier such as `@korri:retroarch/open-menu` or an existing Moonlight
gameplay operation; it cannot contain a process, URL, socket address, or other
payload.

### Provider manifest discovery is a closed first-party vocabulary

ADR 0011 amends ADR 0003 narrowly. `discovery` is a data list of `{id, title}`
records. It requires a callable `handlers["discovery.discover"]`; declaration
evaluation never calls it. Both administrator admission and runtime admission
use `plugin_discovery.rs`'s closed provider-ID to account-relative-glob table:
`@korri:steam/installed-apps` → `steam/steamapps/appmanifest_*.acf`. Unknown
providers, foreign owners, duplicates, empty titles and missing handlers fail
admission. This is first-party integration, not an open plugin capability.
Third-party plugins cannot discover from their own data until another real case
needs it. No plugin may declare a new root or a broader rule.

For an enabled Steam plugin, korrid reads the manifests and calls
`discovery.discover` with `{files: [{relativePath, text}]}`, one file per call.
Paths are relative to the Steam home. There is no `readText` callback, filesystem
API or effect in plugin code. The sandbox and existing execution/input/output
limits still apply. Rust owns the input and observation types exported through
Typeshare. The `provider-ref-release` observation retains legacy `kind`,
`confidence`, `source`, `target`, `release` and `launch`; `source` needs only
`relativePath`, and optional legacy evidence is not carried in this slice.

korrid admits at most 4,096 manifests per pass and 65,536 bytes per manifest.
It skips files over those caps and reports daemon stderr and discovery
diagnostics. It uses descriptor-relative no-follow opens and regular-file
checks, never recursive scanning or symlink traversal. More than 10,000 entries
in one manifest directory, unreadable directories or invalid handler output
preserve previous locations. A missing directory is an empty installed set.
AppIDs must be numeric and match their manifest filenames, provider IDs must
belong to Steam, titles must be non-empty, and release, system, source and
runner must match the admitted Steam case. Steam's pure VDF parser admits only
StateFlags 4 and excludes explicit non-Game types. Titles come from `name`;
there is no invented fallback title.

The existing reconciliation writer publishes provider releases and locations.
Only locations it creates are owned in the existing `origins` journal. Missing,
uninstalled or disabled apps lose those locations; release history stays
unavailable. Authored locations stay authored. Startup and
`app.library.folders.rescan` use this same scan. No new persisted fields, configured
sources, automatic watcher, external Steam libraries or capability model are
added. ADR 0011 records the legacy citations, deliberate deltas and limits.

### Effects are a closed first-party vocabulary

This is a deliberate limit, not an oversight, and it is the one place where
plugins are not yet equal.

`SessionControlEffect` in `plugin.rs` is a closed Rust enum. Every effect name
it accepts is behaviour korrid itself implements, for RetroArch or for Moonlight.
A plugin declaring any other effect name fails to load. So a DIY runner — a
hand-written PPSSPP plugin, say — can declare **no session controls at all**. It
launches, it appears in discovery, it takes settings; it gets no in-game menu.

The family check that guards these effects denies a name, not a capability. A
runner earns an effect by declaring the family that owns it, and family
membership is self-asserted: any runner may write `family: "@korri:retroarch"`
and is then accepted with RetroArch effects, whatever program it actually runs.
korrid will aim RetroArch commands at that process and the commands will go
unanswered. The check stops a plugin from claiming a *foreign* effect id; it
cannot prove the process on the other end speaks the protocol.

The design in `docs/briefs/2026-09-15-plugin-model/OPERATIONS.md` answers this
with two plugin operations, `session.describe` and `session.control`, which
would let a runner advertise and serve its own controls. Neither is built. The
call convention they need now exists — a plugin exports `handlers` and the host
names the operation — so what remains is the part korrid performs: reaching a
running process over a protocol the host does not know. They stay unbuilt until
a real second case needs them, a DIY runner with controls worth reaching,
because inventing that seam ahead of the case is what this repository's guard
forbids.

Until then, say it plainly: **session controls are a first-party integration.**
Do not describe the current contract as equal capability for every plugin.

Registration and enablement still do not prove that a control can run. korrid
resolves enabled declarations only against the active route's ordered
contributions, the current platform, and live executor availability for that
exact session. Unrelated, disabled, unsupported-platform, and unavailable-
executor controls are omitted.

## Open questions

- **What a plugin may declare**, and how korrid matches declarations to device
  capabilities. This is the capability model, deliberately unbuilt (see
  `AGENTS.md`).
- **Installed source-tree registration.** The closed evaluator resolves admitted
  modules, but the current package producer registers only `plugin.ts`.

## Interpreter limits

Each TypeScript source is bounded to 128 KiB. Each JavaScript source is bounded
to 512 KiB. That per-module ceiling also applies to the JavaScript emitted from
TypeScript, so compact source cannot expand past it. The aggregate prepared graph
is bounded to 4 MiB. Both checks run after Oxc allocates; they bound retained and
executed code, not preparation time or host memory. Each QuickJS runtime
has a 16 MiB memory limit, a 1 MiB stack limit,
and a 1000 ms interrupt deadline. The existing empty I/O sandbox remains.
korrid's allocator enforces the memory limit, and reaching it ends the call
with `plugin exceeded its 16 MiB heap`. A plugin cannot catch an
out-of-memory error and recover: quickjs-ng 0.16.2 can drop a promise job
whose allocation fails, so the run would otherwise end without an error.
`tests/script_engine_limits.rs` drives korrid's call path at each limit.
`plugin-host/tests/declaration.rs` exercises nonterminating and memory-growing
source through the real evaluator. Launch inputs are bounded to 512 KiB.
Data conversion permits 8,192 nodes, 64 nesting levels, and 512 KiB of copied
strings across declaration and callback output. Each callback starts with a
fresh module instance. These limits govern interpreted code, not the separately
approved native daemon or the existing Linux session sandbox.
