---
title: Ship Effect validation inside the bounded plugin runtime
type: feat
status: active
date: 2026-09-11
origin: docs/briefs/2026-09-09-plugin-authoring-standard-brief.md
---

# Effect validation runtime

## Problem and scope

The real Effect policy decoder works in the QuickJS experiment. Production
still accepts one source file and supplies none of the required platform APIs.
Shipping the probe or changing one size constant would not connect the actual
checker to installed-plugin settings.

This plan proposes that connection. It does not authorize production changes,
VM runs or device operations until the runtime direction is approved. Physical
measurement and deployment retain their separate operational approval gates.

Binding inputs:

- The user chose A: Effect authors **and validates**. Do not substitute a
  generated schema or a handwritten validator.
- Production, tests and previews call the same shipped checker and runtime.
  Fixture selection changes inputs, not imports or implementations.
- Plugins ship as source and are prepared at runtime. No AOT plugin bundles,
  Bun device runtime, compatibility readers or test-only API providers.
- The original plugin brief still owns per-launcher identity, no kind fold,
  configuration precedence and instance-specific native support checks.

## Evidence and its limits

`docs/research/retroarch-effect-quickjs-probe/TIMERS.md` records all 14 real
policy fixtures passing in three rounds across three fresh runtimes for each
preparation path, at 16 MiB and 250 ms. No policy call scheduled a timer.
Dependencies nevertheless needed timer availability during initialization.

The measured minified prepared input totals 927,060 bytes. The source path totals
3,317,520 bytes. Both paths rewrite the policy's root `effect` import to the public
`effect/Schema` subpath in memory. The shipping policy still uses the root import.
The successful measurements therefore do not establish that unchanged graph's
fit. This proposal uses the public `effect/Schema` import explicitly in the
shipping policy source, without changing schema definitions or decoder logic.
Approval of this plan includes that import change. U1 must measure the actual
shipping graph; preparation and tests must not contain a hidden rewrite.

Preparation uses Bun outside the execution deadline and has not been implemented
in korrid. Heap snapshots are not peak memory. These x86_64 results do not
establish handheld performance or final source limits.

## Requirements

| ID | Required outcome | Grounding |
|---|---|---|
| R1 | Keep the actual Effect decoder and renderer as the policy path. | User choice A; `plugins/retroarch/policy.ts`, `plugins/retroarch/render-settings.ts`. |
| R2 | Use one shipped implementation in production, tests and previews. | User constraint; existing shared `services/korrid/src/script.rs` consumers. |
| R3 | Load only admitted immutable source, without JavaScript filesystem, network, process or package-install access. | Existing plugin isolation and package approval. |
| R4 | Bound preparation separately from execution, with truthful accounting and failures. | Probe preparation measurements; current transpilation precedes the VM deadline. |
| R5 | Specify and verify one text/URL/timer subset, including completion and rejection ordering. | Probe's documented conformance gaps. |
| R6 | Fold the selected launcher's policy, decode/render it, then check rendered native keys against the selected executable. | Existing cascade, legacy producers and `services/korrid/src/launcher/typed_settings.rs`. |
| R7 | Verify target performance before enabling the new path on a device. | Probe is x86_64 only; no target builds are allowed. |

## Proposed production boundary

The following flow is directional guidance for review, not implementation code:

**platform admission → bounded source preparation → fresh QuickJS runtime →
Effect decode/render → instance-source support check → existing launch effects**

### Source and preparation

Use an in-process Rust preparer, with no external Bun requirement on devices.
Feed the evaluator a closed source graph. Linux admission supplies approved
bytes; the evaluator never opens paths. Tests supply fixture bytes through that
same interface. Bundled Android source and publisher authority currently come
from Rust compiled composition in `services/korrid/src/plugin_policy.rs`.
Preserve that authority. Neither the Linux registry nor PackageManager defines
external Android plugin-source installation, which remains unresolved.

