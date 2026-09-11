# Plugin scripting in korrid

korrid can transpile and run TypeScript or JavaScript **at runtime**, on every
device Korri targets. A plugin is a self-contained ES module with named exports;
korrid performs any resulting effects itself. Completion values, default exports,
`namespace` exports, and the old `contributes` wrapper are rejected.

Plugin source is evaluated at runtime rather than compiled into native code.
Bundled first-party source still ships through the normal korrid build. The
Linux administrator CLI in `plugin-host/` imports independently packaged
services and games. It uses this same interpreter and requires exact-package
approval before native execution. Linux discovery and launching read only the
committed enabled selection projection. They never invoke the administrator CLI.

## Source authority and snapshot checkpoint

This contract is extracted from the current producers and consumers. It does
not register the research probe's graph or authorize a Nix closure as code.

| Producer or consumer | Source authority and snapshot use |
|---|---|
| `plugin-host/builder.nix` | Trusted composition supplies `publisher` and `source`. The builder copies that **file** to the output's `plugin.ts`. It generates `manifest.json` for native artifacts. Today the only registered executable source is `plugin.ts`; `packages`, `files`, `services` and `requires` are not JavaScript source grants. |
| `plugin-host/src/package.rs` | The namespace comes from the generated manifest after verification against the device's bound full key. Nix's signed fingerprint binds the exact output path, `narHash`, `narSize` and sorted references. Reference membership alone does not register executable source. Admission snapshots the registered source before evaluating it. |
| `Report.approval` | The existing approval digest covers policy, package, provenance, declaration, effective unit, complete `plugin.ts` bytes and manifest bytes. Evaluation and source approval must consume the **same** snapshot, not reopen source for hashing. No new receipt field or preparation cache is needed for the current single-file producer. |
| `plugin-host/src/host.rs::publish_registry` and `plugin_installation.rs` | The owner-protected registry projects the existing Report fields `id`, `package`, `files`, `requires`. `package` selects the previously approved immutable output; `id` supplies the approved namespace. The reader does not verify a second signature or infer source grants from native files or required packages. |
| `PluginRegistry::from_installed` | Read only the registered `plugin.ts` through bounded snapshot admission, then evaluate those bytes and verify the resulting ID. Do not use an independent unbounded source read. The filesystem adapter relies on the administrator's immutable-output authority; a temporary directory in a test is not installation authority. |
| `plugin_policy.rs` | `include_str!` and explicit `@korri` composition own bundled Android source and namespace. Those bytes enter the same snapshot interface. Linux receipts and Android PackageManager do not authorize external Android plugin source. |
| `plugins/retroarch/package.json` and `bun.lock` | These are the existing npm resolution and locked-source records (`effect` is `4.0.0-beta.78`). The current builder does **not** ship them or the policy's helper/dependency source. They are not yet registered runtime resolution inputs. |

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

**Remaining registration gate:** full Effect loading needs an actual source-tree
producer through the existing builder `source` boundary. It must ship original
helper source, native npm metadata/lock and locked dependency source, and bind
all selected resolution decisions and bytes to the package's authority. The
current file-copy producer cannot prove that contract for an external source
output. Do not invent a dependency manifest or extend the registry to compensate.
Source-only cross-output registration, export conditions and full-graph approval
must be settled against that producer before enabling imports. The first slice
changes no persisted representation and grants no helper/dependency imports.

The existing 128 KiB source ceiling remains the first slice's aggregate content
ceiling, not a measured Effect graph limit. Bounded snapshot storage is not a
bound on Oxc's parse/transform allocations or cancellation. A wall-clock check
between parses cannot interrupt one parse. Hard preparation memory and cancellation guarantees are deferred by the user's
2026-09-11 decision in
`work/items/active/01M26Z12QCQ708KHWXSDK80GAC-effect-plugin-validation-runtime/work.md`.
Preparation stays inside korrid. A pathological parse can stall or crash the
process, even with approved source. Keep the source-admission checks; no helper
process, parser fork or hard preparation watchdog is implied. QuickJS stays at
16 MiB / 250 ms. This decision adds no platform APIs, timers, policy integration,
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
| Execute | `rquickjs` (QuickJS) | An interpreter, so Android's restrictions on generating executable code never come into play; also small |

Both cross-compile to `aarch64-linux-android` and `x86_64-linux`. The same
plugin file runs on every device: **source is portable where binaries are not.**

The first bundled production source is `plugins/android-app.plugin.ts`. It is
byte-for-byte pinned to the reviewed checkpoint copy under
`docs/research/android-app-plugin-schema-checkpoint/`, and parity is part of the
check suite so either copy changing alone fails. Bundled mGBA and RetroArch
sources come from their `android/plugin.ts` files. The Linux packages use their
parent `plugin.ts` files; no environment-based Linux implementation remains.

