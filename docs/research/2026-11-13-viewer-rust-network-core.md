# Research: which Rust network library for Korri's viewer (step E of korri-os/korri #391)

Date: 2026-11-13. Labels: **verified** (I read the source named), **inferred** (consistent with the sources, not proven), **unknown**, **opinion** (my judgement).

Source: a researcher agent's brief for #391, step E. Its claims were not audited independently; each carries its own label.

## Summary

Three Rust client implementations of the Moonlight protocol exist today. #391 lists two of them. The third is new to #391: `nightjar-core` in `RamazanKara/nightjar`, under GPL-3.0-only. It is the only one that covers everything Korri uses: encrypted video, AV1, HDR, RFI, gamepad input, decode on the receive thread, and per-frame receive times. It has differential tests against moonlight-common-c, and its author published an A/B on a handheld with a "no worse" result (verified). `MrCreativ3001/moonlight-common-rust` (GPL-3.0-or-later, sans-IO) still cannot decrypt video and has no AV1, RFI, receive timestamps or direct submit. Starfire (Apache-2.0) sends no gamepad input, has no Linux backends, and announces no encryption by default (verified). Recommendation (opinion): at step E, vendor a pinned fork of `nightjar-core` behind a Korri trait seam. Measure it against the C core with the same Korri session part and picture code. Contribute fixes upstream. Do not take MCR as the base, and write Korri's own core only for the parts where the fork fails the gate. Before any verdict, the stage-0 measure needs three things it does not have today: packet loss, a fixture Sunshine that encrypts video, and an input-latency number.

## Findings

### 1. The Rust implementations (question 1)

