# Stage 1 / slice 1 — aka proof

Implementation commit: `095a66d00efad764d0ce60846a6501ffbdc30760`.
Built on zao with `nix build .#korri-viewer`:
`/nix/store/mac720f6dl57mx7bb7xjyyvb11dgvdkp-korri-viewer-0.1.0`.

The measure runs from the main checkout, not this worktree. No measure,
korrid or plugin source changed. Nothing was installed into the product.
The tool imports immutable Nix closures and owns its temporary fixtures.

## Checks

| Check | Result |
| --- | --- |
| Initial CLI tests against an empty executable | Failed as expected; then passed with the implementation |
| Rendered-PC-settings CLI regression | Failed on missing `-hdr`; then passed with all current rendered settings accepted |
| TLS-session-cache regression | Failed with `[false, true]` (second connection resumed); passed with `[false, false]` after matching Moonlight's disabled cache |
| Nix viewer build, including Rust release tests | Passed: 8 Rust tests |
| `cargo clippy --all-targets -- -D warnings` | Passed |
| `cargo fmt --check`, Nixfmt, staged whitespace check | Passed |
| `checks.x86_64-linux.korri-viewer-picture` | Passed: VA failure cleanup; X11 newest-frame ownership and resize; actual H.264 software decode and SDL presentation |
| Compare carried PC files and copied tests against the patch stack extracted from `korri-plugins` history at `2094c1e` | Passed; only the documented X11 input removal differs |
| Secret-scanning pre-commit hook | Passed |

The SDL test initially lacked Mesa's driver search path inside the Nix sandbox.
Adding that path to the test environment fixed it. The production picture
code did not change for the test.

## First failed device attempt

`/tmp/korri-stage1-aka/base`, 2026-11-13 15:57 UTC, was invalid. Pairing
succeeded, but the second HTTPS request failed with a TLS internal alert.
The Rust transport had left libcurl's TLS session cache enabled. Moonlight's
`http_init` explicitly disables it. A real TLS regression test reproduced
resumption before the fix. The next package disables it too.

That attempt's sender and viewer restore checks passed. Their recorded product
units did not change; temporary directories and fixture processes were removed.
It was not mixed into the candidate results.

## Base set

Five valid runs, started after a quiet preflight at 16:05:59 UTC: no active
`korri-game-*` or `hookshot-*` unit; 1-minute load 0.93. The device did not stay
quiet. The measured non-tool CPU share ranged from 9.9% to 97.8% across runs.
At 16:14 UTC a separate `/var/tmp/oot-piano-test/game/hookshot` process used
about 11 CPUs. The test did not stop or reconfigure it.

The strict verdict refused this comparison on load. The diagnostic verdict,
explicitly allowing different conditions, is in `aka/base/verdict-diagnostic.txt`.
It says **worse** for latency p50, latency p95 and CPU. That is not an autonomous
performance pass, and the data cannot separate viewer cost from the other load.
Every other measured row says equal. The startup range includes a 10.573 s
outlier; the baseline also has starts around 10 s.

All five runs report `x11_vaapi`, `decode=vaapi`, and `renderD129`. They have
no capture gap, stream stall or viewer exit. All sender/viewer restore checks
pass, with no changed product unit and no remaining fixture process.

The unchanged command ran from the main checkout:

```sh
nix run .#stream-cadence -- view --sender root@aka --sender-fixture \
  --viewer root@aka --runs 5 \
  --viewer-package /nix/store/mac720f6dl57mx7bb7xjyyvb11dgvdkp-korri-viewer-0.1.0 \
  --out /tmp/korri-stage1-aka/base2
```

## Queue set

Five valid runs. The preflight waited from 16:15:37 to 16:42:42 UTC
(27 minutes), then found no active `korri-game-*` or `hookshot-*` unit and a
1-minute load of 0.40. Other activity resumed during the set: its measured
CPU share ranged from 6.9% to 29.5%. The strict verdict refused this comparison
too. Nothing was stopped to make the machine quiet.