Preserve original npm resolution metadata and locked sources. Select explicit
package-export conditions based on the pinned artifacts; do not search ambient
ancestor `node_modules`. Support the static ESM and CJS interoperability required
by the actual policy and platform graphs, including their JSON dependency data.
The probe inventory includes `tr46/lib/mappingTable.json` and
`@exodus/bytes/fallback/multi-byte.encodings.json` inside locked dependencies.
Both are required by pinned CJS consumers. On first require, parse admitted JSON
with bounded strict JSON semantics and cache the resulting module value by
canonical identity. Repeated requires return that same value; lazy require
remains lazy without reopening files. General ESM JSON-import syntax is not
implied unless a real pinned edge needs it. A dependency-local CJS `require` may
resolve only admitted modules; it must not become a global host `require`.
Dynamic discovery, host builtins, native addons and network resolution fail.
Ordinary module cycles and filesystem symlink cycles need different treatment.

A Nix closure is not a source allowlist. Native binaries and configuration in
that closure must not become importable code. Resolve symlinks before assigning
identity, reject escapes, and require bounded regular-file reads. Bind all
selected source and resolution inputs to admission, then snapshot bytes once.
Aggregate byte, traversal, resolution-metadata and host-memory accounting starts
while admission constructs that snapshot, before each read or allocation. It
must not wait for a completed graph to reach the preparer.
No persisted preparation cache or new manifest fields are specified here.
The source-registration representation must be extracted from the real builder
and consumer before implementation; do not promote the probe's manifest format.

Keep preparation results in memory initially. Preparation must account for
aggregate input, module count, traversal, transformed output, peak host memory
and cancellation. A QuickJS interrupt does not stop an Oxc parse. Checking time
only between modules must not be presented as an interruptible preparation
budget. Resolving this is a release gate, not a deferred performance nicety.

### Common platform APIs

Install one implementation for every evaluator consumer. Preserve real codecs,
WHATWG URL parsing and the deadline-bound scheduler; do not add substitutes
that exist only to satisfy imports.

| API | Proposed bounded contract |
|---|---|
| TextEncoder | UTF-8. WebIDL string conversion, defaults, receiver checks and genuine bounded Uint8Array destinations. Constructor arguments do not select another encoding. |
| TextDecoder | UTF-8, Windows-1252 and UTF-16LE/BE with their standard aliases. Correct label/dictionary conversions, genuine BufferSource checks, streaming, BOM and fatal behavior. Unsupported labels throw; no silent UTF-8 fallback. |
| URL / URLSearchParams | The pinned WHATWG implementation and conversions. Parsing a `file:` URL grants no file access. No fetching or blob registration. |
| Timers | Function-only `setTimeout` and `clearTimeout`, using private monotonic elapsed time. Zero delay is deferred; equal deadlines retain registration order. No intervals or string evaluation. Preserve the documented delay/ID coercions rather than claim a browser event loop. |

Close the known text conversion gaps at the common constructor boundary, not
inside individual policies. Include supported shared, detached and resizable
buffer behavior explicitly. Capture scheduler-critical intrinsics before plugin
execution, not only the functions used for cleanup.

### Completion, output and lifetime

Proposed initial semantics keep synchronous plugin outputs:

1. Require fulfillment of the module evaluation promise before extracting any
   declaration or calling launch. Immediate or job-delayed rejection fails
   initialization and discards queued work. Rejection tracking does not replace
   checking that promise. If jobs are exhausted while initialization remains
   pending, reject it explicitly. Timer-dependent top-level await is unsupported;
   its queued work is discarded.
2. Capture declaration data through the existing strict JSON conversion. Drain
   initialization jobs/timers before invoking a launch or reporting admission.
3. Capture a synchronous callback result before draining its queued work.
   Returned promises remain unsupported. Later mutation cannot change captured
   output, but a later error invalidates the entire result.
4. At each exhausted promise-job checkpoint, fail on remaining unhandled
   rejections before another timer or success. A handler within that checkpoint
   can clear rejection tracking; a later timer is too late. Track identities,
   not just a global error flag.
5. Property inspection, coercion and error conversion are untrusted execution.
   They share the deadline; work scheduled during extraction enters the final
   drain. Do not emit partial output after failure.
6. Clear timers and rejection tracking while their context is alive on every
   exit. After interruption, only private non-user-code cleanup runs. Nothing
   survives disposal or leaks into the next evaluation.

Use 16 MiB / 250 ms as the initial measured execution gate, with the existing
stack and copied-output safeguards. Start the deadline before context/platform
initialization and do not reset it across phases. Host-side conversion also
checks elapsed time. This is not a hard real-time guarantee.

