# Effect / QuickJS feasibility probe

**Current verified result: real deadline-bounded timers unblock both full policy paths at 16 MiB / 250 ms; policy initialization and decoding schedule zero timers.** All 14 fixtures pass in three rounds across three fresh runtimes per path.

See **[TIMERS.md](TIMERS.md)** for the current implementation, experimental 256-entry retention bound, conformance limits, timing and heap measurements, lower-cap failures, reproduction and recommendation. Final logs are in `logs/timers-final/` and `logs/timer-caps-final/`. This remains unshipped research; production is unchanged.

---

## Historical zero-timer continuation — 2026-09-10

The following report and its logs are preserved as the prior baseline. Its missing-timer blocker and next-step recommendation are superseded by TIMERS.md. The current `run.sh` adds real-timer trials and assertions; invoking the executable without the `timers` argument still reproduces the zero-timer path.

**Verified next blocker: `setTimeout is not defined` in fast-check.** Real
TextEncoder/TextDecoder and full WHATWG URL libraries now initialize. Both the
public `effect/Schema` graph and its minified bundle still fail before the actual
policy decoder becomes available, at **16 MiB / 250 ms / 512 KiB stack**.
No timer bindings, vendor patches, replacement decoder, production changes,
network/filesystem bindings, or shipped AOT artifacts were added.

## Selected sources