## Named exports and publisher identity

`script::eval_plugin_ts(source)` transpiles and evaluates a module. It returns
JSON containing its data exports, not the JavaScript completion value.
`name` must be a non-empty string. Optional identity exports are `title` and
`description`. Data export names come from the current producers: `providers`,
`systems`, `launchers`, `transports`, `runtimes`, `sessionControls`, and
`discovery`. Android record shapes are retained; native launcher/runtime fields
follow the instance/kind model described below. Discovery remains the shipped
`fileReleases` map, never a callback. `services` names native units packaged by
`plugin.nix`. The Linux host selects those units; the data registry validates
the names but does not activate them or change its existing records. `android`
remains reserved for its platform consumer. `daemons` and the old `contributes`
wrapper are rejected. No TypeScript export encodes systemd directives.

`plugin::load_plugin_source(namespace, source)` and
`decode_plugin_declaration(namespace, json)` receive publisher identity from
the caller. Bundled sources receive `@korri` explicitly in `plugin_policy.rs`.
Nothing infers it from a filename or trusts a namespace inside plugin source.
External daemon packages instead carry a generated `manifest.json` with
`publisher.namespace`. The host verifies the selected NAR against the device's
bound full public key before using that claim. See `plugin-host/README.md`.

An optional `launch` export must be a function. Evaluation checks but never
calls it. `script::call_plugin_launch_ts(source, input)` invokes it in a fresh
interpreter with bounded JSON input and output. Module evaluation and invocation
share one deadline, memory limit, and output budget. Launch must return JSON
synchronously; a promise, function, non-finite number, or undefined is rejected.
The evaluator performs no effects. `launcher/plugin_launch.rs` defines the
Rust/Typeshare `PluginLaunchInput` and `PluginLaunchOutput` treaties. Input
contains the selected full launcher/kind/runtime IDs, program and core file paths,
content path, existing account root, and kind-owned manifest files. Output
preserves legacy `command`, `args`, `env`, `envUnset`, and `cwd`, plus the
existing directory and provisioned-file declarations.

`launcher/linux_plugin.rs` builds a `korrid plugin-launch SOURCE INPUT_JSON`
command. The existing systemd game unit starts this command as the runtime
user, with its existing sandbox. That process calls the callback, writes the
files atomically, applies environment/cwd, and replaces itself with the
program. It refuses root. The daemon never writes callback-selected paths.
Approval authorizes output; the runner adds no path or argument policy.

Native instances use the approved `{ id, kind, program }` fields. A runtime
uses `launcher` and its existing `path` field as a manifest file key. The
instance owns the program key, the runtime owns the core key, and the kind
owns callback source and auxiliary files. The kind's default instance points
to itself. Android retains its `app` and absolute `path` records; these are
platform-specific declarations, not Linux fallback aliases.

The RetroArch callback preserves the existing Linux settings and argv. It
places savestates under `states/<full-runtime-id>/`. Raw configuration retains
legacy `overrides.config.prepend/append`, after Korri's lines; `replace` and
the approved reserved keys fail. Typed configuration cascade, stored route
choices, and chooser UI are not implemented by this slice.

The read-only registry probe requires an explicit publisher namespace for an
external source: `nix run .#korrid-plugin-review -- @publisher plugin.ts`.
With no arguments, it reviews the trusted bundled Android source.

## The sandbox has no I/O

A plugin gets only prepared source from its immutable snapshot, plus the
function-only timers described below. `require`, `process`, `fetch`,
`XMLHttpRequest` and `setInterval` remain undefined. Plugins declare effects;
korrid performs them. Neither source imports nor timers grant filesystem,
network or process access.

### In-process module preparation

`eval_plugin_snapshot` and `call_plugin_launch_snapshot` prepare the same closed
graph with Oxc before creating a fresh QuickJS runtime. The single-file helpers
use that same path. Static relative imports and re-exports resolve only against
retained snapshot identities. Explicit `.js` and `.ts` paths are exact. Existing
extensionless TypeScript helper imports select `.ts`, not an ambient package or
index file. QuickJS owns module cycles, cache identity and live bindings.
True type-only imports do not request runtime source.

Preparation follows the emitted static module record and checks dependencies
before any plugin code runs. Package imports, CommonJS, JSON modules, dynamic
imports, `import.meta`, absolute paths and snapshot escapes remain unsupported
in this slice. The resolver closes before module evaluation, so `eval` and
`Function` cannot use it for dynamic discovery. Preparation and evaluation never
reopen files. A filesystem snapshot still relies on the caller's existing
immutable-package authority.

