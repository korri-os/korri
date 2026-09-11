# Real timer continuation — 2026-09-10

**Verified: the full policy decoder and renderer run at 16 MiB / 250 ms with real timers available. They schedule zero timers.** Both source-subpath and minified paths pass all **14 fixtures × 3 rounds × 3 fresh runtimes**. This is feasibility evidence for an unshipped runtime experiment, not approval to change korrid.

Final logs: [`logs/timers-final/`](logs/timers-final/). Lower-cap observations: [`logs/timer-caps-final/`](logs/timer-caps-final/). The preceding zero-timer report remains in [README.md](README.md), with its original logs. Exploratory timer runs remain in `logs/timers-first/` and `logs/timer-caps/`; the final runs supersede their measurements.

## PROBE runtime contract, not production policy

- Each evaluation has one real timer queue. `setTimeout` stores a function, its arguments and a monotonic due time. `clearTimeout` removes the entry and releases its references. Zero-delay callbacks run on a later host-loop turn, never inline. The host sleeps until the next due time, capped by the remaining total deadline. It drains QuickJS promise jobs between callbacks. Callbacks can schedule and cancel timers.
- **Experimental retention bound: 256 pending callbacks per evaluation.** The 257th pending registration throws `RangeError`. This small fixed bound makes retention and cleanup testable. It is not an approved production limit. Callback arguments also consume the existing QuickJS heap cap. Cancellation and firing free queue slots; IDs are not reused during an evaluation.
- The same scheduler serves policy runs, text/URL checks and timer conformance. There is no alternate testing scheduler or test-specific scheduler branch. CLI modes choose inputs, not timer implementations.
- The total 250 ms deadline starts before context construction. It covers platform initialization, policy initialization, decoding and timer draining. A long wait, endless nested scheduling and an infinite-loop callback all terminate at this deadline. Private cleanup and accounting take place after interruption; observed final deadline runs finish in **250.023–250.201 ms**. This is not a hard real-time guarantee.
- JS owns all queued callback and argument references. Rust holds a controller with the context's lifetime, never persistent JS handles. Every evaluation exit closes and clears the queue before that controller, context and runtime are dropped. Cleanup captures the native Map clear/size operations so prototype replacement cannot redirect cleanup. No timer thread, global host queue or callback survives runtime disposal.
- The sole added host input is elapsed monotonic time, held in the private scheduler closure. No arbitrary reads, filesystem, network, Node process or module bindings are granted. `setInterval`, `fetch`, `process` and `require` remain undefined. URL parsing performs no I/O.

**Limits of this experiment:** function callbacks only; string callbacks throw `TypeError`. Delays are numerically coerced and truncated; negative/non-finite delays become zero and delays above 2,147,483,647 ms become 1 ms. There is no browser nested-timer minimum, background throttling, interval support or full HTML event loop. Callback exceptions stop the evaluation. Module initialization still uses the existing synchronous promise completion path; timer-dependent top-level await and unhandled-rejection policy were not validated. Do not treat this scheduler as a complete browser timer implementation or a shipment proposal.

## Policy results and timings

The real `decodeRetroArchPolicy` and `renderRetroArchSettings` bodies are unchanged. The source path uses the public `effect/Schema` import experiment. The minified path bundles that same graph. Both load the same pinned pure text and WHATWG URL sources documented in README. No vendor patches, generated schemas, replacement decoder, production APIs or production limits changed.

Every negative fixture must produce **`Schema.isSchemaError(error)`**, not just any exception. The log checker also compares each error message with native Bun. A missing-API `ReferenceError` cannot count as a passing negative fixture. Positive outputs are compared in full with native Bun, including renderer defaults. All ten rejected and four accepted fixtures run in all three rounds. Valid HTTPS, malformed HTTPS, HTTP rejection, relative URL rejection and null URLs remain covered. The actual URL validator catches constructor exceptions; successful HTTPS fixtures are therefore essential in addition to error classification.

All three timer counters — after platform initialization, after policy initialization and after decoding — report **scheduled = 0, fired = 0, pending = 0**. `setTimeout` and `clearTimeout` are functions. Fast-check needs their availability during the observed import, not callback execution in these policy calls.

Three fresh QuickJS runtimes per row; numbers are **median [minimum–maximum]**. Warm timing uses the same `nested-video-audio` input in rounds 1 and 2 (six calls), not a mix of differently sized fixtures.

| Path / heap cap | Policy module init, ms | Cold ready including platform, ms | First decode/render/serialize, µs | Warm same-input call, µs | Init + all 42 calls + GC, ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Source subpath / 16 MiB | 63.593 [61.848–67.394] | 102.035 [101.196–107.475] | 3,666 [3,538–3,823] | 279 [266–298] | 117.602 [116.658–124.706] |
| Minified / 16 MiB | 75.553 [68.987–78.432] | 119.695 [111.438–120.147] | 987 [746–990] | 311 [278–322] | 134.676 [124.576–134.829] |
| Minified / 14 MiB | 61.762 [57.914–65.953] | 98.417 [94.544–102.602] | 793 [644–846] | 269 [242–294] | 110.244 [105.695–114.322] |

