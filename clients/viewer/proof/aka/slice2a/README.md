# Stage 1 / slice 2a — PC sound and input (#435)

Code commits: `c86b95259`, `77ae2b6b7`.
Built and checked on zao, not on a device:
`/nix/store/z3pdx54250q24isdkpkaqszp8fpryhf1-korri-viewer-0.1.0`.
The source starts at `origin/main` / `c00fe2680`. No plugin or product update
was installed. No tool source changed. No push or landing was done.

The rebased landing commits are `ca4f091cd` and `fdcd8562b`, with package
`/nix/store/bpdhn0vnwk1dbj70l3p3hiqf0ff3vxdr-korri-viewer-0.1.0` and 27
passing tests; `io/`, `picture/`, `src/player_io.rs`, `src/options.rs` and
`build.rs` are identical between that package and the pre-rebase package above.

## Archived checks (pre-rebase)

| Check | Result |
| --- | --- |
| New CLI option-retention test, before implementation | Failed: the parser discarded input/audio options. `checks/options-red.log`. |
| Mouse-emulation producer shutdown regression | Failed before the teardown adaptation; passed after it. `checks/producer-red.log`. |
| `nix build .#korri-viewer` | Passed, including 9 Rust tests. |
| `checks.x86_64-linux.korri-viewer-picture` | Passed: VA-API failure cleanup, latest-frame ownership, resize, actual H.264 software decode/presentation. |
| `checks.x86_64-linux.korri-viewer-io` | Passed: real Opus packets into ALSA null; SDL/X11/evdev forwarding; controller hotplug, first-press seat and quit ownership; producer shutdown. |
| `cargo test`, under Xvfb | Passed: 9 tests. |
| `cargo clippy --all-targets -- -D warnings` | Passed. Carried C warnings remain; the carried C was not rewritten to silence them. |
| `cargo fmt --check`, Nixfmt, `git diff --check` | Passed. |
| Carried-source comparison | Passed: 9 files byte-identical; 6 adapted files differ only as named in `io/README.md`. Exact diffs are in `checks/carried-source.txt`. |
| Shipped controller mapping versus Moonlight #466 | `cmp` passed: byte-identical, including both InputPlumber corrections. |
| Secret-scanning commit hook | Passed for both code commits. |

The Opus check encodes and decodes four 20 ms packets for each of stereo,
5.1 and 7.1. It reads back the PCM written through ALSA's file plugin over
null: 15,360, 46,080 and 61,440 bytes, respectively. It checks that PCM is
nonzero. This proves decode and output, not audible hardware output.

`checks/viewonly.trace` records the real C initializers and SDL event loop
for both PC platform choices with view-only enabled. It contains zero
`/dev/input` paths. Keyboard, mouse and controller forwarding are also zero.
This is build-machine evidence; it is not a substitute for the requested
aka run's isolation proof.

The input checks record calls at the actual network-core boundary. They do
not measure encrypted packets leaving the device. evdev uses real libevdev
capabilities without a kernel node. Its shutdown check therefore logs the
expected unbound-device message when the carried drain operation runs.
Physical evdev discovery/grabs/rumble and a live Pulse server remain unproven.

## What did not work in the build

The first native check used separately copied Nix source paths, which broke
relative C includes. One fileset-backed source root fixed it. The software
picture check needed the VA-API object once the shared C flags enabled VA-API.
The HTTPS launch-refusal test now reaches input/presenter setup before launch,
as Moonlight does. It initially failed without a display; the Nix Rust test
runner now uses Xvfb, and that test explicitly uses `-viewonly`.

## First stopped aka attempt

Tool: the main checkout at `c00fe2680`. A is Moonlight #466:
`/nix/store/38crl2m37akjqqscrmnq1gbwrp8l3rcx-moonlight-embedded-korri-2.7.1-korri`.
B is the package above. Command:

```sh
nix run .#stream-cadence -- view --sender root@aka --sender-fixture --viewer root@aka \
  --viewer-package A --candidate-package B --runs 5 --out DIR
```

| Attempt | Tctl | Result |
| --- | --- | --- |
| 2026-11-13 19:31 UTC, base | 78.875 °C before start; 94.125 °C 15 seconds later | Stopped before the first baseline viewer started. No candidate run. No performance or isolation verdict. |

Unrelated shader preparation and compiler processes were running on aka.
Only this task's fixture was stopped. Their processes were left alone.

**The stopped run did not pass the tool's restore check.** The first heat
wrapper sent SIGINT to the whole local process group, including the SSH
transports. The tool then recorded `Broken pipe` for both restore requests.
That interruption method was wrong. The next wrapper interrupts only the
tool PID so it can close the agents in order.

A manual audit at 19:38 UTC found no fixture process or fixture directory.
The product's Sunshine, compositor and korrid remained active with the same
PIDs and start times. The product Sunshine PID was `2569277`, started on
November 12, before this attempt. See
`first-heat-stop/manual-restore-audit.txt`. This manual audit does not turn
the failed automated restore record into a pass.

`first-heat-stop/result.json` is the unchanged stopped-run result.
`first-heat-stop/verdict.txt` is the tool's refusal: zero valid viewer runs;
five per side are required.

## Temperature wait and device verdict

The first tool recorded no thermal zones on aka and said `heat_guarded: false`.
An outside watcher read k10temp Tctl at
`/sys/class/hwmon/hwmon4/temp1_input`. From 19:35 UTC it required Tctl below
80 °C for ten minutes, sampled every five minutes, before another attempt.
No such window occurred.