The existing per-source and aggregate generated-JavaScript limits remain. This
is not full Effect loading: installed-package admission still selects only
`plugin.ts`, and the measured Effect graph exceeds the current graph allowance.
Native package source registration and dependency loading remain separate
implementation work. No new persisted source manifest or registry field exists.

### Shared completion and timers

Every evaluator consumer installs the same `setTimeout` and `clearTimeout`.
Callbacks must be functions. Zero delay is deferred, equal due times retain
registration order, and cancellation accepts numeric ID conversion. Negative or
non-finite delays become zero; delays above 2,147,483,647 ms become 1 ms. Fractional
delays truncate. There are no intervals, strings or browser background-timer
policies. Queued callbacks and arguments stay in the existing limited JS heap;
the research probe's 256-callback limit is not adopted.

The existing 250 ms deadline starts before context and timer initialization.
It includes module evaluation, output extraction, promise jobs, timer waits,
callbacks and final serialization. Source preparation stays outside it under
the explicitly accepted stall/crash risk.

Module initialization must fulfill before output extraction. Promise jobs can
complete it, but stalled or timer-dependent top-level await fails without
invoking launch. Declaration output is captured before initialization jobs and
timers drain. Initialization must finish before launch. A synchronous launch
result is captured before its queued work drains. Later mutation cannot change
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

korrid strictly decodes evaluated declarations into the narrow legacy plugin
seam proven by the Android application schema checkpoint. Bundled plugins are
registered from repository-owned source, and generic policy layers keyed by
plugin ID decide enablement. The built-in layer enables `@korri:android-app`,
`@korri:mgba`, and `@korri:retroarch` by default; a later layer can disable any
ID without changing registry code or adding an integration-specific switch. The
user policy layer is intentionally empty for these slices.

An enabled plugin can contribute provider, system, launcher, transport, runtime,
file-release discovery, and contextual session-control records. Disabled plugins
remain registered but contribute nothing while still reserving every declared
identity, including control identities. Discovery records are data-only extension
claims that name existing system, launcher, and runtime records. They never
receive filesystem handles or executable callbacks; the scanner performs
traversal and asks the enabled registry which normalized file extensions are
claimed. Unsupported declaration fields and explicit `null` values fail rather
than disappearing. As in legacy, contribution keys retain the contributing
plugin's identity; records retain strict schema IDs for route resolution.

Session controls are declaration-only and attach to one launcher, transport, or
runtime contribution owned by the same plugin. Their strict record contains a
stable ID, label and optional description, one of `command`, `toggle`, `choice`,
or bounded `range`, presentation flags, and one opaque allowlisted effect ID.
Choice options and range metadata are validated while the plugin is loaded. An
effect is a closed identifier such as `@korri:retroarch/open-menu` or an existing
Moonlight gameplay operation; it cannot contain a process, URL, Android intent,
socket address, Java method, or other payload.

Registration and enablement still do not prove that a control can run. korrid
resolves enabled declarations only against the active route's ordered
contributions, the current platform, and live executor availability for that
exact session. Unrelated, disabled, unsupported-platform, and unavailable-
executor controls are omitted. The Android launch/session seam will publish that
live context; until it does, the list and invoke RPCs return the tagged
`Unavailable` outcome rather than inferring a route from titles or game IDs.

Review that boundary without reading Rust:

```sh
nix run .#korrid-plugin-review
```

The report uses the bundled Android plugin source (kept byte-identical to the
checkpoint copy) and shows both its enabled and disabled states. This registry
is device-local only: it does not perform an Android launch or publish anything
to federation peers.

## Local route review

The checkpoint readable configuration now resolves through the production
snapshot loader, bundled policy, enabled registry, and narrow route resolver.
That resolver selects the one launchable release, follows `launch.use`, resolves
a `provider-ref` target into the legacy flattened target string, and joins the
provider/system/launcher declarations that are present after policy.

Review that boundary without Android effects:

```sh
nix run .#korrid-plugin-route-review
```

The enabled half reports the TMNT route owned by `@korri:android-app`; the
disabled half reports the same route as unavailable instead of falling through
to a process command or another launcher.

The companion RetroArch checkpoint in
`docs/research/retroarch-plugin-route/` resolves a file target through the
plugin-provided `@korri:retroarch/retroarch` launcher and
`@korri:mgba/mgba` runtime. `plugins/retroarch/android/` owns the launcher
artifact while `plugins/mgba/android/` owns the core build. The launcher APK
temporarily carries the core as an Android packaging bridge; plugin evaluation
itself still performs no I/O.

## Earlier script evaluator measured on hardware

These measurements predate the named-module cut. This slice has not been run
on Android hardware.

Tablet SM-X930 (Android 16, aarch64), running the example plugin: transpile
1.78 ms, evaluate 1.22 ms. Runtime transpilation is not a performance concern
at plugin size.

