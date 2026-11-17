# #391 step E, slice 1: nightjar-core behind `NetworkCore`

Three parts, each with its own evidence: the audit of the research brief's
claims about nightjar-core, the proof that the viewer streams with it, and
one set of turns of the measure against the C core.

## 1. The brief's claims, read in the source

Brief: `docs/research/2026-11-13-viewer-rust-network-core.md`. Read on
2026-11-16 in `RamazanKara/nightjar` at
`7faa84f7d2ee86ad0b5b39dd67677c42aaedc8cf`, the commit that
`vendor/nightjar` holds; paths are under `protocol/` unless named.

| Claim | Holds? | Where |
| --- | --- | --- |
| GPL-3.0-only | Yes. No licence trouble for ADR 0059 (GPL-3). | `Cargo.toml:16` `license = "GPL-3.0-only"`, inherited by every crate (`client/Cargo.toml:6`); `LICENSE.txt` is the GPL v3 text. Its git dependency `rubylight-native-api` is GPL-3.0-only too (Rubylight `17fc45bb`, `rust/core/native-api/Cargo.toml:11`). The vendored `rusty_enet` is MIT (`vendor/rusty_enet/Cargo.toml:25`) |
| Encrypted video | Yes | `client/src/video/decrypt.rs:1-36`: AES-128-GCM over each `SS_ENC_VIDEO` datagram's 32-byte header; `client/src/video/mod.rs:64` takes the key; `client/src/session/mod.rs:1218,1226` sets it when the host enabled video encryption |
| Encrypted audio | Yes | `client/src/session/mod.rs:1437` (`AudioEncryption`); AES-128-CBC in `client/src/audio/mod.rs:194` |
| Controller input | Yes | `client/src/input/queue.rs:412` `send_controller` (and arrival `:523`, touch `:548`, motion, battery); encoded as `MULTI_CONTROLLER` in `client/src/input/packets.rs:181,287` |
| RFI | Yes | `client/src/control/session.rs:542` queues `InvalidateReferenceFrames`; negotiated from `x-nv-video[0].refPicInvalidation` (`client/src/rtsp/describe.rs:201`, `client/src/rtsp/mod.rs:303`) |
| Per-frame receive times | Yes, with a limit | `client/src/video/mod.rs:134-137` (`receive_time_us`, `enqueue_time_us`), set in `client/src/video/depacketizer.rs:451-452`. The clock is `session::now_us()` (`client/src/session/mod.rs:50-53`). The datagrams of one `recvmmsg` batch share the time the batch was handled (`client/src/session/video.rs:986`); no kernel receive stamp is read |
| Differential tests against moonlight-common-c | Partly | `difftest/` compiles Nightjar's **patched** moonlight-common-c (16 Nightjar patches and its own `android_rtp_video.c` and `android_depacketizer.c`, `difftest/build.rs:10-35`) from the submodule at `f900dd4767759c7b9d0e93bcea666b55c69ea62f`, not Korri's `b126e481`, and compares the receive paths only: video RTP queue and depacketizer (12 tests, `difftest/tests/video.rs`, with loss, reorder, duplicates and `encrypted_video` at `:417`), audio (7) and FEC (3). RTSP, ENet control and input have no differential test. The "JNI parity test" (`app/src/test/java/com/limelight/nvstream/jni/RustEngineJniParityTest.java`) checks JNI signatures, not behaviour. Korri cannot run `difftest`: it needs Nightjar's app submodule, which is not vendored |
| Works against Sunshine | Yes in code, untested upstream; **proven here** (part 2) | The code targets the Sunshine family: encrypted RTSP `rtspenc://` (`client/src/rtsp/mod.rs:9,154`), `x-ss-*` encryption and feature flags (`client/src/sdp.rs:56`), Sunshine detected by the negative fourth `appversion` part (`client/src/control/negotiation.rs:61-62`: "Sunshine and its forks (Apollo, Vibepollo, Rubylight)"). Its interop tests run against Rubylight's host code (`client/tests/host_interop.rs`, `butterpollo-core`), and its end-to-end records are against Rubylight (`internal/RUST_CORE.md:82-85,143-147`). No test or record against stock Sunshine was found |

No hard need fails, so slice 1 went on to the vendored copy and the
implementation.