At 22:03 UTC the owner changed this rule: use the main tool at `849e74149`,
which starts below 85 °C and ends at 100 °C on AMD's package sensor. Start as
soon as no `korri-game-*` or `hookshot-*` unit is active and Tctl is below
85 °C. The outside 80/90 °C guard and ten-minute wait were removed. The
three-hour wait deadline remains 22:35 UTC.

`temperature.tsv` is the unaltered watcher log. Columns are UTC, phase, Tctl
in millidegrees Celsius, GPU edge temperature in millidegrees Celsius, then
`/proc/loadavg` and any active game units. Its phase marks the rule change.

The second attempt polled Tctl at 84.375 °C, but the tool read 90.875 °C
by the time it prepared the sender and refused to start. Its turn checkpoint
records say “fixture is not prepared”; both final `session_cleanup` records
pass, with no remaining process. Evidence: `second-heat-refusal/`.

### Third attempt: four valid turns, then a tool fault

The same command on main `849e74149` started at 22:13 UTC. It requested five
turns per side. The immutable aggregate results are in `turns/base/`.

| Viewer / turn | Recorded validity | Startup | CPU seconds/second | Tctl range |
| --- | --- | --- | --- | --- |
| Moonlight 1 | valid | 802.9 ms | 0.02025 | 78.2–88.8 °C |
| Korri viewer 1 | valid | 753.8 ms | 0.01771 | 68.5–86.6 °C |
| Moonlight 2 | valid | 705.0 ms | 0.01866 | 68.9–87.1 °C |
| Korri viewer 2 | valid | 718.4 ms | 0.01802 | 67.1–83.8 °C |
| Moonlight 3 | invalid: hot checkpoint | 753.4 ms, diagnostic only | 0.02238, diagnostic only | 74.1–95.2 °C |

Both candidate turns pass `viewer_isolation`. Every one of their 328 combined
isolation samples has `input_nodes: []`. Both turns report `viewer_fixture.audio:
null`, VA-API decode, zero stream stalls, zero capture gaps and zero output
repeats. Sender and viewer restore checks pass for all four valid turns.

Moonlight turn 3 finished below the new 100 °C end limit. The viewer's
checkpoint then failed at 86 °C with the *start* refusal: “A run does not start
at or above 85.0 °C”. This is a tool fault: the start guard is also checked
after stopping the viewer. The recorded third turn stays invalid; its JSON
was not edited. Both final session-cleanup records pass, the fixture directories
are removed, no fixture process remains, and the product-unit comparisons show
no change. No product service was restarted or reconfigured by this task.

The owner confirmed that fault, stopped further attempts and assigned the tool
fix and the full rerun elsewhere. The original deadline no longer applies.
No further fixture is running for this task.

| Requested proof | Verdict | Tctl |
| --- | --- | --- |
| Base, five turns each | Refused: only two valid runs per side; no equal/better claim | Valid turns: 67.1–88.8 °C |
| Queue, five turns each | Not run after the base tool fault | Not measured |
| Candidate view-only isolation | Pass: both valid aka turns, 328 empty input-node samples | 67.1–86.6 °C |
| Opus decode and null output | Pass on zao, stereo/5.1/7.1 | Not measured |

`turns/base/verdict.txt` is the real tool verdict: “2 valid viewer runs; a verdict
needs 5 in each result”. These observations are not a performance acceptance.

## Sound in the sender fixture

Both candidate native logs say “No audio traffic was ever received from the
host!” and report the VA-API platform. Those logs are retained in `turns/base/`.
The unchanged fixture provides no sound source or private audio server; it
cannot prove end-to-end sound in these runs. The Opus/null check above covers
the shipped decoder and output instead. A separate sound-enabled fixture
could run a private Pulse server with a null sink, give only that private
socket to Sunshine via `PULSE_SERVER`, and feed a known tone into that sink.
Sunshine would capture its monitor. That setup is not tested here and is not
an option in the unchanged measure. Do not connect it to product audio.

Not measured: audible sound, physical input, button-to-packet delay, battery,
or packet-arrival-to-display timing.

## Full proof in turns (coordinator, 2026-11-13)

After the tool fixes `84a68e52c` (a hot checkpoint keeps a finished run, and
the wait between runs ends 3 °C below the start limit), the coordinator ran
the full proof with the tool on `main` at that commit:

- A: Moonlight with #466,
  `/nix/store/38crl2m37akjqqscrmnq1gbwrp8l3rcx-moonlight-embedded-korri-2.7.1-korri`.
- B: this viewer with the SDL mouse fix (`1ff54c502`),
  `/nix/store/hvmgv9g2g4pgd9i4ayv0jz5rh5ngirq9-korri-viewer-0.1.0`.
  The fix touches only the SDL path; both sides ran VA-API.
- Five turns each, started only with no game or other test unit live, load
  below 1.5 and Tctl below 80 °C. Both measures were kept on their first try.

Results and the unaltered verdicts are in `full/{base,queue}/`.

| Measure | Started (UTC) | Verdict | Other load, A / B |
| --- | --- | --- | --- |
| Base | 23:15 | equal on every measured row | 4.1–47.2% / 10.4–20.9% |
| Queue | 23:44 | equal on every measured row | 0.8–2.9% / 0.9–7.5% |

Base: delay p50 11.9–13.7 ms (A) and 12.0–12.2 ms (B), CPU 0.012–0.018 on
both. Queue: delay p50 40.3–40.8 ms (A) and 40.3–40.5 ms (B). All 20 runs
are valid, pass the isolation proof (no input node held), and record
`viewer_fixture.audio: "null"` (the ALSA null device). Earlier attempts,
refused for a game or stopped by the hot-checkpoint fault, are not part of
this verdict. The `turns/` folder above is the worker's partial attempt.