At 16 MiB, real timer installation takes 0.165–0.202 ms. Text initialization takes 0.631–0.736 ms and URL initialization 35.805–41.414 ms. First 14-fixture rounds take 5.716–6.162 ms for source and 3.036–3.659 ms for minified; subsequent rounds take 2.068–2.323 and 2.206–2.596 ms respectively. Individual call measurements include error conversion and JSON serialization, but exclude host parsing and timer draining. Full evaluation times include both.

These are x86_64 build-machine timings, not device timings. Trials were not interleaved; OS/file caches were not dropped. Three samples do not establish a reliable performance distribution. In particular, the lower-cap trial being faster is **not evidence that reducing memory improves speed**.

## Memory and source cost

`JS_ComputeMemoryUsage` reports snapshots, **not peaks**. Cap trials use the same executable and prepared graph as final baseline trials.

| Path | After policy init, bytes | After 42 calls, bytes | After GC, bytes | 12 MiB | 14 MiB | 16 MiB |
| --- | ---: | ---: | ---: | --- | --- | --- |
| Source subpath | 9,852,999 | 10,157,877–10,161,115 | 9,926,264 | 0/3 pass | 0/3 pass | 3/3 pass |
| Minified | 8,244,397 | 8,571,513 | 8,327,430 | 0/3 pass | 3/3 pass | 3/3 pass |

**Lowest tested passing caps: source 16 MiB; minified 14 MiB.** These are not exact minima. We first tried 12 MiB because snapshots were about 8–10 MB, then 14 MiB to narrow the interval after both paths failed. We stopped there. No broad sweep and no upward budget change was needed. The failed runs stop during initialization with QuickJS's null exception (`Exception generated by QuickJS: Null"null"`), before any decode. A cap-dependent allocation failure is **inferred**, not a diagnostic emitted by this engine. All failed runs still clear their queues. A heap snapshot is not a safe sizing estimate for parse/compile transients.

Context-only snapshot: 83,378 bytes. With the scheduler installed: 91,055 bytes. After both platform libraries: 3,586,512 bytes. These exclude Rust graphs, loader buffers, host preparation memory and allocator overhead outside QuickJS accounting.

| Prepared input | Source-subpath bytes | Minified bytes |
| --- | ---: | ---: |
| Policy + measurement wrapper | 2,830,659 | 440,199 |
| Text + URL source | 484,458 | 484,458 |
| Real timer scheduler | 2,403 | 2,403 |
| Combined, excluding primitive fixtures | 3,317,520 | 927,060 |

The stronger harness imports `isSchemaError`; the resulting minified graph is larger than the historical 376,213-byte wrapper graph. Compare the recorded graphs, not those byte counts as identical inputs. The policy bundle still fits 512 KiB; combined source does not. Production has neither this package loader nor this externally prepared graph. Bun 1.3.11 remains an external preparation stand-in, not korrid's runtime preparer.

Host graph preparation takes 306.684 ms (source) / 330.868 ms (minified), plus 49.228 ms policy bundling on the minified path. Platform preparation takes 14.291 / 11.943 ms. Rust manifest read/parse takes 5.625 / 1.704 ms; Oxc policy transpilation takes 0.629 / 0 ms. Bun startup and final manifest write are excluded. These costs are separate from the 250 ms evaluation deadline; no prepared artifact is proposed for shipment.

## Conformance and validation

| Check | Verified result |
| --- | --- |
| Timer primitives, 3 runtimes | Non-inline zero delay, real 12 ms delay, argument identity/value, callback receiver, cancellation by string/numeric ID, repeated cancellation, cancellation inside another callback, negative delay, nested scheduling and promise-before-next-timer ordering pass. |
| Retention, 3 runtimes | 256 pending registrations succeed; the next throws. Clearing all 256 releases slots. Final statistics: 262 scheduled, 258 cancelled, 4 fired, zero pending. |
| Deadline, 3 runtimes each | A 10-second timer does not fire; endless nested zero-delay timers stop; an infinite callback is interrupted. Each queue is empty after cleanup. |
| Error/lifetime, 3 runtimes each | A throwing callback stops draining and discards the remaining timer. Early evaluation exit discards a timer without firing it. Subsequent runtimes start with fresh state. |
| Text/URL, separate runtime using the same scheduler | **48/51** differential matches; all **20 URL cases** match. Zero timers scheduled. Checks take 3.981 ms; platform + checks + cleanup takes 44.668 ms. |
| Native Bun policy control | 14/14 pass; all QuickJS accepted outputs and rejected messages match. |
| Static checks | Rust fmt check and release Clippy with `-D warnings`; strict TypeScript on all five probe TS files; shell syntax; unchanged engine-lock check. |
| Plugin regression | Typecheck passes; 9 tests pass, 4 native-parser tests skip, zero fail. |

