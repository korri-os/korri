# #476 slice 4 — a new picture size in the middle of the stream, on the Android emulator

Run on 2026-11-15 at 09:32 to 09:35 (aka local time) from the tree of
commit `435977d72f88116d1e1d0a62d8663f385c7e884b` (`git archive`), on aka:

```sh
XDG_STATE_HOME=/var/tmp/resize-decoder/state nix run path:.#viewer-android-proof -- \
  --sender local --stream-host 100.117.97.45 --out /var/tmp/resize-decoder/proof-12 --port 5596
```

No person took part. Heavy work no longer runs on zao (owner rule,
2026-11-15), so aka built everything, ran the emulator and was the sender.
The emulator was Android 12 (API 31), google_apis, x86_64, headless, 2
cores, `nice` 10, with its own adb server and its sound to a WAV file; no
other emulator ran on aka at its start. The sender was stream-cadence's
private Sunshine fixture on aka itself (`--sender local`), which the
emulator reached at aka's Tailscale address `100.117.97.45`, port 57093,
streaming 1280x720 at 60 fps with its 1000 Hz tone and its input log. Its
Sunshine was `sunshineFollowingOutput`: nixpkgs' Sunshine 2026.516.143833
with `nix/stream-cadence/sunshine-follows-output.patch`, which encodes at
the hidden output's size. The fixture ran beside the product and touched
no display, no product Sunshine and no input; its restore check passed.
The emulator was stopped at the end; no `qemu-system` process of this run
was left.

The heat guard refused seven starts before this one, each before anything
started: another session's game test held aka near 90 °C, and its package
sensor (Tctl) read 85 °C or more at the guard's read, seconds after the
waiting script had read 77 to 79 °C.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/4dpybywsh2iwcbpn0zmk2kwxcc4kbw2z-korri-viewer-android-apk-0.1.0` |
| Refusal probe (unsigned, test only) | `/nix/store/00fgx992pnszmr7i0vkmvw9ldjssngkk-korri-viewer-refusal-probe-apk-0.1.0` |
| Linux viewer, for pairing | `/nix/store/x04xv34j06lqjj2yn5sb6d5d54sbcqia-korri-viewer-0.1.0` |
| Android tests (x86_64) | `/nix/store/a5gxmna7yk0b90cm3pczpfjlfbswhmn6-korri-viewer-android-check` |

## What the proof did

D5's run (`../android-d5/`), with one step after the first four
screenshots: the proof asked the sender's agent for `resize` to 720x1280.
The hidden output took the mode 720x1280 at 58.823 Hz, and the frame-ID
source started again at that size, so its IDs started again at 1.
Sunshine's capture started again on the new output size, and its new
encode session began with an IDR frame and an SPS of 720x1280. The input
(with one tap off the picture's centre), the audio server's restart, 15 s
of streaming, the overlay (Guide, then Back) and Home followed at the new
size.

## The size change, step by step

| Time | Where | What |
| --- | --- | --- |
| 09:33:34.877 | `sunshine.log` | `Fixture: the stream follows the output: 1280x720 to 720x1280` |
| 52.703 s | `logcat.txt` | `picture: the stream's size changes from 1280x720 to 720x1280 at frame 599; the decoder follows it (adaptive playback up to 2960x2960)` |
| 52.758 s | `logcat.txt` | `picture: decoded size 720x1280` |
| 52.761 s | `logcat.txt` | `picture: video layer 720x1280 placed at (896, 0)-(1504, 1080) in the window 2400x1080` |

The emulator's decoder, `c2.goldfish.h264.decoder`, has
`FEATURE_AdaptivePlayback`; the largest square it supports at 60 fps is
2960x2960. So it took the new SPS by itself, and the codec did not
restart. 55 ms passed from the SPS to the first decoded frame of the new
size.

## Checks

All 80 passed (`result.json`). The checks of D1 to D5 are the same as in
`../android-d5/`. `frame_record_ids_move` means that the IDs never go back
within a picture size; a picture that the sender encoded twice is listed,
not failed. The new ones:

| Check | Evidence |
| --- | --- |
| The pictures before the change have the stream's shape | `streaming-1.png` to `streaming-4.png`: IDs 2975, 3074, 3167, 3261; the bar's cell is 48.0 pixels wide at y 270, as a 1280x720 picture in (240, 0)-(2160, 1080) puts it (32 × 1.5) |
| The sender's output took the new size | `result.json`, `resize.sender_display`: 720x1280 at 58823 mHz |
| The decoder saw the new size, and gave it | `logcat.txt`, above |
| No decoder failure | no `the decoder failed` line; `picture_losses`: no IDR frame asked for by the decoder, no frame not decoded, no queue overflow, no frame that the network dropped |
| The layer took the new shape | the logged placement equals the app's rule for 720x1280 in 2400x1080: (896, 0)-(1504, 1080) |
| The bar reads at the new size | `resized-1.png` to `resized-4.png`, a second apart: IDs 99, 187, 274, 376 |
| The screenshots show the new shape | in each, the bar's cell is 15.0 pixels wide at x 927 and y 275: a 720x1280 picture in the new letterbox puts it 15.2 wide at y 270 (18 × 608/720). In the old placement it would be 48 wide |
| The off-centre tap arrived where the new picture puts it | `sunshine-input.json`: one touch down at (0.25, 0.75), from a tap a quarter across and three quarters down the new rect. In the old rect the same screen point is at x 0.42 |
| The input maps into the new picture | the mouse's reference is 608x1080, the new picture's size on the screen; 38 touches at the picture's middle |
| The picture moves on after the input, at the new size | `after-input-1.png` to `after-input-3.png`: IDs 1238, 1335, 1453 |
| The overlay works over the new picture | Guide opened it at 100.694 s and closed it 7.3 s later (it took 12 keys and 6 motion events); Back opened and closed it; every overlay check passed, the video moving under it and the input held at the sender among them (`overlay-*.png`, `result.json`, `overlay_at_the_sender`) |
| The frame record goes on at the new size | `times.csv`: 4377 rows in index order; 4375 decoded, 4294 handed off. Decoded frame by frame at each frame's own size: 586 handed-off frames at 1280x720 (IDs 2758 to 3354) and then 3708 at 720x1280 (IDs 4 to 3780), each run never going back |
| Sunshine encoded at the new size | `sunshine.log`, above |

The frame record's one repeat: units 596 and 597 both carry ID 3354, the
last picture at 1280x720, handed off at 52.659 s
(`frame_record.sizes[0].repeated`). Sunshine encoded it twice as its
capture started again; the viewer cannot make a unit. Run 8 on ibuki
(below) showed the same at its switch.

D1 to D5's checks held with the size change: the tone played (70.9 s at
1000 Hz in the emulator's WAV file), the sound went on after Android's
audio server restarted, every key, touch, mouse and pad packet arrived,
the core's input gate closed before the core stopped (119.599 s), and Home
ended the stream with no crash. The emulator's decoder service did not
crash: `logcat` has no `removePts` and no Codec2 component death.

## The run before this one: the emulator's decoder service crashed

`decoder-crash/`, from the tree of commit
`39a5d474848d69315c114cd1033fd19f63fd2dd5`, on aka at 09:00 to 09:03 the
same morning, with the same command (`--out .../proof-11`). The viewer's
code was the same as in the run above; only the record's README differs.

**It failed, and not at the size change.** 33 checks ran; 32 passed, and
the run ended in D5's overlay step (`decoder-crash/result.json`):

