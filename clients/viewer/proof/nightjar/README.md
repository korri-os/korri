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