With the explicit diagnostic override, the table says better for latency p50,
latency p95, repeated frames and late frames; **worse** for CPU; equal for the
other measured rows. Dropped frames are not judged in the queue measure.
All five restore checks pass on both sides, with no changed product unit,
remaining fixture process, capture gap, stall or viewer exit. All runs report
VA-API on `renderD129`.

The command was the same as the base command, with `--queue-measure` and
`--out /tmp/korri-stage1-aka/queue`.

## Baseline and interpretation

At the final comparison, `main` and freshly fetched `origin/main` both resolve
to `ec76bcd150a8e5023af32fe06b71fc27fcf9620c`. The baselines are the files at
`nix/stream-cadence/baselines/aka/private-pair/{base,queue}/results.json` in that
commit. The #466 baseline update had not landed. These are still the #439
package, whereas the candidate carries #466. Differences cannot be attributed
to Rust alone, even without the other load.

The strict verdicts are kept in `aka/{base,queue}/verdict.txt`. The full
unaltered diagnostic output, including the load warning, is beside each at
`verdict-diagnostic.txt`. Raw result lists are beside them. The invalid first
attempt is kept separately at `aka/first-failure/results.json`.

These runs prove function, not equal-or-better performance. Do not treat the
diagnostic override as a performance pass. No row or run was removed to make
the comparison look better.

### Base diagnostic table (unaltered)

```text
number | metric | baseline runs | candidate runs | verdict
1 packet arrival to display | packet_to_display_ms | not measured | not measured | not measured
1 sender presentation to display | latency_p50_ms | 10.365..10.457 | 12.051..16.483 | worse
1 sender presentation to display | latency_p95_ms | 17.564..17.643 | 19.164..24.243 | worse
2 smoothness | dropped_frames | 0.000..0.000 | 0.000..14.000 | equal
2 smoothness | repeated_frames | 232.000..234.000 | 233.000..266.000 | equal
2 smoothness | late_frames | 232.000..234.000 | 228.000..234.000 | equal
2 smoothness | torn_frames | 0.000..0.000 | 0.000..0.000 | equal
2 smoothness | unreadable_frames | 0.000..0.000 | 0.000..0.000 | equal
2 smoothness | output_repeats | 0.000..0.000 | 0.000..0.000 | equal
3 CPU and battery | cpu_seconds_per_second | 0.011..0.011 | 0.013..0.032 | worse
3 CPU and battery | battery_mw | not measured | not measured | not measured
4 startup | startup_ms | 640.803..10075.467 | 656.707..10572.664 | equal
5 input | input_ms | not measured | not measured | not measured
6 stability | stream_stalls | 0.000..0.000 | 0.000..0.000 | equal
6 stability | viewer_exits | 0.000..0.000 | 0.000..0.000 | equal

```

### Queue diagnostic table (unaltered)

```text
number | metric | baseline runs | candidate runs | verdict
1 packet arrival to display | packet_to_display_ms | not measured | not measured | not measured
1 sender presentation to display | latency_p50_ms | 749.230..829.532 | 40.247..41.720 | better
1 sender presentation to display | latency_p95_ms | 1201.321..1267.822 | 47.099..48.732 | better
2 smoothness | dropped_frames | not measured | not measured | not measured
2 smoothness | repeated_frames | 2.000..3.000 | 0.000..0.000 | better
2 smoothness | late_frames | 2.000..3.000 | 0.000..0.000 | better
2 smoothness | torn_frames | 0.000..0.000 | 0.000..0.000 | equal
2 smoothness | unreadable_frames | 0.000..0.000 | 0.000..0.000 | equal
2 smoothness | output_repeats | 0.000..0.000 | 0.000..0.000 | equal
3 CPU and battery | cpu_seconds_per_second | 0.012..0.012 | 0.014..0.024 | worse
3 CPU and battery | battery_mw | not measured | not measured | not measured
4 startup | startup_ms | 721.025..766.808 | 658.795..739.236 | equal
5 input | input_ms | not measured | not measured | not measured
6 stability | stream_stalls | 0.000..0.000 | 0.000..0.000 | equal
6 stability | viewer_exits | 0.000..0.000 | 0.000..0.000 | equal

```