The text gaps are unchanged: numeric TextEncoder input lacks string coercion, and invalid numeric TextDecoder input is accepted instead of throwing. The split streaming BOM differs from Bun but matches the WHATWG serialization algorithm and the retained Node control. Only UTF-8, Windows-1252 and UTF-16LE/BE codecs are registered. Other labels, nonstandard encoder labels and full WebIDL behavior remain gaps. No upstream WPT suite, full browser timer suite, shell linter, VM or device check was run.

## Reproduce

Use the build-machine tool environment shown in README, then:

```sh
D=docs/research/retroarch-effect-quickjs-probe
# run.sh retains its Nix shebang; use a fresh output directory.
PROBE_SAMPLES=3 PROBE_LOGS=/tmp/retroarch-timer-reproduction bash "$D/run.sh"
python3 "$D/check-results.py" /tmp/retroarch-timer-reproduction 3
```

`run.sh` installs frozen locks, checks Cargo dependency identity with the separate unchanged `check-cargo-lock.py`, builds the probe, and runs zero-platform, zero-timer, real-timer policy, primitive and lifetime cases. It asserts the full successful baseline. Defaults remain 16 MiB / 250 ms / one sample; `PROBE_SAMPLES=3` requests the measured repetition. `PROBE_HEAP_MIB` and `PROBE_DEADLINE_MS` affect this probe only.

Using the scratch path printed by the script:

```sh
S=/tmp/retroarch-effect-quickjs-probe  # replace with printed scratch path
P="$S/target/release/retroarch-effect-quickjs-probe"
# Availability absent: reproduces the verified fast-check capture failure.
"$P" "$S/schema-minify-real.json" 16 250 1
# Availability real; same scheduler for every mode.
"$P" "$S/schema-minify-real.json" 16 250 3 policy timers
"$P" "$S/schema-minify-real.json" 16 250 1 primitives timers
"$P" "$S/schema-minify-real.json" 16 250 3 timer-checks timers
# Focused cap observations, not an automatic sweep:
"$P" "$S/schema-minify-real.json" 12 250 3 policy timers
"$P" "$S/schema-minify-real.json" 14 250 3 policy timers
"$P" "$S/schema-real.json" 12 250 3 policy timers
"$P" "$S/schema-real.json" 14 250 3 policy timers
```

The executable emits measurements even for expected failing trials; process exit alone is not policy success. `check-results.py` requires the requested positive sample count, rejects missing/duplicate sample identities, and asserts the baseline's full 14-case outputs, primitive differences and timer lifecycle cases. Its regression tests mutate copies of the recorded evidence; the original measurements remain unchanged. Raw JSONL files retain every timing, cap, error, fixture result and counter.

**Recommendation (judgment): A is feasible for these synchronous policy fixtures; do not expand production budgets or ship this probe.** The next decision is the real runtime preparation/platform boundary, not another missing API for these calls. Its costs include source preparation, 927,060 combined JS bytes on the measured minified route, incomplete text/timer API conformance and unmeasured device performance. A production direction must resolve those costs explicitly. No commits, pushes, device operations or delegation occurred.

## Sample-count verification follow-up — 2026-09-10

The checker now requires a positive expected count and exactly sequential sample IDs. `run.sh` passes that count; both the script and the rebuilt Rust CLI reject zero samples with exit code 2. Seven checker regression tests pass, including missing, truncated, duplicate, reordered and extra samples, zero expected count, and the complete recorded trials.

The existing `logs/timers-final/` passes the checker with expected count 3. One fresh minified-policy runtime from the latest source passes all 14 fixtures × 3 rounds at 16 MiB / 250 ms, with exact native outputs, zero scheduled timers and an empty cleanup queue. The cached scratch crate was rebuilt offline; the benchmark matrix and cap trials were not rerun. Historical measurements and log files are unchanged.

Rust fmt, release Clippy (`-D warnings`), Ruff format/lint on all three Python scripts, strict TypeScript on all five probe sources, ShellCheck 0.11.0 (`-s bash`), shell syntax, and the engine-lock check pass. Reproduction scripts retain their Nix shebangs and contain no inline Python heredocs. Recheck the recorded evidence with:

```sh
D=docs/research/retroarch-effect-quickjs-probe
python3 "$D/test_check_results.py" -v
python3 "$D/check-results.py" "$D/logs/timers-final" 3
```