Do not adopt the probe's 256-entry timer bound or 927,060-byte artifact size as
production limits. Determine and document retention and preparation limits from
actual implementation measurements, including hostile inputs. Size safeguards
may change with evidence; no silent budget increase is permitted.

**Accepted cost if approved:** queued work can invalidate an otherwise valid
synchronous result. Some web APIs and timer-dependent initialization remain
unsupported. Preparing source each time costs CPU until measurements justify
reuse. Source preparation and ARM performance still require implementation work.

## Source-registration checkpoint before U1

First extract and document the source-registration and snapshot-authority
contract from `services/korrid/plugin-host/builder.nix`,
`services/korrid/plugin-host/src/package.rs`, `services/korrid/src/plugin.rs`,
`services/korrid/src/plugin_installation.rs` and
`services/korrid/src/plugin_policy.rs`. Establish how each actual consumer obtains
bounded, approved source and resolution inputs. Record the agreed contract in
`services/korrid/SCRIPTING.md` before implementing U1. If that extraction requires
an ungrounded persisted representation or new authority, request approval first.
U4 implements that contract; it does not defer its design until after U1–U3.

## Implementation units

### U1. Close and measure source preparation

**Requirements:** R3, R4. **Dependencies:** the approved source-registration checkpoint.

**Files:** `services/korrid/src/script.rs`, proposed private
`services/korrid/src/script/source.rs`, `services/korrid/tests/script_sources.rs`,
`services/korrid/Cargo.toml`, `services/korrid/plugin-host/Cargo.toml` and their
locks when an actual chosen dependency requires them.

Use the existing Oxc/QuickJS versions initially. Establish the closed byte-source
interface and investigate the Rust ESM/CJS preparation path against the exact
pinned graph. No ambient resolver or hidden rewrite of the policy's import.
Measure preparation separately before selecting its enforceable safeguards.

Test static imports/re-exports, package export conditions, ESM cycles, CJS partial
exports/cache identity, admitted JSON export/cache semantics, invalid/deep/oversized
JSON, missing modules, unsupported dynamic imports, builtins, symlink escape/loop,
oversized files, oversized aggregate admission snapshots and a pathological
single parse. Include JSON parsing and output allocations in preparation bounds. Prove no bytes outside the admitted set are requested. Do not continue to
production enablement if cancellation or host-memory bounds remain unsupported.

### U2. Make platform APIs one tested implementation

**Requirements:** R2, R5. **Dependencies:** U1's source interface.

**Files:** `services/korrid/src/script.rs`, proposed
`services/korrid/src/script/platform.rs`, `services/korrid/tests/script_platform.rs`;
actual source/lock packaging selected from the probe's pinned library evidence.

Preserve the real text/URL implementations. Correct conversion and branding at
the common public boundary. Test numeric/null/undefined/symbol input, throwing
coercion, surrogate replacement, short destinations, view offsets, invalid
lookalikes, shared/detached/resizable cases, every supported codec/alias,
unsupported labels, split BOM, incomplete flush, fatal errors and reuse. Retain
all URL cases and add setters, conversion errors and iterable malformed pairs.
Use standards evidence and differential controls, not Bun's known BOM bug.

### U3. Enforce completion and disposal

**Requirements:** R2, R4, R5. **Dependencies:** U1, U2.

**Files:** `services/korrid/src/script.rs`, proposed
`services/korrid/src/script/completion.rs`, `services/korrid/tests/script_completion.rs`.

Use the same scheduler for every call. Test non-inline zero delay, stable due
order, argument identity, cancellation, reentrancy, promise-before-timer order,
job-resolved initialization and explicit rejection of stalled/timer-dependent
initialization. Immediate and job-delayed rejected initialization must prevent
launch invocation and dispose queued timers. Cover same-checkpoint handled rejection, later-timer handlers,
rejected async timer callbacks, serialization-created work and captured-output
mutation. Exercise infinite jobs/callbacks, long waits, allocation failures,
prototype tampering, cleanup after interruption and fresh-runtime isolation.

### U4. Bind packaged source to existing authority

**Requirements:** R2, R3. **Dependencies:** U1 through U3.

