# Plugin scripting in korrid

korrid can transpile and run TypeScript or JavaScript **at runtime**, on every
device Korri targets. A plugin is a self-contained ES module with named exports;
korrid performs any resulting effects itself. Completion values, default exports,
`namespace` exports, and the old `contributes` wrapper are rejected.

Plugin source is evaluated at runtime rather than compiled into native code.
Bundled first-party source still ships through the normal korrid build. The
Linux administrator CLI in `plugin-host/` now imports independently packaged
external daemon declarations. It uses this same interpreter and requires
exact-package approval before native execution. This does not add daemon
contributions to the game-route registry below.

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
check suite so either copy changing alone fails. The mGBA production source is
shared by the repository plugin path and the bundled korrid path so discovery
and runtime declarations cannot drift.

## Named exports and publisher identity

`script::eval_plugin_ts(source)` transpiles and evaluates a module. It returns
JSON containing its data exports, not the JavaScript completion value.
`name` must be a non-empty string. Optional identity exports are `title` and
`description`. Data export names come from the current producers: `providers`,
`systems`, `launchers`, `transports`, `runtimes`, `sessionControls`, and
`discovery`. Their record shapes are unchanged. Discovery remains the shipped
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
The API performs no effects and defines no launch-spec schema. The generic
Linux runner and its approved source-driven effects remain a later slice.

The read-only registry probe requires an explicit publisher namespace for an
external source: `nix run .#korrid-plugin-review -- @publisher plugin.ts`.
With no arguments, it reviews the trusted bundled Android source.

## The sandbox is empty on purpose

A plugin gets no module loader, no host bindings, and no I/O. `require`,
`process`, `fetch`, and `XMLHttpRequest` are all `undefined` — asserted in
tests. The rule "plugins declare, korrid performs effects" is therefore
enforced by construction rather than by convention, and a plugin cannot pin
itself to one machine by reaching for local resources.

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
