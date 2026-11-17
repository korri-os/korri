# #391 — from the tap to the first frame

Two play proofs on aka's emulator on 2026-11-17, one with the start's marks
only and one with the fixes, and one PC private pair with the marks. Each
emulator run started only below 62 °C with a 1-minute load below 2, no
other emulator, and no client on the product's korri-sunshine; both started
at 60.4–60.6 °C. Two earlier starts at 60–63 °C ended in the sender
fixture's own heat refusal (94.5 and 96.6 °C, 15 s after the emulator
booted, while other sessions' builds ran); nothing was measured there.

```sh
# on aka as root, from a copy of the branch; HOME and XDG_STATE_HOME under /var/tmp
nix run .#android-app-play-proof -- --build-store auto \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5690 --adb-port 5191 --out DIR --apk APK
nix run .#stream-cadence -- view --build-store auto --sender local --sender-fixture \
  --viewer local --viewer-package VIEWER \
  --sender-sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --runs 1 --seconds 10 --warmup 2 --window 10 --frame-record --out DIR
```

| Run | Build | Result |
| --- | --- | --- |
| Before, 04:58–05:03 | The fixture APK with the marks only ("each step from Start to the first frame logs its monotonic time") | 97 of 97 checks |
| After, 05:52–05:58 | The fixture APK with the marks, the early decoder and the layers beside the certificate | 72 of 84: the first start and its picture passed; the stream died later (below) |
| PC private pair, 03:24 | `korri-viewer` with the marks only | valid, `startup_ms` 258 |

The builds' store paths are left out. Each was built on aka from a copy of
the branch before it was rebased onto `main`, and the copies differed from
the commits in rustfmt's layout and one lint attribute, so a clean build of
the commits gives other paths. The code that ran is the commits' code on
their earlier base, `0e34272af`. Sunshine's H.264 stream has no reference
frame invalidation, so the core logged the line that marks DESCRIBE's
answer in every run here; on a host with it, that mark is missing.

`before.json` and `after.json` hold each run's failed checks, its
`start_timeline` and its TCP counters (from `result.json`);
`pc/stream-cadence.json` the pair's startup. `tools/capture.sh` captured the TCP headers (and the first 160 bytes) of the
fixture Sunshine's HTTP, HTTPS and RTSP ports on aka's loopback during each
emulator run; `tools/connections.py` and `tools/timeline.py` put each
connection beside the marks (`timeline-before.txt`, `timeline-after.txt`).
Those rows assume that aka's and the emulator's wall clocks agree; they do
not quite (about 0.5 s), so read only their own gaps (SYN to request), which
need no second clock.

## The emulator's timeline, ms after the tap (first start)

The tap's time is the emulator's wall clock just before `input tap` runs;
`input` itself takes part of the first 0.2 s. Every other time is a mark
(`start: … at … ms`, CLOCK_MONOTONIC), anchored to the tap by its logcat
time.

| Step | Before | After |
| --- | --- | --- |
| korrid starts the viewer | 218 | 172 |
| Video layers asked; window turn asked | 1 635; 1 879 | 186; 186 |
| Client certificate read (key made on first use) | 611 | 249 |
| Video layers made | 1 822 | 270 |
| Server certificate provisioned | 1 503 | 970 |
| Start | 1 900 | 1 041 |
| HTTP serverinfo (connected / total) | 2 918 (477 / 977) | 1 862 (304 / 792) |
| HTTPS serverinfo, paired (connected / TLS done / total) | 3 938 (483 / 1 013 / 1 021) | 2 942 (627 / 1 033 / 1 079) |
| applist | 5 055 (543 / 1 076 / 1 117) | 3 968 (504 / 1 006 / 1 016) |
| Decoder made | 14 795, inside the video start | 4 407, beside the launch |
| launch answered (connected / TLS done / first byte) | 6 959 (568 / 1 021 / 1 881) | 5 661 (5 / 511 / 1 691) |
| RTSP DESCRIBE answered | 9 111 | 7 663 |
| RTSP SETUP audio, video, control answered | 9 980; 10 596; 11 831 | 8 689; 9 227; 9 758 |
| RTSP done (ANNOUNCE, PLAY) | 13 398 | 10 982 |
| Control stream started | 13 410 | 11 082 |
| Video stream started | 14 795 | 11 083 |
| First video packet | 14 980 | 11 247 |
| **First frame shown** | **15 373** | **11 512** |