Not asked, found while reading: nightjar-core's `send_controller_touch`
takes a touchpad index that moonlight-common-c `b126e481` does not have
(`client/src/input/queue.rs:548`); Korri passes the first touchpad.
nightjar-core refuses to build for aarch64 without `--cfg aes_armv8 --cfg
polyval_armv8` (`client/src/crypto.rs:10-12`).

## 2. The proof: picture, sound and input with each core

```sh
# On aka, as root, in a checkout. The packages are in aka's store.
nix run .#viewer-nightjar-proof -- --build-store auto \
  --viewer /nix/store/zqya4nbg6hwi0z0vj01v4vskmvl52adl-korri-viewer-nightjar-0.1.0 \
  --sunshine /nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri
```

`run.py`'s docstring has the steps. The sender is stream-cadence's sender
fixture with Korri's patched Sunshine (`sunshine-korri`
`2026.914.233613-korri`, patches through 0046: the one aka's product runs),
encryption mandatory, a 1000 Hz tone and its input log. One viewer binary,
`korri-viewer-nightjar`, streams twice: `-networkcore c`, then
`-networkcore nightjar`, each on a private Xvfb with the SDL platform, its
sound to SDL's disk driver, `-viewonly`.

Record: `aka-2026-11-17/` (`result.json`; each core's `viewer.log`,
`times.csv` and first and last screenshot; the sender's Sunshine log at
info and above, and the input packets it logged). All 16 checks passed.

| | C core | nightjar-core |
| --- | --- | --- |
| Sunshine | "Video encryption enabled", session started | "Video encryption enabled", session started |
| Bar in 6 screenshots | 207 → 1212, rising | 1769 → 2773, rising |
| Sound (≈23 s of PCM) | 20.1 s at 1000 Hz | 22.1 s at 1000 Hz |
| Keys at Sunshine (5 presses) | 10 keyboard packets, key C | 10 keyboard packets, key N |
| Click at Sunshine | press and release | press and release |
| Frame record | 1333 units, 1324 shown; enqueue to display p50 2.97 ms | 1366 units, 1355 shown; p50 2.99 ms |
| Exit after SIGINT | 0 | 0 |

nightjar-core's own count at the stop: 26,448 video datagrams (4,965 FEC),
0 decrypt failures, 1,355 frames completed and submitted, 0 dropped, 0 key
frame requests; 4,419 audio packets, 0 decrypt failures; RTT 1 ms. The
fixture's restore check passed: no product unit changed its PID, no input
device appeared or went, the run folder was removed.

Not proven by this run: loss and recovery (a loopback loses nothing, so no
FEC repair or RFI ran), a controller (`-viewonly`; Sunshine's controller
input is off in the fixture), rumble and the other host events, VA-API (the
SDL software path ran here; the turns below run VA-API).

## 3. Turns: C against nightjar-core

One set of turns of the measure on 2026-11-17, 01:05 to 01:21 UTC, private
pair on aka, five runs of each side in the order A B A B, at the defaults
(1920x1080, 60 fps request, H.264, base measure, 60 scored seconds), with
Korri's patched Sunshine and mandatory encryption:

```sh
# On aka, in a checkout, while no game and no other test ran there.
nix run .#stream-cadence -- view --build-store auto --sender local --sender-fixture --viewer local \
  --viewer-package /nix/store/xc2kqd2lxxv1h1s9dscymjkmwnnqykxq-korri-viewer-0.1.0 \
  --candidate-package /nix/store/zqya4nbg6hwi0z0vj01v4vskmvl52adl-korri-viewer-nightjar-0.1.0 \
  --sender-sunshine /nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri \
  --require-encryption --runs 5 --out DIR
nix run .#stream-cadence -- verdict DIR/baseline/results.json DIR/candidate/results.json
```

- A, the baseline: `korri-viewer` of this branch, the C core
  (`acbefdbd6`, the feature off).
- B, the candidate: `korri-viewer-nightjar` of the same commit, whose
  `bin/moonlight` streams with nightjar-core.
- Both `x11_vaapi`, `decode=vaapi`. All 10 runs valid; every run had
  encryption required, `sunshine-korri` as the sender, no live game, no
  input device change, and a restore that passed. Other work on aka: 0.9 to
  17.4 % of all CPUs (baseline), 0.9 to 16.3 % (candidate). Tctl 62.9 to
  84.2 °C.

