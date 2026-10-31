# THROWAWAY: executable surface conformance and cost experiment

This is a spike, not a Korri package schema, trust mechanism or deployment.
The real surface packages and their visual design are unchanged.

## What runs

```sh
cd clients/surface-shell-spike
cargo build --release
./target/release/surface-verify verify surfaces/zone fixtures/model.json
./target/release/surface-verify verify surfaces/pico fixtures/model.json
```

`verify` compiles `main.slint` with Slint interpreter **1.18.1**, records every
compiler diagnostic, finds the exported `Surface`, and enumerates its public
properties. The pinned interpreter's internal metadata preserves `int` versus
`float`, nested struct fields and array element types. No window is created.

It evaluates `logic.js` in a fresh QuickJS context. It calls `init`, then
`view`, then `update` and `view` for each of the eight CONTRACT.md actions in
contract order. It follows with 500 xorshift32 actions from seed `0x00c0ffee`.
There are **509 checked views and 508 updates** per passing package. Each call,
including source evaluation and JSON serialization, has a **250 ms wall-time
budget**, checked by `Runtime::set_interrupt_handler` and after return. The
QuickJS heap has a **64 MiB hard limit**. The runner calls
`Runtime::set_memory_limit` and also caps its measured backing allocator.

Every view must serialize to a JSON object. Every supplied key must name a
public property after the shell's camelCase-to-kebab conversion, including
null keys. Struct fields and array items are checked recursively. Unknown
struct fields fail. Null and omitted fields retain their value, as CONTRACT.md
specifies. Numbers follow the shell's declared int/float conversion rules.
Every image must be a sole `$image` string object for a declared image value,
and the shell's real decoder must decode it. Image results are cached only
inside this package's verification run. Exceptions, allocation failures and
time budget failures fail the package.

The backing allocator measures the actual high-water mark, including buffers
freed before a call returns. It delegates pointer layout and allocation to
rquickjs's `RustAllocator`; its private allocation headers are not counted.
This is not a post-call heap sample and not total process RSS.

The random action sequence is deterministic. `TZ=UTC` fixes timestamp
formatting; `Date.now` and `Math.random` throw. This is a bounded observation,
not proof about every model or action sequence, purity, security, visual
quality or a surface's capabilities. Untrusted Slint imports and pathological
native builtins still require a process sandbox in any product design.
No signature verification or permission checks are removed or replaced.

## Derived summary (spike-only)

The filename **`.spike-verification.json`** and **all its fields are spike-only**.
The verifier generates the file next to the package, including on conformance
failure. Never hand-edit it. It is excluded from its own package digest.

| Field | Derived value |
| --- | --- |
| `spike_only`, `verifier_version` | Explicit throwaway marker and verifier revision |
| `package_sha256` | All package file paths and bytes, sorted, length-prefixed; not the summary |
| `fixture_sha256` | Exact input model bytes |
| `slint_version` | Pinned interpreter version |
| `diagnostics` | Compiler messages |
| `properties` | Public property names and recursive declared types |
| `sweep` | Fixed seed, random action count and completed checked views |
| `memory_limit_bytes`, `call_budget_ms` | Limits used by this run |
| `max_call_ms` | Slowest QuickJS call, including load and JSON serialization |
| `peak_quickjs_backing_bytes` | High-water live QuickJS backing allocation payload bytes |
| `pass`, `reasons` | Conformance result and explicit failure reasons |

CLI stdout adds `worker_hwm_kib` for the cost harness. That process measurement
is not written into the package's derived file. The summary is not signed and
is not an authority. Binding an authenticated package to a trusted verifier's
output is separate work.

| When | Work | Cost / limitation |
| --- | --- | --- |
| Install or update | Authenticate through the existing package path; hash content once; compile and run the bounded sweep; derive the summary | Compilation and 1,018 init/update/view calls are deliberately not startup work |
| Startup | Enumerate installed ids and read summaries; optionally compare cached size/mtime identity, or an immutable Nix store path | Metadata is not tamper detection; preserved sizes/mtimes can hide edits |
| Switch | Compile and create only the selected package; load its fresh context | The current shell still compiles at switch time; this prototype does not change its loader |

Do not fully hash all installed packages at every startup to rediscover the
same immutable facts. For mutable folders, reverify changed identities; a
security-sensitive mutable folder still needs authenticated content checking.
The existing shell loader is unchanged and does not read these summaries yet.
The CLI demonstrates the install/startup costs, not a wired product lifecycle.
There is no new persisted product index in this experiment.

## Reproduce measurements