`seconds_from_tap_to_running` (the proof's own clock): 16.6 before, 12.3
after; #495 read 12.4 with 12.8 s to its first picture. One run each: the
same build gives different numbers from run to run (below).

## Beside the PC private pair, ms after Start

Same Sunshine package (`bvz43…`), the same network core and the same marks.
The pair's measured start was a resume (its locating start had launched),
so it had no encoder probe.

| Step | Emulator, before | PC private pair |
| --- | --- | --- |
| HTTP serverinfo | 1 018 | 1.3 |
| HTTPS serverinfo, paired | 2 039 | 3.7 |
| applist | 3 155 | 5.6 |
| Picture and input ready | 3 155 | 76.4 (VA-API) |
| launch / resume answered | 5 059 | 182.4 (Sunshine 106 ms) |
| RTSP DESCRIBE | 7 211 | 184.6 |
| RTSP SETUP audio, video, control | 8 081; 8 696; 9 932 | 184.7; 184.8; 184.9 |
| RTSP done | 11 498 | 185.4 |
| Control stream started | 11 510 | 185.7 |
| Video stream started | 12 895 | 188.2 |
| First video packet | 13 080 | 212.6 |
| First frame shown | 13 473 | 226.8 |

## Each gap's cause

- **Every new TCP connection waits in the emulator, about 0.5 s twice.**
  aka sends each SYN-ACK at once (+0.0 ms), and the emulator's first data
  on that connection reaches aka 168–680 ms later (median 521 ms, 12
  connections of the first start). In the emulator, libcurl reports
  connected after 304–627 ms (once 5 ms), and its first answer about 0.5 s
  after that (TLS done − connected: 406–533 ms; HTTP first byte −
  connected: 488–500 ms), although aka sent that answer within 4 ms. The
  emulator's TCP counters show 0 SYN retransmits from the tap to the first
  frame. 11 connections lie between Start and the first frame (4 HTTP and
  HTTPS, 7 RTSP; Sunshine closes each after its answer) and one more before
  Start (korrid's serverinfo for the host UUID). Without Sunshine's own time
  (the launch's probe, 0.86 s), the 4 HTTP requests took 4.2 s and the 7
  RTSP requests 6.4 s: about 10.6 s of the 13.5 s from Start. The PC pair runs
  the same 7 RTSP requests in 1.1 ms. Verified: the wait lies between aka's
  socket and the emulator's. Inferred: libslirp, the emulator's user-mode
  network, takes a new socket into its poll only at its next turn, and caps
  a turn's wait at 499 ms while TCP timers run and at 1000 ms otherwise
  (`slirp_update_timeout`, `TIMEOUT_SLOW`). #495's 1 s and 0.5 s steps are
  the same. Outside Korri.
- **Sunshine re-probes every encoder at its first launch** (0.78 s: nvenc
  fails, Vulkan fails twice behind fixed 200 ms sleeps, then VA-API;
  `sunshine-before.log` 05:01:11.855–12.633). `probe_encoders()` runs at
  each launch without a session, and on Linux
  `needs_encoder_reenumeration()` returns true at its first call because its
  `last_render_device` starts empty. The proof's Sunshine starts fresh, so
  every proof pays it; the product's pays it once per restart. Outside Korri.
- **The decoder was made inside the core's video start**: 1.39 s here
  (0.28 s in #495). Ours: fixed ("Android's decoder is made while the
  stream connects").
- **The layers waited for the certificate, and the window turned only
  after Start**: 0.19 s and 0.4 s. Ours: fixed ("the layers and the
  window's turn wait for no certificate"); Start now
  comes 71 ms after the certificate instead of 397 ms.
- **The client key is made at the first start after install**: 393 ms
  before, 77 ms after (RSA prime search varies; 1.07 s in a run where it
  shared the 2 cores with the window's turn); 0.9 ms at the second start.
  Ours, not fixed: a follow-up.
- **The certificate's provisioning** (0.72–0.89 s): korrid's serverinfo
  for the host UUID on a new connection (288 ms of emulator wait), then the
  broker's round trip through the holder. Partly the emulator; it now runs
  beside the layers and the turn.
- **#495's 0.5 s on ANNOUNCE**: the fixture's PulseAudio refuses Sunshine's
  three virtual sinks (13 ms in these runs; not in the product's recent
  journal). The fixture's.

Between the two runs, the fixes account for about 1.7 s of the 3.86 s: the
decoder (1.38 s off the path) and the layers and turn (0.33 s). The rest is
run-to-run variance: the key's making (−0.32 s), the certificate's
provisioning (−0.17 s) and the emulator's connection waits (−1.1 s in the
RTSP handshake, −0.4 s in the HTTPS requests). The second start of the before run (korrid's recovery after
Home: no key to make, a resume with no probe) took 9.73 s from Start.

## The after run's failure

The first start and its picture passed. At 05:55:12 the stream lost most of
its packets; at 05:55:30 the core's control stream reported an unexpected
disconnect, and Sunshine logged a ping timeout for the emulator. The stream
ended under the overlay, so its checks, the input, the controls and the
return failed. korrid's recovery then started the stream again and again;
each try's HTTPS handshake got no answer from Sunshine for 10 s. The first
start's launch connection (port 42936) was never closed on aka's side, and
this Sunshine package lacks korri-plugins patch 0045 (send the HTTPS
close_notify without waiting, korri#443): one open connection holds every
later HTTPS client. Both runs lost many packets in the stream (the before
run, which passed: 813 waits for an IDR frame, 201 unrecoverable frames;
those lines are left out of `logcat-app-*.txt`). Nothing in the fixes acts
after the first frame. A second after run was queued; aka stayed at 87–96 °C
with other sessions' work, and it was stopped.

## Not kept

`serverinfo` and `applist` side by side (one connection's wait less) was
built and tested, and the after run with it never reached Running: this
Sunshine finished the second TLS handshake only after the first connection
closed, and libcurl kept the first one open for reuse, so both waited 30 s.
In both runs libcurl also writes each next request into the previous HTTPS
connection after Sunshine's close_notify, then closes it and opens a new
one (the capture: 155 and 362 bytes, unanswered).