| | `nightjar-core` (Nightjar) | `moonlight-common-rust` (MCR) | Starfire | Korri's baseline: moonlight-common-c `b126e481` |
|---|---|---|---|---|
| Licence | GPL-3.0-only (workspace `Cargo.toml`); repo licence GPL-3.0 — verified | GPL-3.0-or-later — verified | Apache-2.0 — verified | GPL-3.0 — verified (Nightjar and MCR credit it as GPL-3.0) |
| Activity | Repo created 2026-10-07; last push 2026-11-13; Rust engine is the Android default since 2026-10-23 — verified | Created 2026-02-14; last push 2026-10-26 (`a972937`); no release tag; not on crates.io — verified | Created 2026-06-18; last push 2026-11-13 — verified | Pin dated 2025-11-25. Upstream master is 95 commits ahead; last upstream commits 2026-11-11 and 2026-11-13 — verified |
| Authors | `RamazanKara` 3,593 commits; `cgutman` 2,855 (inherited moonlight-android history); `Kaitul` 31; `claude` 24. The first page of `protocol/client` history is all RamazanKara — verified. One author writes the Rust engine — inferred | 316 commits by MrCreativ3001; 1 each by Greaper88, Tom1tk, naari3 — verified | `tim-almond-house` 68, `Ttimmahlax` 4; commits list Claude as co-author — verified | cgutman — verified |
| RTSP (incl. encrypted RTSP) | Yes, incl. encrypted RTSP; RTSP over ENet for GFE < 3.22 not ported — verified (RUST_CORE.md) | Yes, incl. RTSP encryption — verified (README table, code) | Plain RTSP (one TCP connection per request) — verified. Encrypted RTSP: not found — unknown | Yes |
| ENet control | Patched, vendored `rusty_enet`: rounds the RTT smoothing step up as the moonlight ENet fork does, and counts datagrams from another address — verified | Upstream `rusty_enet` 0.4.0 (MIT, transpiled from C), fixed 50 ms service tick (TODO) — verified | Upstream `rusty_enet` — verified | Moonlight's ENet fork |
| Control encryption | Yes (encrypted stream only) — verified | Yes (Sunshine V2 and Nvidia style) — verified | Default announces `encryptionEnabled 0` (plaintext). The doc comment says this is valid only when the host does not require encryption — verified | Yes |
| Video FEC | Yes, difftested against the patched C — verified (doc) | Yes, via `fec-rs` (BSD-2, by moonshine's author) — verified | Own Cauchy RS decoder, checked byte-exact on a captured Sunshine fixture — verified | Yes (nanors) |
| Audio FEC | Yes, difftested — verified (doc) | Yes (off for GFE < 7.1.415) — verified | Unknown (`audio.rs` is 708 bytes) | Yes |
| Video encryption | Yes (`VideoConfig.encryption_key`, `ENC_VIDEO_HEADER`) — verified | **No.** `sunshine_encryption: None, // TODO` at video setup — verified. The SDP code still sets the video flag when the host requests it, so a host in "required" mode would send video that MCR cannot decrypt — inferred | No (session code has no encryption path) — verified for `session.rs` | Yes |
| Audio encryption | Yes — verified | Yes — verified | No (default 0) — verified | Yes |
| Codecs | H.264, HEVC, AV1, 10-bit, 4:4:4 — verified | H.264, HEVC; **no AV1** — verified | AV1, HEVC, H.264 claimed — verified (code enum) | All |
| HDR | HDR mode and `SS_HDR_METADATA` — verified | HDR mode packet and metadata types present — verified | HDR10 passthrough claimed in README — not checked in code (unknown) | Yes |
| RFI | Yes — verified | No — verified (README) | Unknown | Yes |
| Input | Keyboard, mouse, controllers, touch, pen, motion, battery; JNI parity test against the C natives — verified (doc) | Keyboard, mouse, gamepad, touch, pen, controller touch, motion — verified (packet types) | Keyboard and mouse only in `starfire-core/src/input.rs` and `starfire-input`, although the README claims gamepad — verified | All |
| Tests | Unit tests; `nightjar-difftest` compiles the patched C receive paths and compares them on the same packet sequences, with loss, reorder and duplicates; loopback session tests; interop against Rubylight's host code (`butterpollo-core`); e2e with `nightjar-cli` — verified (docs, `host_interop.rs`). No interop test against Sunshine found — unknown | Unit tests against its own payloader (round trip). CI runs fmt, clippy and `cargo test` per feature set. No test against the C core or a live host found — verified (CI file, file list) | Fixture tests from captured Sunshine traffic — verified | Upstream |
| Timing model | Each module is a pure state machine fed with "each datagram with its receive time". `Session` owns threads as the C does (VideoRecv, VideoDec unless direct submit, audio, control). Session clock `now_us()` stamps receive and enqueue times. `recvmmsg` with UDP GRO on the video socket. SO_PRIORITY/DSCP, nice levels. 2048-packet receive buffer, or about 60 ms at the bitrate — verified | Sans-IO with explicit `Instant` (`sans-io-time`) and `poll_timeout`. The std driver runs 3 threads per UDP stream (send, receive, timeout) around one `Mutex` and `Condvar`. One `recv_from` per datagram into a 4096-byte buffer. Frame timeout 100 ms, IDR request every 1 s, stall at 2 s. Frames leave as events, not by direct submit. `receive_time` and `enqueue_time` are commented-out TODOs — verified | `FrameMeta { first_packet_at, complete_at }` (std `Instant`); non-blocking ENet polling with 2 ms sleeps while it connects — verified | Thread per stream, blocking receive, `receiveTimeUs`, `enqueueTimeUs`, `presentationTimeUs` per decode unit, VideoPing thread every 500 ms — verified |

Findings with sources:

1. **Claim:** `nightjar-core` is a full Rust port of moonlight-common-c. It is shipped as the Android default (since 2026-10-23), and its author measured it no worse than the C core on a Retroid Pocket 6. Results: HEVC 1080p60 at 40 Mbps, 4 runs of 75 s each (ABBA order), median picture age 7.04 ms (C) against 6.92 ms (Rust), decode 0.92 ms on both; 1% and 5% injected loss with no extra skipped frames; 30-minute soak with flat memory. **Sources:** [RUST_CORE.md](https://github.com/RamazanKara/nightjar/blob/main/internal/RUST_CORE.md), [PARITY-MATRIX.md](https://github.com/RamazanKara/nightjar/blob/main/docs/PARITY-MATRIX.md). **Support:** direct evidence (the author's own measurement; the host was Rubylight on Windows, not Sunshine). **Confidence:** medium.
2. **Claim:** The port had real timing differences from the C core, and users found them only on harder links. (a) The ENet RTT smoothing truncated instead of rounding up, so the overlay showed an RTT of 7–8 ms on a 2 ms link. The fix was a vendored `rusty_enet`. (b) Controller states were sent in batches of about 20 every 60–90 ms on a 55–75 ms mobile link, because the Rust engine pushed every queued state into ENet's reliable window. The C core merges newer states into the packet that is still queued. Reported 2026-11-11, fixed in 5.0 on 2026-11-12. **Sources:** [PARITY-MATRIX.md](https://github.com/RamazanKara/nightjar/blob/main/docs/PARITY-MATRIX.md), [nightjar #97](https://github.com/RamazanKara/nightjar/issues/97). **Support:** direct evidence. **Confidence:** high. Inference: MCR uses upstream `rusty_enet` 0.4.0, so it probably has the same RTT-smoothing difference.
3. **Claim:** Using `nightjar-core` from outside Nightjar has costs. It is not on crates.io. It depends by git on `rubylight-native-api` (pinned revision) and on `rubylight-traverse` from the same workspace, plus `zbus` on Linux and `ring`/`rustls`. It also carries Rubylight-only features (PyroWave, 0x55xx messages, USB/IP, roaming). The Rubylight licence is GPL-3.0. **Sources:** [protocol/client/Cargo.toml](https://github.com/RamazanKara/nightjar/blob/main/protocol/client/Cargo.toml), [crates.io nightjar-core](https://crates.io/api/v1/crates/nightjar-core), [Rubylight licence](https://api.github.com/repos/RamazanKara/Rubylight/license). **Support:** direct evidence. **Confidence:** high.
4. **Claim:** Nightjar changes fast and has one main author. The reporter of #97 ran version 4.12.9 on 2026-11-11, and the maintainer said 5.0 fixed the bug on 2026-11-12. The repo shows 3,593 commits by one account. **Sources:** [nightjar #97](https://github.com/RamazanKara/nightjar/issues/97), [contributors API](https://api.github.com/repos/RamazanKara/nightjar/contributors). **Support:** direct evidence. **Confidence:** high.
5. **Claim:** MCR has no video decryption, AV1, RFI, receive timestamps or direct submit. It builds the client SDP with `StreamingConfig::Local` whatever the remote setting is. **Sources:** [MCR `src/stream/proto/mod.rs`, `video/mod.rs`, `video.rs`, README](https://github.com/MrCreativ3001/moonlight-common-rust). **Support:** direct evidence (code). The effect of the hard-coded `Local` is inferred. **Confidence:** high.
6. **Claim:** Starfire does not fit Korri. It is built for Windows and macOS only (VideoToolbox; Media Foundation "planned"), encodes keyboard and mouse only, and announces plaintext streams by default. **Sources:** [starfire](https://github.com/Remade-With-Rust/starfire), `crates/starfire-core/src/{input,rtsp,control}.rs`, `crates/starfire-decode/src/backend/mod.rs`. **Support:** direct evidence. **Confidence:** high.
7. **Claim:** No other Rust Moonlight *client* core was found. `hgaiser/moonshine` (BSD-2, active), `DanielAdolfsson/solstice` and `r4nt/redfog` are servers. `RamazanKara/Rubylight` is a host. The crates.io searches "moonlight", "gamestream" and "sunshine stream" found no client crate. I also searched GitHub for `moonlight`, `gamestream` and `sunshine client` in Rust repositories, and searched the web. I could read only part of the GitHub search results (79 hits for `moonlight`), so a client in the unread part could exist. **Sources:** [moonshine API](https://api.github.com/repos/hgaiser/moonshine), crates.io search API, GitHub search API. **Support:** direct evidence for the searches; an exhaustive result is inferred. **Confidence:** medium.

### 2. What Sunshine requires, and what Korri uses (question 2)

Sunshine: the latest stable release is `v2026.914.233613`, the latest prerelease is `v2026.1113.11031`, and the repo has moved to `SunshineStream/Sunshine`. The release notes for 914 list no client protocol change (verified).

| Requirement (Sunshine `master` source) | Status | Korri uses it? |
|---|---|---|
| HTTPS with a paired client certificate; `launch` throws when `rikey` is missing; `rikeyid` builds the IV — verified `nvhttp.cpp` | Required | Yes. Korri's Rust session sends `rikey` and `rikeyid` — verified `stream.rs` |
| `corever >= 1` turns on encrypted RTSP (`rtspenc://`) — verified `nvhttp.cpp` | Optional | Korri appends the C core's launch parameters. That they contain `corever=1` is inferred: MCR mirrors them as `&corever=1` |
| DESCRIBE always advertises `CONTROL_V2` and `AUDIO` encryption and always requests `CONTROL_V2`. It adds `VIDEO` unless the address's mode is NEVER. MANDATORY mode requests `VIDEO\|AUDIO`, and ANNOUNCE returns **403** when the client does not enable both — verified `rtsp.cpp` | Video and audio encryption are required only in "required" mode | Korri passes `ENCFLG_ALL` on x86 with AES, else `ENCFLG_AUDIO` — verified `stream.rs` |
| Defaults: `lan_encryption_mode = NEVER`, `wan_encryption_mode = OPPORTUNISTIC`, `ping_timeout = 10s`, `fecPercentage = 20` — verified `config.cpp` | — | The `@korri:sunshine` plugin exposes `network.lan-encryption` and `network.wan-encryption` with no default, so Sunshine's defaults apply unless a user sets them — verified `settings.ts`. Values on the devices: unknown |
| With the encrypted control protocol (type 13), Sunshine drops unencrypted messages — verified `stream.cpp` | Required once negotiated | Yes |
| ENet control; `ML_FF_SESSION_ID_V1` matches the session by connect data, else by IP — verified `stream.cpp` | Required | Yes, through the C core |
| Every ENet event refreshes the ping deadline. With no event for `ping_timeout` (10 s), Sunshine stops the session ("Ping Timeout"). Video and audio need the UDP ping payload first ("Initial Ping Timeout") — verified `stream.cpp` | Required | Yes |
| AV1 request when AV1 is off → 400 — verified `rtsp.cpp` | — | Korri offers `av1` as a codec choice; the default is `h264` — verified plugin `settings.ts` |
| New extension `0x5504` (player LEDs) in `master` — verified | Optional | No |

What Korri uses (verified unless stated):

- Codec `auto/h264/h265/av1`, default `h264`. HDR (`hdrMode=1`, Main10 for HEVC or AV1). Resolution, fps, bitrate, packet size (1392), remote optimisations, SOPS, play-on-host, stereo/5.1/7.1 — plugin `settings.ts`, viewer `options.rs` and `stream.rs`.
- Decode on the receive thread: Korri's VA-API and SDL callbacks set `CAPABILITY_DIRECT_SUBMIT`. The SDL and x11 software paths also advertise HEVC RFI — `picture/video/{x11,sdl}.c`.
- Product viewer (Moonlight Embedded): audio and controller input. Rust stage-1 viewer: no input, and audio goes to null callbacks — viewer `README.md`.
- Not used: microphone, PyroWave, Rubylight or Apollo extensions on Linux. Touch and pen: unknown.
- When korrid forwards a stream over iroh through local listeners, whether Sunshine classes the peer as LAN (and so turns video encryption off): unknown.

### 3. Options (question 3)

| Option | Korri's up-front work | Ongoing cost | Main risks |
|---|---|---|---|
| A. Adopt MCR and contribute | Write video decryption, AV1, RFI, receive and enqueue timestamps, a direct-submit path and ENet-fork parity. Get them through one maintainer's review | Low if upstream merges; high if not | It has the most missing features. Its tests check its own payloader against its own depayloader, not against the C core or a host. There has been no push since 2026-10-26 |
| B. Adopt `nightjar-core` as a git dependency | A thin adapter from Korri's session part to `Session::start` (it has the shape of `LiStartConnection`); build the Rubylight git crates and `zbus` in Nix | Track a fast upstream with many Rubylight-only features | API churn; one maintainer; Rubylight coupling; Sunshine-specific evidence not found |
| C. Fork `nightjar-core` (pinned, vendored) | B, plus removing PyroWave, Rubylight, USB and traverse behind features or by deletion; keep `difftest` | Korri owns fixes and picks upstream fixes by hand | Divergence. GPL-3.0-only fixes the combined binary at GPL-3.0 (compatible with ADR 0059's "GPL-3", inferred) |
| D. Korri's own sans-IO core | A full port: RTSP/SDP, ENet integration, video and audio FEC, crypto, input batching, control, plus difftests. MCR and Nightjar are GPL references | Full ownership | The highest timing risk. Finding 2 shows two regressions that took field users to find |

**Recommendation (opinion):** C first, run as B to start. Pin a fork of `nightjar-core` in `korri-os`, put it behind a Korri `NetworkCore` trait that the current C binding also implements, and run the gate below. Upstream what Korri fixes. Keep MCR as design reference for a sans-IO boundary only. Use D only for a part that the fork fails and cannot fix.

- **Why:** It is the only candidate whose feature set covers what Korri uses (table above). It has a differential harness against the C core and a published device A/B.
- **What it costs:** Korri inherits the maintenance of a single-author, fast-moving codebase that is coupled to Rubylight. The session layer owns sockets and threads, so Korri does not get full sans-IO control of timing. The licence becomes GPL-3.0-only.
- **What it does not solve:** It gives no picture path and no handheld decode.
- **What could still go wrong:** Sunshine behaviour that Rubylight does not show, such as encryption modes or `0x5504`. Regressions upstream between pins.

### 4. Parity parts and how stage 0 shows them (question 4)

To isolate the network library, compare two packages that differ **only** in the core: Korri's Rust viewer with the C binding against the same viewer with the Rust core, with the same picture code. Run `view --sender root@aka --sender-fixture --viewer root@aka --viewer-package A --candidate-package B --runs 5`, once for base and once with `--queue-measure`, then `verdict`. "Equal" means inside the run-to-run spread (#391, stream-cadence README). The current aka baselines (2026-11-13) are the product's moonlight-embedded build with VA-API: base p50 11.78–12.16 ms, queue p50 40.11–40.97 ms, viewer CPU 0.011 CPU-s/s, startup 753–801 ms (verified). `verdict` refuses a decode difference, so the candidate also needs VA-API (verified).

| Part | Parity means | Stage-0 number that shows it | What today's measure misses |
|---|---|---|---|
| Video receive: socket, decrypt, RTP reorder, FEC, depacketize, direct submit | Same delay and drops at equal CPU | 1 (gated as `latency_ms` p50 and p95), queue-measure delay, 2 (dropped/late), 3 (CPU s/s, which is very sensitive at 0.011) | A loopback private pair loses no packets, so FEC recovery never runs (inferred). The fixture Sunshine runs at defaults, so on LAN video is not encrypted (inferred from `config.cpp` defaults) |
| RTSP, ENet connect, first IDR | Same time to first frame | 4 `startup_ms` | — |
| Control: periodic ping, IDR/RFI, loss stats, termination | No stalls, no ping timeouts, same recovery | 6 (stalls, exits) | Recovery only under loss |
| Input batching over ENet | Same press-to-host delay at real RTTs | 5 `input_ms` — **not built** (verified) | The Nightjar #97 class of bug shows only at 55–75 ms RTT |
| Audio receive, FEC, decrypt | No added CPU or drops | 3 only | Sound is null in the fixture; audio FEC is not exercised |
| Per-frame receive time | The core exposes it, so number 1 (packet to display) can exist | Today "not measured" (verified) | MCR has no receive time; `nightjar-core` has one |

Gaps to close before a verdict (opinion):
1. `netem` loss and delay inside the pair's own network namespace (for example 1% and 5%, and 60 ms RTT), as Nightjar did on its host.
2. A fixture Sunshine with `lan_encryption_mode=2`, so video and audio are encrypted.
3. A press-to-packet input measure.

The `run` path's `LD_PRELOAD` shim reads the C core's `receiveTimeUs`, so a Rust core needs an equal hook (inferred).

## Contradictions

- **#391 vs a candidate it missed:** #391 lists only MCR and Starfire. `nightjar-core` exists, ships, and covers more. #391 also says "No Rust program does hardware video decode on Linux ARM today". Nightjar's Rust desktop ships aarch64 Linux packages that decode through FFmpeg's V4L2 M2M. Its README says the 10-bit P010 path was tested with a fake decoder, so a run on real ARM hardware is unknown. A native Rust decoder still does not exist (inferred).
- **Starfire README vs code:** The README claims gamepad input and encrypted control. The core input encoder has keyboard and mouse only, and the default announce is `encryptionEnabled: 0`.
- **MCR commit count:** #391 says 321 commits, 316 by one author. The contributors API attributes 319 (316 + 1 + 1 + 1). This is minor.

## Missing evidence

- Any run of `nightjar-core` or MCR against Sunshine `v2026.914` on Linux x86. Nightjar's evidence is against Rubylight.
- The CPU cost of each Rust core against the C core on x86 Linux.
- Korri's deployed Sunshine `lan_`/`wan_encryption_mode` values, and how Sunshine classes iroh-forwarded peers.
- The size (lines of code) of each core and of the work options A and D need: not measured.
- Whether the Nightjar maintainer would accept a Sunshine-only feature boundary: not asked. That would be an outside contact, so the parent decides.

## Sources

- Kept: [Epic: one Korri viewer in Rust… #391 (API)](https://api.github.com/repos/korri-os/korri/issues/391) and [its comments](https://api.github.com/repos/korri-os/korri/issues/391/comments) — stages, step E, earlier findings — fetched
- Kept: local `clients/viewer/{Cargo.toml,build.rs,README.md,src/{main,lib,options,stream}.rs,picture/{bridge.c,bridge.h,video/x11.c,video/sdl.c}}`, `nix/stream-cadence/README.md`, `korri-plugins/plugins/{moonlight,sunshine}/{plugin,settings}.ts` — what Korri uses, the measure, the baselines — read
- Kept: [MrCreativ3001/moonlight-common-rust](https://github.com/MrCreativ3001/moonlight-common-rust) (cloned: `Cargo.toml`, CI, `proto/mod.rs`, `video/mod.rs`, `frame.rs`, `video.rs`, `fec.rs`, `enet.rs`, `control/packet.rs`, `std/driver.rs`), [repo API](https://api.github.com/repos/MrCreativ3001/moonlight-common-rust), [contributors](https://api.github.com/repos/MrCreativ3001/moonlight-common-rust/contributors), [commits](https://github.com/MrCreativ3001/moonlight-common-rust/commits/master) — fetched
- Kept: [RamazanKara/nightjar](https://github.com/RamazanKara/nightjar) (cloned: `internal/RUST_CORE.md`, `docs/PARITY-MATRIX.md`, `protocol/{README.md,Cargo.toml,core/*,client/Cargo.toml,client/src/{lib,crypto,video/mod,session/mod}.rs}`), [desktop README](https://raw.githubusercontent.com/RamazanKara/nightjar/main/protocol/desktop/README.md), [host_interop.rs](https://raw.githubusercontent.com/RamazanKara/nightjar/main/protocol/client/tests/host_interop.rs), [issue #97](https://github.com/RamazanKara/nightjar/issues/97), [protocol/client history](https://github.com/RamazanKara/nightjar/commits/main/protocol/client), [repo API](https://api.github.com/repos/RamazanKara/nightjar), [contributors](https://api.github.com/repos/RamazanKara/nightjar/contributors) — fetched
- Kept: [Rubylight licence](https://api.github.com/repos/RamazanKara/Rubylight/license) — GPL-3.0 — fetched
- Kept: [Remade-With-Rust/starfire](https://github.com/Remade-With-Rust/starfire) (cloned: `control.rs`, `input.rs`, `rtsp.rs`, `video.rs`, `starfire-input`, `docs/protocol/07`), [session.rs](https://raw.githubusercontent.com/Remade-With-Rust/starfire/main/crates/starfire-core/src/session.rs), [decode backends](https://raw.githubusercontent.com/Remade-With-Rust/starfire/main/crates/starfire-decode/src/backend/mod.rs), [contributors](https://api.github.com/repos/Remade-With-Rust/starfire/contributors), [commits](https://github.com/Remade-With-Rust/starfire/commits/main) — fetched
- Kept: Sunshine [`src/rtsp.cpp`](https://raw.githubusercontent.com/SunshineStream/Sunshine/master/src/rtsp.cpp), [`src/config.cpp`](https://raw.githubusercontent.com/SunshineStream/Sunshine/master/src/config.cpp), [`src/stream.cpp`](https://raw.githubusercontent.com/SunshineStream/Sunshine/master/src/stream.cpp), [`src/nvhttp.cpp`](https://raw.githubusercontent.com/SunshineStream/Sunshine/master/src/nvhttp.cpp), [Release v2026.914.233613](https://github.com/SunshineStream/Sunshine/releases/tag/v2026.914.233613), [releases API](https://api.github.com/repos/LizardByte/Sunshine/releases?per_page=15) — fetched
- Kept: moonlight-common-c [`Limelight.h` @b126e481](https://raw.githubusercontent.com/moonlight-stream/moonlight-common-c/b126e481a195fdc7152d211def17190e3434bcce/src/Limelight.h), [`VideoStream.c` @b126e481](https://raw.githubusercontent.com/moonlight-stream/moonlight-common-c/b126e481a195fdc7152d211def17190e3434bcce/src/VideoStream.c), [`Misc.c` @b126e481](https://raw.githubusercontent.com/moonlight-stream/moonlight-common-c/b126e481a195fdc7152d211def17190e3434bcce/src/Misc.c), [commit b126e481](https://api.github.com/repos/moonlight-stream/moonlight-common-c/commits/b126e481a195fdc7152d211def17190e3434bcce), [compare b126e481…master](https://api.github.com/repos/moonlight-stream/moonlight-common-c/compare/b126e481a195fdc7152d211def17190e3434bcce...master), [commits](https://github.com/moonlight-stream/moonlight-common-c/commits/master) — fetched
- Kept: crates.io [fec-rs](https://crates.io/api/v1/crates/fec-rs), [rusty_enet](https://crates.io/api/v1/crates/rusty_enet), [nightjar-core (absent)](https://crates.io/api/v1/crates/nightjar-core), [moonlight-common (absent)](https://crates.io/api/v1/crates/moonlight-common), and search APIs — fetched
- Kept: [hgaiser/moonshine API](https://api.github.com/repos/hgaiser/moonshine) — a Rust server, BSD-2 — fetched
- Rejected/deprioritised: [MATA Starfire pages](https://mata.network/remade-with-rust/starfire/) — marketing; search only. [Reddit r/MoonlightStreaming](https://www.reddit.com/r/MoonlightStreaming/top/) — used only as a lead to Nightjar; search only. [newreleases.io Nightjar v3.0.0](https://newreleases.io/project/github/RamazanKara/nightjar/release/v3.0.0) — secondary; the repo docs hold the same numbers; search only. [DanielAdolfsson/solstice](https://github.com/DanielAdolfsson/solstice) — a server proxy, not a client; search only. MCR forks (Ficik, hachimi-cat, s3777091, …) — copies; search only.

## Next steps

1. Spike (opinion): add a `NetworkCore` trait seam in `clients/viewer`. Implement it over a pinned `nightjar-core` fork. Run base and queue in turns on aka against the C-binding build of the same viewer.
2. Extend `nix/stream-cadence` with: `netem` loss and RTT in the pair namespace; a fixture Sunshine with mandatory encryption; a press-to-packet input number; a receive-time hook that does not depend on the C `DECODE_UNIT`.
3. Re-run the comparison on Sunshine `v2026.914` (and the current prerelease) before you choose.

Suggested issues (the parent files them): (a) stream-cadence variants for loss, RTT and mandatory encryption, and an input measure; (b) viewer `NetworkCore` seam and a `nightjar-core` spike for step E; (c) update #391's evidence with the Nightjar candidate and the ARM-decode correction.

Raised, not asked:
- Korri's C baseline `b126e481` is from 2025-11-25 and 95 commits behind upstream. Upstream changed ENet input batching on 2026-11-11. The C baseline that a Rust core must match is therefore old (medium importance).
- `clients/viewer/src/stream.rs` uses `std::arch::is_x86_feature_detected!`. To my knowledge that macro exists only on x86 targets, so the viewer probably will not compile for aarch64 handhelds. This is inferred, not built. It is low importance now and matters at step G.