| What | Evidence (`decoder-crash/`) |
| --- | --- |
| Every resize check passed | `result.json`: the sender's output took 720x1280; `the stream's size changes from 1280x720 to 720x1280 at frame 1040; the decoder follows it (adaptive playback up to 2960x2960)` at 61.789 s; `decoded size 720x1280` and the layer at (896, 0)-(1504, 1080) at 61.852 s (`logcat.txt`) |
| The pictures | `streaming-4.png` ID 3492 with the 1280x720 bar geometry; `resized-1.png` and `resized-4.png` IDs 103 and 399 (four shots: 103, 190, 283, 399), cell 15.0 where 15.2 is expected |
| The off-centre tap reached the new rect | `sunshine-input.json`: a touch down and up at (0.25, 0.75), 09:01:58; the mouse's reference 607x1079, the new 608x1080 less one. The run ended before `run.py` scored it |
| The emulator's decoder service crashed | `logcat.txt`, 82.356 s, 20.6 s after the size change, while the input step's game pad ran: SIGSEGV in `/vendor/bin/hw/android.hardware.media.c2@1.0-service-goldfish`, `C2GoldfishAvcDec::removePts(unsigned long)+401`; `Codec2 component "c2.goldfish.h264.decoder" died` |
| The viewer then ended the stream, as designed | `output failed` and `no input buffer` for 3 s, then `picture: the decoder failed; the stream ends`; the input gate closed first, and `viewer failed: picture or input event loop failed` (D2's rule: no flush, restart or reset after a decoder error yet) |
| What failed after it | the check `picture_moves_after_input` (`after-input-1.png` ID 1139, `after-input-2.png` with no picture), and the overlay's open on Guide, which timed out and ended the run |

Of the runs with adaptive playback up to 2960x2960 that streamed past the
size change (4, 6 and 8 below, the crash run and the run above), this is
the one decoder service crash; D1 to D5's records, without adaptive
playback, show none. Run 7's crash was the emulator's own process, also on
its color buffers (`Failed to find ColorBuffer`; the crash run's
`emulator.log` has `bad color buffer handle 237`). Whether adaptive
playback up to 2960x2960, or the goldfish decoder's handling of the new
SPS, makes this crash likelier is not known from one crash in five runs.
A viewer that recovers a dead codec (flush, restart or reset, as Artemis
does) would have kept the stream going; that is a separate issue.

## The runs before the review

On zao and ibuki, before the branch was rebased onto D5 (`df9911d3f`) and
before the review's fixes. Run 8 passed all 62 of its checks: on ibuki,
from commit `78ef93586` of the branch before its rebase; its files are in
this directory at commit `bb8af6091`. The others are not kept; the
viewer's code was the same in all of them, and `run.py` changed between
them as the table says.

| Run | What happened |
| --- | --- |
| 1 | The sender's agent ended its session at the Android tests: 80 s without a request (the emulator's start and 104 tests), and the agent's limit is 75 s. `run.py` now keeps it alive from a thread (`0b5760e35`). |
| 2 | zao's load was 18 to 20. The emulator at `nice` 10 lost most packets (`Network dropped …`, `Unrecoverable frame …`), the stream ended after 15 s (`stream terminated: -1`), and an Android sound test played too little. |
| 3 | aka was hot (91.4 °C); the fixture refused to start. |
| 4 | On ibuki: every check passed but `frame_record_ids_move`, which required strictly rising IDs; the sender's repeat at the switch failed it. The check now fails a step back and lists a repeat (now in `c20ed9fa8`). |
| 6 | 34 checks passed up to Back; then `input keyevent KEYCODE_BACK` failed while another tool on zao installed `com.simonwjackson.korri.debug.test` into this emulator through the shared adb server. The emulator now has its own adb server (`35a2747ae`). |
| 7 | The emulator process crashed 5 s into the stream, before the size change (crashpad's `read out of range` in `emulator.log`), with zao's 5-minute load at 16 after a peak of 76; the stream had been losing frames since its start. |
| 8 | Passed, 62 of 62 (above). |

## Not proven here

- The restart path on a device. Every decoder here had adaptive playback;
  the restart (stop, configure, start), its capability question and the
  configure without the maximum are proven over the scripted codec and
  the host tests only (`decoder::tests`, `codec::tests`).
- A size change while the overlay is open: the emulator run opens the
  overlay after the change; the hold's part is proven by
  `input::tests::a_new_picture_place_keeps_the_hold_and_takes_the_touches_after_it`.
- A real phone's decoders, their adaptive playback and their output
  format's crop, and the arm64 library: step H, on the Galaxy Z Fold 8.
- A size change that the viewer asks for: Route C's request, a later
  slice. Here the sender changed its own size.
- The emulator's times mean nothing (ADR 0063).
