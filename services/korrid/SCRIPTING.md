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
`systems`, `groups`, `games`, `families`, `runners`, `transports`, and
`controls`. Native launcher/runtime fields follow the instance/kind model
described below. `discovery` and `sessionControls` are unknown exports and fail
admission. `services` names native units packaged by `plugin.nix`. The Linux
host selects those units; the data registry validates the names but does not
activate them or change its existing records. `daemons` and the old `contributes`
wrapper are rejected. No TypeScript export encodes systemd directives.

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
(`docs/adr/0010-plugin-runtimes-download-into-the-shared-cache.md` records
these choices). A file route supplies
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

`runtime.resolve` asks the runner whether it can start this target now, and what
is missing if not. `launcher/runtime.rs` calls it before a launch starts, with
the same `PluginLaunchInput` that `launch.prepare` receives, because both
operations ask about the same runner and the same target. A runner whose runtime
is its own package declares no handler and proceeds; a runner that needs
something else refuses in its own words, and korrid spawns nothing. The refusal
reaches the person through the existing `LocalRouteUnavailable` failure.

`launcher/linux_plugin.rs` builds a
`korrid plugin-launch SOURCE INPUT_JSON COMPOSITION_JSON` command. The existing
systemd game unit starts this command as the runtime user, with its existing
sandbox. That process calls `launch.prepare`, then each enabled modifier's
`launch.compose`, before writing files atomically or applying environment/cwd.
In a private PID namespace it stays as PID 1 and supervises the composed command
as its child; otherwise it replaces itself with that command. It refuses root.
The daemon never writes callback-selected paths. Approval authorizes output;
runners and modifiers add no path or argument policy.

Native instances use the approved `{ id, kind, program }` fields. A runtime
uses `launcher` and its existing `path` field as a manifest file key. The
instance owns the program key, the runtime owns the core key, and the kind
owns callback source and auxiliary files. The kind's default instance points
to itself.

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
declared games, families, runners, transports, and contextual controls. Disabled
plugins contribute no rules, groups or games. Unsupported declaration fields and
explicit `null` values fail rather than disappearing. Contribution keys retain
the contributing plugin's identity; records retain strict IDs for route resolution.

### Content, groups and declared games

These are data exports, never callbacks. Content identification does not need an
enabled runner. Discovery scans paired sources, never plugin data directories.

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
Each release has non-empty `hashes`, an optional `system`, and an optional `file`
relative to an account root, without traversal. Hash hex digits accept either
case and are stored in lowercase. A release without a system uses
only runners whose `releases` accept its hash. No prepared-game system is invented.
`declared_hash` returns all enabled declared releases that name a hash.

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
has a 16 MiB memory limit, a 512 KiB stack limit,
and a 1000 ms interrupt deadline. The existing empty I/O sandbox remains.
`plugin-host/tests/declaration.rs` exercises nonterminating and memory-growing
source through the real evaluator. Launch inputs are bounded to 512 KiB.
Data conversion permits 8,192 nodes, 64 nesting levels, and 512 KiB of copied
strings across declaration and callback output. Each callback starts with a
fresh module instance. These limits govern interpreted code, not the separately
approved native daemon or the existing Linux session sandbox.