**Files:** `services/korrid/plugin-host/builder.nix`,
`services/korrid/plugin-host/src/package.rs`, `services/korrid/src/plugin_installation.rs`,
`services/korrid/src/plugin.rs`, `services/korrid/plugin-host/tests/native_games.rs`,
`services/korrid/tests/installed_game_launch.rs`, `plugins/retroarch/plugin.nix`
and its package/lock files.

Implement the source-registration contract agreed before U1 without a second
dependency manifest. Ship original source, not the research bundles. Migrate
`PluginRegistry::from_installed` in `services/korrid/src/plugin.rs`: its independent
single-file read must not bypass bounded snapshot admission. Exercise packaged
Effect loading through this real installed-registry consumer, including missing
or changed helper source. Verify complete-source approval, source-only cross-output
authority, immutable snapshot reads and unavailable dependencies. Keep current
namespace/signature, revocation and native-service checks. Preserve Rust compiled
composition for bundled Android plugins; do not invent external installation
authority or silently apply Linux registry rules.

### U5. Connect the real nested policy path

**Requirements:** R1, R2, R6. **Dependencies:** U1 through U4.

**Files:** `plugins/retroarch/policy.ts`, `plugins/retroarch/render-settings.ts`,
`plugins/retroarch/plugin.ts`,
`services/korrid/src/config/cascade.rs`, `services/korrid/src/launcher/linux_plugin.rs`,
`services/korrid/src/launcher/typed_settings.rs`,
`services/korrid/src/launcher/plugin_launch.rs`, `services/korrid/tests/config_cascade.rs`,
`services/korrid/tests/typed_settings.rs`, `services/korrid/tests/installed_game_launch.rs`.

Ground the nested input in the actual legacy producers and preserve the selected
launcher-ID boundary. Keep nested-policy merging distinct from generic shallow
settings merging. Keep decoded policy and rendered scalar pairs distinct.
Effect validates; the existing native evidence checks rendered key/type support
for the selected executable. Neither replaces the other.

Retain all 14 real fixtures, complete accepted outputs and SchemaError identity
checks. ReferenceError or resource failure must never count as policy rejection.
Test per-ID precedence, no kind fold, unsupported-setting warnings, valid HTTPS,
reserved keys, invalid types and the actual final native configuration. Run the
same shipped checker through production, preview and test composition. Sanitize
operator diagnostics without replacing the actual decoder or leaking values.
Add sentinel-secret assertions across SchemaErrors, arbitrary throws/rejections,
serialization property names/object tags and source-preparation diagnostics.
Outward errors and logs must not contain the sentinel or raw payload. Internal
SchemaError classification and the original checker remain intact.

### U6. Verify portability before rollout

**Requirements:** R2, R4, R7. **Dependencies:** U1 through U5.

**Files:** `services/korrid/SCRIPTING.md`, `services/korrid/ROUTES.md`,
`docs/briefs/2026-09-09-plugin-authoring-standard-status.md`,
proposed `services/korrid/tests/script_sources.rs`,
`services/korrid/tests/script_platform.rs` and
`services/korrid/tests/script_completion.rs`.

Verify both korrid and the administrator host use the shipped runtime. Preserve
existing Android declaration behavior. After separate operational approval,
measure prebuilt ARM execution on the actual target: preparation, cold start,
validation, peak process memory, interruption and cleanup. A build-machine ARM
result is useful but not proof of handheld performance. Never compile on a
target. Focused development gates precede one final integrated VM gate; repeat
only for a demonstrated failure. Keep recovery SSH available during later
rollout, and do not mark the larger plugin brief complete prematurely.

## Decisions required before production changes

Approve or revise the proposed source-only runtime and common API/completion
subset. Effect choice A is already decided and is not being reopened.

Implementation must still establish the source-registration representation,
explicit package conditions, CJS mechanism, enforceable preparation safeguards,
retention limits and the callable policy integration contract from real
producers/consumers. No illustrative schema or field names settle those here.
Stop for approval if that work requires a new authority, unresolved schema,
unsupported resource guarantee or a broader API than this proposal.

## Out of scope

No generated-schema replacement, full browser/Node runtime, interval/string
callbacks, timer-dependent top-level await, persisted prepared-code cache,
automatic source installation, compatibility branch, device deployment or
publisher action is included in this design step. Native-service permissions,
SSH enrollment, graphical plugin management and general federation capability
modeling remain separate work.