```sh
# Workstation. Physical file copies by default, all generated data in temp dirs.
verify/workstation.sh ./target/release/surface-verify verify/evidence/workstation
# Each method also runs alone in a fresh process:
./target/release/surface-verify many "$PWD" 100
# Optional storage-sharing experiment, not used for the reported measurements:
./target/release/surface-verify many "$PWD" 100 --hardlinks
# Generate fixture game lists with shared PNG references, then measure QuickJS:
./target/release/surface-verify large "$PWD" /tmp/surface-large-fixtures
# X11 runtime libraries, Xvfb and ImageMagick must be available:
verify/xvfb.sh ./target/release/surface-shell-spike /tmp/surface-large-fixtures /tmp/surface-large-xvfb 50000
```

Each N alternates Zone and Pico under a distinct id directory. Both complete
real package trees are copied, including preview assets. Hashes include relative
paths, their lengths, content lengths and every payload byte. `identity` walks
the same directories but reads only file sizes and mtimes. `summaries` reads
and JSON-parses every derived summary, then discards it. This measures a
streaming scan, not retention of N full parsed records in a shell index.
Every phase runs in a fresh process. The common installed-id enumeration
precedes each phase timer; recursive package directory walks are timed.
Setup/copying is excluded from the phase clocks. Caches are not flushed.
These are normal-cache runs; copying does not guarantee cache residency.
They are not cold-storage or cache-residency guarantees. Each case is measured
once; there are no confidence intervals.

Full verification really recompiles and sweeps every copy, without digest
memoization. Each package gets its own process as well as its own QuickJS
context. This also bounds Slint's process-global font registry. Set
`SURFACE_VERIFY_JOBS` to select concurrency: the workstation harness uses 8,
the SP harness uses 4. Reported per-item time is total elapsed / N, not
single-package latency. RSS is from `/proc/self/status`; worker HWM is the
largest actual worker RSS high-water mark. The emitted worker aggregate
upper bound is worker HWM × concurrency, not a sampled aggregate RSS.

The large-list generator cycles the actual fixture games, making ids and
titles unique. It extracts their eight distinct covers into PNG files and
reuses file paths, rather than duplicating data URLs in every game. Each
package/list gets a fresh process and a **128 MiB** QuickJS heap, with a
**30 second per-call** budget, so the SP run cannot take an unbounded heap
away from the still-running kiosk. Reported `view` includes JSON serialization;
Rust JSON parsing is outside that call time. Absolute cover-reference paths
differ between the two machines, so JSON byte counts are reported separately;
the tables are not a CPU-only architecture comparison. The shell harness sends one `right` action through the shell's existing control
FIFO at sample 12; before/after PNGs show that selection changed. This also
starts Pico's focus animation. The shell run uses a 4 GiB virtual
address-space cap, to stop an unbounded surface from exhausting the workstation.

## Workstation results

Verified on the workstation's Intel Core i9-11900H (8 cores / 16 threads),
31 GiB RAM, Linux x86_64. Full verification uses eight workers. All 6,110
package verifications passed. Zone has 35 payload files / 2,530,639 bytes;
Pico has 19 / 333,604 bytes. The 5,000-package case hashes 135,000 payload
files / 7,160,607,500 bytes, versus about 10 MiB of summaries.

### workstation: installed packages

| N | Work | Total s | ms/item | Phase RSS HWM KiB | Worker RSS HWM KiB |
| ---: | --- | ---: | ---: | ---: | ---: |
| 10 | sha256 | 0.0704 | 7.037 | 5,060 | — |
| 10 | blake3 | 0.0078 | 0.779 | 5,052 | — |
| 10 | identity | 0.0254 | 2.542 | 4,596 | — |
| 10 | summaries | 0.0003 | 0.028 | 4,720 | — |
| 10 | verify | 1.2858 | 128.581 | 5,224 | 24,184 |
| 100 | sha256 | 0.5499 | 5.499 | 4,964 | — |
| 100 | blake3 | 0.0645 | 0.645 | 5,056 | — |
| 100 | identity | 0.0060 | 0.060 | 4,752 | — |
| 100 | summaries | 0.0022 | 0.022 | 4,772 | — |
| 100 | verify | 10.4819 | 104.819 | 5,132 | 24,188 |
| 1,000 | sha256 | 5.4989 | 5.499 | 5,112 | — |
| 1,000 | blake3 | 0.7687 | 0.769 | 5,196 | — |
| 1,000 | identity | 0.0890 | 0.089 | 4,756 | — |
| 1,000 | summaries | 0.0373 | 0.037 | 4,752 | — |
| 1,000 | verify | 100.8309 | 100.831 | 5,328 | 24,208 |
| 5,000 | sha256 | 34.3885 | 6.878 | 5,376 | — |
| 5,000 | blake3 | 30.1355 | 6.027 | 5,576 | — |
| 5,000 | identity | 0.4607 | 0.092 | 5,124 | — |
| 5,000 | summaries | 0.1176 | 0.024 | 5,244 | — |
| 5,000 | verify | 564.8691 | 112.974 | 5,884 | 24,220 |