Results: `turns-2026-11-17/c.json`, `turns-2026-11-17/nightjar.json`; the
verdict, unedited: `turns-2026-11-17/verdict.txt`.

| ADR 0062 number | Field | C core (5 runs) | nightjar-core (5 runs) | Verdict |
| --- | --- | --- | --- | --- |
| 1 Packet arrival to display | `packet_to_display_ms` | not measured | not measured | not measured: the set had no `--frame-record`, by the measure's method (the record costs CPU; README "The viewer's frame record") |
| 1 (the gate) Sender presentation to display | `latency_p50_ms` | 12.006 to 13.053 | 11.872 to 13.158 | equal |
| | `latency_p95_ms` | 19.196 to 20.340 | 19.108 to 20.614 | equal |
| 2 Late, repeated, dropped | `dropped_frames` | 0 to 1 | 0 | equal |
| | `repeated_frames`, `late_frames` | 229 to 234 | 228 to 235 | equal |
| | `torn_frames`, `unreadable_frames`, `output_repeats` | 0 | 0 | equal |
| 3 CPU | `cpu_seconds_per_second` | 0.011 to 0.020 | 0.011 to 0.020 | equal |
| 3 Battery | `battery_mw` | not measured | not measured | not measured: aka has no battery |
| 4 Startup | `startup_ms` | 240.7 to 273.8 | 144.5 to 225.2 | **better** |
| 5 Input | `input_ms` | 0.051 to 0.077 | 0.056 to 0.083 | equal |
| | `input_missed` | 0 | 0 | equal |
| 6 Stability | `stream_stalls`, `viewer_exits` | 0 | 0 | equal |

The proof's frame record (part 2, SDL software path, not the measure's
number 1) gave enqueue to display p50 2.97 ms (C) and 2.99 ms
(nightjar-core).

