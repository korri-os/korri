# Stage 1 / slice 1 — aka proof

Sound/input slice 2a has its own record at `aka/slice2a/README.md`. It includes
passing sound/input checks and two valid aka turns per side; the full verdict
is blocked by a tool checkpoint fault. The record below is slice 1 only.

The handheld `v4l2request` build proof (aarch64, no device) is at
`v4l2request/README.md`.

The frame record and number 1 (#435) are at `aka/frame-times/README.md`.

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
| Compare carried PC files and copied tests against the patch stack extracted from `korri-plugins` history at `2094c1e` | Passed. The decoder and presenter files under `picture/video/` match. Two adaptations differ: X11 input is removed, and `picture/sdl.c` (lines 71 to 121) and `picture/sdl.h` (lines 37 to 39) change SDL event forwarding, polling and teardown for the Rust session |
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

## Proof in turns against Moonlight

Moonlight and Korri's viewer ran alternately in one private-pair session on
aka (`stream-cadence view --candidate-package`, tool at `a2ddfa6a0`). Both
sides see the same machine load, so `verdict` compares them although aka was
busy with other people's tests.

- A, the baseline: `@korri:moonlight` release
  `build-2094c1e77471-moonlight-latest-frame` (#466),
  `/nix/store/38crl2m37akjqqscrmnq1gbwrp8l3rcx-moonlight-embedded-korri-2.7.1-korri`.
- B, the candidate: this viewer,
  `/nix/store/mac720f6dl57mx7bb7xjyyvb11dgvdkp-korri-viewer-0.1.0`.
- Both: `x11_vaapi`, `decode=vaapi`, 1920x1080 at 60 fps, five turns each.

```sh
stream-cadence view --sender root@aka --sender-fixture --viewer root@aka \
  --viewer-package A --candidate-package B --runs 5 [--queue-measure] --out DIR
stream-cadence verdict DIR/baseline/results.json DIR/candidate/results.json
```

Results: `aka/turns/{base,queue}/{moonlight,korri-viewer}.json`, verdicts in
`aka/turns/{base,queue}/verdict.txt`.

| Measure | Other load on aka | Verdict |
| --- | --- | --- |
| Base, 2026-11-13 | 34% to 98% of all CPUs on both sides | equal on every measured row |
| Queue, 2026-11-13 | 34% to 48% on both sides | equal on every measured row |

The first base session stopped at turn 8: the candidate's run 4 had one
capture gap that lost a frame ID, which makes a run invalid. Its first three
turns of each viewer were valid. The whole base session was run once more;
all ten runs of the second session are valid, and they are the ones kept.

Base: delay p50 12.2 to 16.7 ms on both sides, CPU 0.019 to 0.034 seconds each
second on both. Moonlight's startup range includes one start of 10.2 s (#467);
the viewer's starts were 706 to 1095 ms. Queue: delay p50 40.3 to 40.6 ms
(Moonlight) and 40.3 to 40.4 ms (viewer).

Not measured: packet arrival to display, button press to packet, battery.
The viewer has no sound yet (null audio callbacks); the test sender sends
none either.

The earlier sets of this slice, recorded at different times than their
baseline, were refused for load and are removed. The invalid first attempt
stays at `aka/first-failure/results.json`.