### workstation: large game lists (QuickJS)

| Games | Surface | init ms | right ms | view ms | View JSON bytes | QuickJS peak MiB | RSS HWM MiB | Result |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 1,000 | zone | 0.180 | 0.041 | 30.928 | 330347 | 2.50 | 10.65 | Pass |
| 1,000 | pico | 0.016 | 2.898 | 29.334 | 334867 | 2.44 | 10.12 | Pass |
| 10,000 | zone | 0.190 | 0.036 | 303.075 | 3304097 | 20.64 | 50.89 | Pass |
| 10,000 | pico | 0.034 | 37.829 | 253.900 | 3358158 | 19.57 | 46.69 | Pass |
| 50,000 | zone | 0.146 | 0.025 | 824.230 | 16560996 | 101.27 | 229.28 | Pass |
| 50,000 | pico | 0.036 | 121.537 | 1235.690 | 16874994 | 96.36 | 209.04 | Pass |

### sp: installed packages

| N | Work | Total s | ms/item | Phase RSS HWM KiB | Worker RSS HWM KiB |
| ---: | --- | ---: | ---: | ---: | ---: |

### Workstation shell under Xvfb (640×480)

| Games | Surface | First frame ms | Max sampled RSS MiB | Reported redraw fps | Result |
| ---: | --- | ---: | ---: | --- | --- |
| 10,000 | zone | 1344.490 | 242.70 | 32.555, 26.803, 16.496, 19.553, 26.386 | Frame rendered |
| 10,000 | pico | 3448.562 | 1254.23 | 0.200, 0.000, 0.930, 1.588, 1.142 | Frame rendered |
| 50,000 | zone | 5944.364 | 1029.12 | 3.327, 1.314, 2.737, 3.397 | Frame rendered |
| 50,000 | pico | — | 3685.27 | — | Allocation failed at 4 GiB virtual limit |

The largest tested working list is 50,000 for Zone and 10,000 for Pico.
Zone still draws, but its animation is slow. Pico's 50,000-game run fails
allocation before its first frame at the safety cap; removing that cap was
not tested. Pico's initial 0 fps intervals mean an unchanged static screen,
not a failed frame. After `right`, its active redraw rate is 0.930–1.588 fps.
The screenshots were read back: Zone selects “Tetris 2” and Pico changes
from “Celeste Classic 0” to “Hollow Knight 1”.

Both surfaces do unbounded work over the catalog:

| Surface | Where | Cost that remains |
| --- | --- | --- |
| Zone | `surfaces/zone/logic.ts:225–267` inside this spike: `band.map` builds every card on each view | O(N) projection and JSON; the `.slint:1590` repeater still creates one outer Rectangle/condition per card, though it culls inner CardViews |
| Pico | `surfaces/pico/logic.ts:45–53,88–96,174–200`: shelf projection, selection search, readout/filter, narrowest-art reduction, and `games.map` | O(N) update/view work; `.slint:472` constructs a full ShelfCart for every game, including offscreen carts |

No surface package was changed. These are runtime/boundary measurements,
not visual redesigns. Full UI virtualization and changed projection strategies
would need their own slice.


## RG35XX SP results and safety

Build with the repository's configured aarch64 builders, fuji/yari, never SP:

```sh
nix build --impure --no-link --print-out-paths --expr \
  '(import <nixpkgs> { system = "aarch64-linux"; }).callPackage ./clients/surface-shell-spike/package.nix {}'
clients/surface-shell-spike/device/bundle.sh STORE_PATH surface-verify /tmp/surface-verify-sp
clients/surface-shell-spike/verify/sp.sh /tmp/surface-verify-sp.tar.gz clients/surface-shell-spike/verify/evidence/sp
```

The driver checks that `korri-games.slice` is absent or recursively empty
before copying and immediately before running. It uses SSH port 2222 to
`root@10.42.5.1`. All workload commands run under `nice -n 10`. They open no
window and do not access input devices. They do not stop/start any service,
change kiosk/InputPlumber configuration, widen permissions, bypass signatures
or build on the device. A unique `/tmp/surface-verify-*` holds the archive,
bundle, generated copies, game lists, temporary HOME/cache and results. The
EXIT trap removes only that run's directory after copying evidence back.
The driver puts HOME, XDG cache/config, fontconfig cache and temp files
under that same directory. It disables core dumps. `SURFACE_VERIFY_PART=installed`
or `large` runs just that workload; the default runs both.
`SURFACE_VERIFY_LAUNCHER` names the bundle's `run.sh` so child processes use
the bundled loader too, not `/proc/self/exe`'s explicit-loader identity.

**Incomplete SP run.** All 19 completed rows below finished before
**2026-10-31 16:11 UTC** and are clean. N=10, 100 and 1,000 full verification
passed with zero failures. An independent session then used SP and repeatedly
stopped/started its kiosk. This driver did not make those service calls.
The supervisor stopped this benchmark to avoid interference.