`nix run .#korrid-script-device -- <adb-serial>` re-runs that check any time. It uses the
standalone `script_probe` binary, so it verifies the arm64 path without adding
anything to the APK.

An earlier in-app proof (since removed, see "Not wired into the app yet") ran
the same plugin inside the Korri app process at 1.06 ms, and picked up an
edited plugin pushed to the running app with no rebuild and no reinstall.

## Cost, if the app carries it

Measured per APK entry, compressed:

| APK entry | without | with | delta |
|---|---|---|---|
| `lib/arm64-v8a/libkorrid.so` | 2,190,553 | 3,694,948 | +1,504,395 |
| `lib/arm64-v8a/liboxc_sourcemap-*.so` | — | 166,186 | +166,186 |
| **total** | | | **+1,670,581 (~1.59 MB)** |

Note that oxc emits a **second** native library beyond `libkorrid.so` — easy to
miss when estimating. Roughly: QuickJS ≈ 0.45 MB, oxc ≈ 1.1 MB compressed.
JavaScript-only plugins (no runtime TypeScript) would save about two thirds.

Do not compare whole-APK totals across different checkouts to derive this. A
baseline APK measured that way showed ~3.35 MB of unexplained slack, which made
the build *with* the runtime look smaller — nonsense. Per-entry comparison is
the trustworthy measure.

## Android app route source of truth

The Android plugin-backed route is wired into production. On each local-games
list or launch, korrid reloads the two fixed readable documents under the
existing local storage root (`config.yaml` and `library.yaml`), composes the
bundled default-enabled `@korri:android-app` plugin, resolves the route, signs
the unchanged `LaunchSpec`, and leaves installed-package/activity truth to the
Android shell's `PackageManager` edge.

The checkpoint files under `docs/research/android-app-plugin-schema-checkpoint/`
are review fixtures, not install defaults. Device proof copies those exact bytes
into the existing Android storage root before starting the brain; a fresh empty
root still initializes to empty readable documents and must not invent TMNT.

`command: android-app` is only the allowlisted integration token for this route.
It never falls through to generic process execution.

Review the installed surface with an explicit adb target and an already
installed TMNT package:

```sh
nix run .#android-app-route-check -- <adb-serial>
```

That gate verifies protected RPC shape/signature, launches the portal-selected
local game through the native bridge, asserts `com.playdigious.tmnt` from
Android's top-resumed activity field (`topResumedActivity` or Android 12
`mResumedActivity`), checks process evidence, verifies the embedded brain still
answers RPC while the game is foreground, and proves the measured
Home/task-switch relaunch/resume path. It does not install, uninstall, clear, or
otherwise mutate the game package.

## Traps, for the next person who touches the build

Fixed in `devshell.nix`, but they return on a new machine or a version bump.

1. **QuickJS ships no ready-made Rust bindings for Android.** They exist for
   common systems but not this target, so the build generates them — which
   needs `libclang` present in the dev shell.
2. **The Android build hands the phone's compiler to jobs meant for this
   computer.** `cargo-ndk` sets the C compiler globally, so host-side build
   steps get the phone's compiler and cannot find local system headers. Fixed
   by pinning `HOST_CC` / `CC_x86_64_unknown_linux_gnu`.
3. **The binding generator doesn't inherit the "building for Android"
   settings.** It runs libclang directly and needs the sysroot spelled out via
   `BINDGEN_EXTRA_CLANG_ARGS`. The Android-scoped spelling of that variable
   silently did nothing; the general one worked. The dashed form cannot be
   exported from bash at all.
4. **TypeScript `enum` panics the transpiler unless scoping is built with
   `with_enum_eval(true)`.** The panic message names the setting.
5. **Network adb targets drop between runs** — reconnect and `wait-for-device`
   before any device step, or a gate fails halfway through.

## Open questions

- **What a plugin may declare**, and how korrid matches declarations to device
  capabilities. This is the capability model, deliberately unbuilt (see
  `AGENTS.md`).
- **Imports between plugin files.** There is no module resolver; a plugin is
  currently a single self-contained file.

## Interpreter limits

TypeScript source is bounded to 128 KiB. Generated JavaScript is bounded to
512 KiB. Each QuickJS runtime has a 16 MiB memory limit, a 512 KiB stack limit,
and a 250 ms interrupt deadline. The existing empty I/O sandbox remains.
`plugin-host/tests/declaration.rs` exercises nonterminating and memory-growing
source through the real evaluator. Launch inputs are bounded to 512 KiB.
Data conversion permits 8,192 nodes, 64 nesting levels, and 512 KiB of copied
strings across declaration and callback output. Each callback starts with a
fresh module instance. These limits govern interpreted code, not the separately
approved native daemon or the existing Linux session sandbox.