**What this set cannot judge** (#470): the private pair streams over a
loopback, so no packet is lost or late. FEC repair, RFI and IDR recovery,
and the timers under delay did not run for either core. Encryption did
(`--require-encryption`). A verdict on loss needs `--net` sets.

Why nightjar-core starts faster is not known: the measure's startup ends at
the first displayed frame, and this set does not show where the C core
spends its extra 15 to 130 ms (run against run).

## 4. Turns under loss and delay (#470)

The private pair's own loopback with stream-cadence's `--net` (its README,
"Network conditions and encryption"), so that FEC, the IDR recovery and
the timers run. Same pair as part 3: aka, five runs of each side in the
order A B A B, at the defaults, Korri's patched Sunshine (`--sender-sunshine
/nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri`),
`--build-store auto`. Each set started only after three checks a minute
apart found aka quiet (1-minute load below 1.5, k10temp Tctl below 78 °C, no
Nix build, no game and no other session's proof), and held
`/var/tmp/korri-turns.lock` on aka while it ran, which the other workers
wait for.

- A: `korri-viewer`, the C core:
  `/nix/store/k497w9l1wb5q0vcjwwh2zx1gm448y73m-korri-viewer-0.1.0`.
- B: `korri-viewer-nightjar`:
  `/nix/store/rf1ra8ncy4vvj7fsap46z13fhk45d0i3-korri-viewer-nightjar-0.1.0`.
- Both built on aka from `origin/main` `124e06840`. Both `x11_vaapi`,
  `decode=vaapi`.

**The counters of each core.** stream-cadence counts the C core's own log
lines in the measured start (`discovered.net.core_log`: unrecoverable
frames, network-dropped frames, RFI and IDR requests). nightjar-core logs
none of those lines, so the tool reads 0 for it; its counters are the two
lines that the viewer logs when that start stops (`logs/nightjar/run-*.log`,
`nightjar: at the stop` and `nightjar: recovery requests sent`). The C core
logs no FEC repair (its "Recovered" line needs `FEC_VERBOSE`); nightjar-core
counts the packets that it rebuilt from parity (`packets_recovered`). Its
`frame_drops` counts each frame that its depacketizer gave up, lost or
skipped while it waited for a key frame, so it is not the C core's
unrecoverable frames.

### 5 % burst loss, encryption required

`--require-encryption --net 'loss gemodel 1.3158% 25%'` (mean loss 5 %,
mean burst 4 packets, UDP only), 2026-11-17 10:39 to 10:55 UTC. All 10 runs
valid; other work on aka 0.9 to 1.2 % of all CPUs; Tctl 58.5 to 83.5 °C.
Two earlier attempts of this set, at 08:09 and 09:01 UTC, ended early and
are not kept: other sessions' builds and tests heated aka to 96 °C, so the
tool's heat guard ended one after its second turn, and one run of the
other had two capture gaps. Record: `net-2026-11-17/loss-burst-5/`
(`c.json`, `nightjar.json`, `verdict.txt` unedited, each run's `viewer.log`).

| ADR 0062 number | Field | C core | nightjar-core | Verdict |
| --- | --- | --- | --- | --- |
| 1 Packet arrival to display | `packet_to_display_ms` | not measured | not measured | not measured (no `--frame-record`) |
| 1 (the gate) Sender presentation to display | `latency_p50_ms` | 12.166 to 12.492 | 12.152 to 12.387 | equal |
| | `latency_p95_ms` | 19.273 to 19.563 | 19.406 to 19.556 | equal |
| 2 Late, repeated, dropped | `dropped_frames` | 527 to 591 | 550 to 603 | equal |
| | `repeated_frames` | 762 to 824 | 784 to 840 | equal |
| | `late_frames` | 379 to 403 | 373 to 407 | equal |
| | `torn_frames`, `unreadable_frames`, `output_repeats` | 0 | 0 | equal |
| 3 CPU | `cpu_seconds_per_second` | 0.011 | 0.010 to 0.011 | equal |
| 3 Battery | `battery_mw` | not measured | not measured | not measured |
| 4 Startup | `startup_ms` | 138.2 to 3840.4 | 137.9 to 298.5 | equal |
| 5 Input | `input_ms` | 0.051 to 0.056 | 0.055 to 0.069 | equal |
| | `input_missed` | 0 | 0 | equal |
| 6 Stability | `stream_stalls`, `viewer_exits` | 0 | 0 | equal |

The C core's one slow start (run 3, 3840 ms) is in its range; with loss on
UDP only, a lost early datagram of the handshake or the first frames is a
likely cause (inferred, not shown).

Each core's recovery in the measured start of each run:

| Run | netem dropped (C / nightjar) | C: unrecoverable frames | C: IDR requests | nightjar: FEC-rebuilt packets | nightjar: frames given up | nightjar: IDR requests |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 5897 / 5955 | 506 | 418 | 1485 | 1783 | 424 |
| 2 | 5771 / 6230 | 463 | 385 | 1524 | 1840 | 432 |
| 3 | 5714 / 6050 | 487 | 395 | 1547 | 1682 | 400 |
| 4 | 6120 / 6080 | 516 | 408 | 1585 | 1706 | 408 |
| 5 | 5970 / 6264 | 485 | 404 | 1651 | 1825 | 432 |

Neither core sent a reference frame invalidation: aka's VA-API H.264
encoder offers none ("Reference frame invalidation is not supported by
this host"), so both recover with IDR frames, 385 to 432 in a measured start
(its warm-up, 60 scored seconds and the input presses). nightjar-core rebuilt 1485 to 1651 packets from parity in each run;
the C core does not log its own. Neither decoder asked for a key frame
itself (nightjar's `frames_need_idr` 0), and no decrypt failed.

### 30 ms delay

`--net 'delay 30ms'` (every packet in both directions, so a 60 ms round
trip), 2026-11-17 11:05 to 11:21 UTC. All 10 runs valid; other work on aka
0.8 to 0.9 % of all CPUs; Tctl at each run's cool-down end 61.5 to
75.9 °C. Record: `net-2026-11-17/delay-30ms/` (`c.json`, `nightjar.json`,
`verdict.txt` unedited, each run's `viewer.log`).

| ADR 0062 number | Field | C core | nightjar-core | Verdict |
| --- | --- | --- | --- | --- |
| 1 Packet arrival to display | `packet_to_display_ms` | not measured | not measured | not measured (no `--frame-record`) |
| 1 (the gate) Sender presentation to display | `latency_p50_ms` | 42.016 to 42.069 | 41.967 to 42.071 | equal |
| | `latency_p95_ms` | 49.206 to 49.287 | 49.190 to 49.293 | equal |
| 2 Late, repeated, dropped | `dropped_frames` | 0 | 0 | equal |
| | `repeated_frames` | 231 to 235 | 232 to 235 | equal |
| | `late_frames` | 231 to 235 | 232 to 235 | equal |
| | `torn_frames`, `unreadable_frames`, `output_repeats` | 0 | 0 | equal |
| 3 CPU | `cpu_seconds_per_second` | 0.011 | 0.011 | equal |
| 3 Battery | `battery_mw` | not measured | not measured | not measured |
| 4 Startup | `startup_ms` | 1759.1 to 1792.0 | 1774.4 to 1775.8 | equal |
| 5 Input | `input_ms` | 30.052 to 30.058 | 34.351 to 35.839 | **worse** |
| | `input_missed` | 0 | 0 | equal |
| 6 Stability | `stream_stalls`, `viewer_exits` | 0 | 0 | equal |

In each run nightjar-core's key presses arrived at a median of 34.35 to
35.84 ms (C: 30.05 to 30.06) and its releases at 40.02 ms (C: 30.04 to
30.05).

**The cause.** nightjar-core's `ControlHandle` wakes its control thread
with a UDP datagram to the control socket's own address
(`vendor/nightjar/protocol/client/src/control/driver.rs`, `Waker`). That
datagram crosses the pair's loopback, so `--net` delays it 30 ms too. A key
then waited for the thread's next idle tick (`MAX_IDLE_WAIT`, 10 ms): 0 to
10 ms more. The C core's input thread waits on an in-process queue
(`InputStream.c`) and sends at once. `tests/nightjar_input_wake.rs` plays
the condition (netem on a namespace's loopback, nightjar-core's control
client, a plain ENet host): 20 keys arrived a median of 5.16 ms and at most
9.65 ms beyond the delay. With the vendored edit in `../../vendor/README.md`
(on Unix the wake also goes through a local socket pair), medians of
0.15 and 0.21 ms and maximums of 0.25 and 0.33 ms in two runs. On a real
network the loopback has no delay, so this cost is mostly the test pair's
(inferred).

**The set again, with the edit.** Same condition and options, 2026-11-17
15:07 to 15:24 UTC. Both packages built on aka from one tree,
`origin/main` `7bbafe964` with the test and the edit above, so the two
sides differ only in the `nightjar` feature:

- A: `/nix/store/sy7shkb4d5hamwk4l891vvwzlih6jpbr-korri-viewer-0.1.0`.
- B: `/nix/store/symzr107bn6qyal5m305hfh6mffqqgm2-korri-viewer-nightjar-0.1.0`.

All 10 runs valid, and equal on every number. Other work started after the
quiet gate and took 1.0 to 17.1 % of all CPUs (more than 5 % in nine of the
ten runs), so the ranges are wider than in the first set. Record:
`net-2026-11-17/delay-30ms-wake/`.

| ADR 0062 number | Field | C core | nightjar-core | Verdict |
| --- | --- | --- | --- | --- |
| 1 (the gate) Sender presentation to display | `latency_p50_ms` | 42.030 to 43.222 | 42.030 to 43.254 | equal |
| | `latency_p95_ms` | 49.266 to 50.526 | 49.139 to 50.570 | equal |
| 2 Late, repeated, dropped | `dropped_frames` | 0 | 0 | equal |
| | `repeated_frames`, `late_frames` | 228 to 239 | 229 to 237 | equal |
| 3 CPU | `cpu_seconds_per_second` | 0.011 to 0.021 | 0.013 to 0.022 | equal |
| 4 Startup | `startup_ms` | 1755.9 to 1792.2 | 1756.6 to 1776.8 | equal |
| 5 Input | `input_ms` | 30.054 to 30.099 | 30.057 to 30.083 | equal |
| | `input_missed` | 0 to 1 | 0 to 2 | equal |
| 6 Stability | `stream_stalls`, `viewer_exits` | 0 | 0 | equal |

nightjar-core's key presses now arrive at a median of 30.06 to 30.08 ms
(was 34.35 to 35.84) and its releases at 30.04 to 30.08 ms (was 40.02).
The missed presses (1 for C, 2 for nightjar-core) are in the same turn,
run 4, with 16.7 and 14.8 % other work.