| Pinned source | Why selected / limits |
| --- | --- |
| [`@kayahr/text-encoding` 2.2.0](https://github.com/kayahr/text-encoding) | Maintained TypeScript/JS fork of the archived WHATWG text-encoding implementation; published 2026-08-28. Public `no-encodings` entry plus real Windows-1252, UTF-16LE and UTF-16BE codecs. Not a fully WebIDL-conformant native replacement; see differences below. |
| [`whatwg-url` 17.1.1](https://github.com/jsdom/whatwg-url) | Maintained full WHATWG URL implementation, published 2026-09-10. Includes real URLSearchParams, WebIDL conversions, UTS #46 and Punycode. Upstream declares URL-standard revision `9dc3827`. No partial URL-shaped substitute. |
| URL transitive sources | `@exodus/bytes` 1.15.1, `webidl-conversions` 8.0.1, `tr46` 6.0.0, `punycode` 2.3.1. |

Registry metadata and installed source/README files were inspected. Exact versions
and integrity hashes live in this directory's **probe-only `bun.lock`**, not the
plugin lock. `@types/whatwg-url` 13.0.0 is a probe-only typecheck dependency.
Upstream whatwg-url declares Node `^22.14.0 || >=24.0.0`; this experiment tests its
bundled source in QuickJS, not upstream-supported QuickJS compatibility.

The probe always installs the selected constructors, without native/fallback
branches in probe wiring. Text source initializes before URL source. Bun's
browser package conditions select @exodus/bytes implementations that require real
UTF-8, Latin-1 and UTF-16 decoders at module scope. Initial UTF-8-only and then
UTF-8/Latin-1 trials exposed those missing codecs; their logs remain in
`logs/platform/`. Adding the actual published codec modules resolved them.
No installed library source was edited.

The in-memory loader grants no I/O. The four constructors only transform data.
`setTimeout`, `setInterval`, `fetch`, `process` and `require` remain undefined.
URL parsing does not fetch URLs or read `file:` paths. Blob URL registration,
streams, timers and all untested APIs are outside this evidence.

## Results and cost

Final reproduction: `logs/final-platform/`. Each row is **one fresh runtime**, not
a timing distribution. Earlier exploratory results are retained separately;
the original 50-run experiment was **not repeated** ([historical report](BASELINE.md)).

| Path | QuickJS policy JS bytes | Platform JS bytes | Text / URL startup ms | Policy initialization until error ms | Heap snapshot after error, bytes | Outcome |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Public subpath, no policy bundling | 2,830,550 | 484,458 | 0.779 / 46.199 | 64.134 | 8,069,335 | Missing `setTimeout` |
| Public subpath + minified bundle | 376,213 | 484,458 | 0.762 / 41.669 | 44.827 | 6,014,069 | Missing `setTimeout` |
| Minified, **zero platform libraries** | 376,213 | 0 | — | 36.792 | 2,394,085 | Missing `TextEncoder` |

The minified policy plus the two platform modules total **860,671 JS bytes**
(376,213 + 7,678 + 476,780), before conformance code. The policy bundle alone fits
the shipped 512 KiB JS cap; the combined source does not. Production accepts
neither this prepared graph nor a package loader today. No sizing decision follows
from a policy-only byte count.

| Host preparation, ms | Public subpath | Subpath + minified |
| --- | ---: | ---: |
| Graph discovery/read/hash/TS AST parse | 459.971 | 387.230 |
| Experimental policy bundling | — | 36.254 |
| Experimental platform bundling/read/hash | 24.058 | 12.105 |
| Oxc policy TS transpilation | 0.843 | Not used |
| Rust manifest read/JSON parse | 6.752 | 1.738 |

Bun 1.3.11 is an **external runtime-preparation stand-in**, not korrid's runtime
preparer. It bundles CommonJS URL dependencies and optionally the policy. Nothing
is compiled ahead of time for shipment. Host preparation excludes Bun startup and
final manifest serialization/write. Native differential control costs another
1.535 / 1.774 ms, separately logged. The platform source hash manifest covers 68
loaded files. Prepared manifests and executable remain in `/tmp`.

Heap readings use `JS_ComputeMemoryUsage`: **snapshots, not peaks**. Context-only
snapshot: 83,378 bytes; after both platform libraries: 3,579,333 bytes. They exclude
Rust graphs, loader buffers and host preparation memory. Shared wall deadlines
include context construction, platform startup, policy startup and any calls.
Loader setup is outside the deadline. No caches were dropped. These are x86_64
build-machine observations, not device timings.

**Successful decoder startup, first decode, warm decode, full validation cost,
and lowest passing policy cap remain unknown.** The real 14-case harness retains
three decode/render/serialize rounds, but reaches **zero** calls. The separate
native Bun policy control passes **14/14**, including valid HTTPS, malformed
HTTPS, HTTP rejection, relative URL rejection and nullable URLs. No cap sweep was
run: more memory or time cannot supply a missing timer API.

## Primitive differential checks

`primitive-cases.ts` runs identical inputs against native Bun and the source
libraries in an independent QuickJS runtime. **48/51 match Bun; all 20 URL cases
match.** Platform initialization plus all checks finishes in 47.487 ms at 16 MiB /
250 ms; checks alone take 4.388 ms including their module initialization and result
serialization. Final heap snapshot: 3,649,485 bytes. This is not policy validation.

Coverage: UTF-8, non-BMP characters, lone surrogates, malformed/overlong/truncated
bytes, fatal decoding, BOM handling, streaming, decoder reuse, view offsets and
`encodeInto`. URL cases include IDNA, IPv4/IPv6, default ports, percent encoding,
dot segments, bases, credentials, malformed URLs, lone surrogates, opaque/file
URLs, duplicate query keys, live search params and URL statics.

| Differential case | Native Bun 1.3.11 | Source library / QuickJS | Interpretation |
| --- | --- | --- | --- |
| Split UTF-8 BOM across streaming calls | Keeps BOM | Removes BOM | Library agrees with WHATWG serialization and supplementary native Node 24.18.0; Bun discrepancy. |
| `TextEncoder.encode(123)` | UTF-8 bytes for `"123"` | Empty bytes | Library lacks required string coercion. |
| `TextDecoder.decode(123)` | Throws TypeError | Empty string | Library accepts invalid BufferSource. |

The [WHATWG serialization algorithm](https://encoding.spec.whatwg.org/#serialize-i/o-queue)
removes the first BOM when `ignoreBOM` is false. Node corroboration for these
three cases is retained in `node-control.json`. Parity is **not** full conformance:
no upstream WPT suite was run. The selected text entry registers only UTF-8,
Windows-1252 and UTF-16LE/BE. Other decoder labels fail. The library also supports
nonstandard encoder labels, unlike native TextEncoder. Do not promote this
experiment as a drop-in, all-API standards implementation.

## Boundary still unresolved

Source inspection locates the reached failure at fast-check 4.9.0's module-scope
`const safeSetTimeout = setTimeout;`, followed by `safeClearTimeout`.
`effect/Schema` directly imports `effect/testing/FastCheck`, which re-exports
fast-check. Its internal arbitrary module's FastCheck import is type-only.
The tested public-subpath minifier retains this initializer. No
claim is made that timer callbacks are needed by synchronous policy decoding:
initialization fails before that can be tested.

**Recommendation (judgment): parent review at this blocker.** Do not raise
production budgets or install fake timers. A real timer/runtime boundary would
add lifecycle, scheduling and cancellation costs; an approved dependency-graph
change would require new validation. Neither is implemented or approved here.
The text API gaps and combined source size are additional review costs, even if
the timer blocker is later removed.

## Reproduce on the build machine

```sh
export PATH=/nix/store/c6zis5smvx4lynpqzi3cwnmhc7xfhkhh-rust-default-1.97.1/bin:/nix/store/w88q44gqd1qg5wmkk7v0h97rpiqvam0l-gcc-wrapper-15.3.0/bin:$PATH
export LIBCLANG_PATH=/nix/store/f160pmgwc710rrc5c2vjgach0xfxa4nm-clang-21.1.8-lib/lib
export BINDGEN_EXTRA_CLANG_ARGS='-isystem /nix/store/qwi863qp79xkdm2yw3xzjh73dpg9qh67-glibc-2.42-84-dev/include'
bash docs/research/retroarch-effect-quickjs-probe/run.sh
```

Supply equivalent host tools elsewhere. The script creates fresh scratch/log
directories, installs both locks frozen, builds only the probe and runs three
policy initialization trials plus one independent primitive trial. It does not
interpret completion as a passing decoder. `PROBE_HEAP_MIB`, `PROBE_DEADLINE_MS`
and `PROBE_SAMPLES` are **probe-only** settings (defaults 16, 250, 1).
For an existing scratch build:

```sh
D=docs/research/retroarch-effect-quickjs-probe
S=/tmp/retroarch-effect-quickjs-probe
bun "$D/prepare.ts" schema-minify "$S/schema-minify-real.json" real
"$S/target/release/retroarch-effect-quickjs-probe" "$S/schema-minify-real.json" 16 250 1
"$S/target/release/retroarch-effect-quickjs-probe" "$S/schema-minify-real.json" 16 250 1 primitives
# Use `none` instead of `real` to reproduce the zero-platform baseline.
```

Verified: probe `cargo fmt --check`, release `cargo clippy --locked -- -D warnings`,
strict TypeScript check of all five probe TS files, `bash -n run.sh`, final log
assertions, plugin typecheck, and plugin tests (**9 pass, 4 native-parser tests
skipped, 0 fail**). Full WPT and a shell linter were not run. Only this research
directory changed. No commits, pushes, VM or device operations occurred.