The partial N=5,000 full-verification run overlapped that workload and is
**contended and invalid**. The final row below is explicitly **extrapolated**
from the clean N=1,000 rate: `962.019037748 / 1000 * 5000 = 4810.095188740 s`
(**80.1683 min**). Its RSS and pass/fail result were not measured.

### SP installed packages (4 verification workers)


| N | Work | Total s | ms/item | Phase RSS HWM KiB | Worker RSS HWM KiB |
| ---: | --- | ---: | ---: | ---: | ---: |
| 10 | sha256 | 0.2070 | 20.695 | 3,824 | — |
| 10 | blake3 | 0.0820 | 8.203 | 3,836 | — |
| 10 | identity | 0.0042 | 0.415 | 3,508 | — |
| 10 | summaries | 0.0015 | 0.152 | 3,500 | — |
| 10 | verify | 11.2540 | 1125.403 | 3,940 | 23,012 |
| 100 | sha256 | 3.1850 | 31.850 | 4,728 | — |
| 100 | blake3 | 1.2429 | 12.429 | 4,728 | — |
| 100 | identity | 0.0427 | 0.427 | 4,416 | — |
| 100 | summaries | 0.0263 | 0.263 | 4,396 | — |
| 100 | verify | 98.4433 | 984.433 | 4,800 | 23,028 |
| 1,000 | sha256 | 128.1326 | 128.133 | 4,768 | — |
| 1,000 | blake3 | 104.9775 | 104.977 | 4,640 | — |
| 1,000 | identity | 1.8202 | 1.820 | 4,408 | — |
| 1,000 | summaries | 0.8947 | 0.895 | 4,516 | — |
| 1,000 | verify | 962.0190 | 962.019 | 4,924 | 23,028 |
| 5,000 | sha256 | 657.2019 | 131.440 | 5,056 | — |
| 5,000 | blake3 | 526.3433 | 105.269 | 5,092 | — |
| 5,000 | identity | 17.5640 | 3.513 | 4,988 | — |
| 5,000 | summaries | 6.9752 | 1.395 | 5,080 | — |
| 5,000 | verify **EXTRAPOLATED** | 4810.0952 | 962.019 | — | — |


**SP large-game-list QuickJS: not run.** Both surfaces' 1k/10k/50k runs were
cancelled before starting because the device was occupied. No estimates are
substituted for those six tests. The final prebuilt binary and `sp.sh` can run
them later with `SURFACE_VERIFY_PART=large` when SP is free.

Only our own verifier processes and their SSH command were terminated. The
owned `/tmp` directory was deleted and no verifier process remained. We did
not touch the independent session or restore its kiosk. Compositor PID 18798
and InputPlumber PID 22326 stayed unchanged. See `evidence/sp/INTERRUPTED.md`,
`interruption.txt`, `cleanup.txt` and the unmodified raw rows.


Zone and Pico's SP summaries have identical package/fixture digests, recursive
property descriptions, sweep counts and pass results to the workstation
summaries. Their measured SP max call times are 43.301 ms and 22.642 ms;
QuickJS backing peaks are 856,768 and 621,064 bytes.


## Checks

`cargo fmt --check`, `cargo clippy --release --all-targets -- -D warnings` and
`cargo test --release --bin surface-verify` pass. The verifier target has 15
tests. Planted defects cover an unknown key (including a null value), a wrong
scalar type, a nested array/struct type error, a throw, an infinite loop,
memory exhaustion, and an undecodable image. A positive test covers retained
nulls and partial structs. The allocator test checks the hard cap and that a
transient allocation remains visible in the peak after free. Existing shell
converter tests are also exercised through the verifier target. The native
aarch64 Nix build on fuji also passed all 25 crate tests (10 shell + 15 verifier).
`bash -n`, ShellCheck and Python byte-compilation pass for the harnesses.

### Remaining SP run (only when the device is free)

The missing large-list measurements can use the final tested aarch64 build:

```sh
# If /tmp/surface-verify-sp-final.tar.gz was removed, recreate it first:
clients/surface-shell-spike/device/bundle.sh \
  /nix/store/ry7vhnb1w51ncvdid414nl9bbg9z2c08-surface-shell-spike-0.0.0 \
  surface-verify /tmp/surface-verify-sp-final
SURFACE_VERIFY_PART=large clients/surface-shell-spike/verify/sp.sh \
  /tmp/surface-verify-sp-final.tar.gz /tmp/surface-verify-sp-large-results
```

`bundle.sh` requires a fresh output directory/archive name. Do not run the
SP driver while the independent session is using the device, even if the game
slice is empty. The existing game-slice preflight alone does not reserve SP.
