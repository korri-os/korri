# #476 slice 4 — a new picture size in the middle of the stream, on the Android emulator

Run on 2026-11-14 at 21:29 to 21:32 (zao local time) from commit
`78ef93586ed4bf0b74a00adde3739777dca636e3`, with a clean tree:

```sh
nix run .#viewer-android-proof -- --sender root@ibuki --out /tmp/korri-476-s4/proof-8 --port 5596
```

No person took part. The tool built everything on zao. The emulator was
Android 12 (API 31), google_apis, x86_64, headless, 2 cores, `nice` 10, on
zao, with its own adb server and its sound to a WAV file on zao; zao's
load was 4.5 at the start. The sender was stream-cadence's private
Sunshine fixture on ibuki (Intel NUC8i3BEK, `h264_vaapi`), at ibuki's
Tailscale address `100.96.31.114`, port 47493, streaming 1280x720 at 60 fps
with its 1000 Hz tone and its input log. Its Sunshine was
`sunshineFollowingOutput`: nixpkgs' Sunshine 2026.516.143833 with
`nix/stream-cadence/sunshine-follows-output.patch`, which encodes at the
hidden output's size. The sender was ibuki, not aka as in D1 to D4: aka
was above its heat guard's limit (91 °C, with a program named `longshot`
using about five cores) for the hour of these runs, and the guard refuses
to start the fixture there. The fixture
ran beside the product on ibuki and touched no display, no product
Sunshine and no input. The emulator was stopped at the end; no
`qemu-system` process was left, and the fixture's restore check passed.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/9f97g8hf8fjq07qclwp4rm3di4wgfz1i-korri-viewer-android-apk-0.1.0` |
| Refusal probe (unsigned, test only) | `/nix/store/00fgx992pnszmr7i0vkmvw9ldjssngkk-korri-viewer-refusal-probe-apk-0.1.0` |
| Linux viewer, for pairing | `/nix/store/rsim06gambvx8d3jr8nlfqp5ma2gjisa-korri-viewer-0.1.0` |
| Android tests (x86_64) | `/nix/store/4nx035pshyl6qmvshvn0clza3g6175v3-korri-viewer-android-check` |

## What the proof did

D4's run (`../android-d4/`), with one step after the first four
screenshots: the proof asked the sender's agent for `resize` to 720x1280.
The hidden output took the mode 720x1280 at 58.823 Hz, and the frame-ID
source started again at that size, so its IDs started again at 1.
Sunshine's capture started again on the new output size, and its new
encode session began with an IDR frame and an SPS of 720x1280. The input,
the audio server's restart, 15 s of streaming and Back followed at the new
size.

## The size change, step by step

| Time | Where | What |
| --- | --- | --- |
| 04:31:19.828 UTC | `sunshine.log` | `Fixture: the stream follows the output: 1280x720 to 720x1280` |
| 97.511 s | `logcat.txt` | `picture: the stream's size changes from 1280x720 to 720x1280 at frame 606; the decoder follows it (adaptive playback up to 2960x2960)` |
| 97.605 s | `logcat.txt` | `picture: output format … crop: Rect(0, 0, 719, 1279), width: int32(720), height: int32(1280), max-height: int32(2960), max-width: int32(2960) …`, then `picture: decoded size 720x1280` |
| 97.605 s | `logcat.txt` | `picture: video layer 720x1280 placed at (896, 0)-(1504, 1080) in the window 2400x1080`, in the same millisecond on the app's thread |

The emulator's decoder, `c2.goldfish.h264.decoder`, has
`FEATURE_AdaptivePlayback`; the largest square it supports at 60 fps is
2960x2960 (`picture: decode … adaptive 2960x2960`). So it took the new SPS
by itself, and the codec did not restart. 94 ms passed from the SPS to the
first decoded frame of the new size.

## Checks

All 62 passed (`result.json`). The checks of D1 to D4 are the same as in
`../android-d4/`. The new ones:

| Check | Evidence |
| --- | --- |
| The pictures before the change have the stream's shape | `streaming-1.png` to `streaming-4.png`: IDs 5515, 5605, 5710, 5811; the bar's cell is 48.0 pixels wide at y 270, as a 1280x720 picture in (240, 0)-(2160, 1080) puts it (32 × 1.5) |
| The sender's output took the new size | `result.json`, `resize.sender_display`: 720x1280 at 58823 mHz |
| The decoder saw the new size, and gave it | `logcat.txt`, above |
| No decoder failure | no `the decoder failed` line; `picture_losses`: no IDR frame asked for by the decoder, no frame not decoded |
| The layer took the new shape | the logged placement equals the app's rule for 720x1280 in 2400x1080: (896, 0)-(1504, 1080) |
| The bar reads at the new size | `resized-1.png` to `resized-4.png`, a second apart: IDs 95, 183, 275, 367 |
| The screenshots show the new shape | in each, the bar's cell is 15.0 pixels wide at x 927 and y 275: a 720x1280 picture in the new letterbox puts it 15.2 wide at y 270 (18 × 608/720). In the old placement it would be 48 wide |
| The picture moves on after the input, at the new size | `after-input-1.png` to `after-input-3.png`: IDs 1157, 1245, 1334 |
| The input maps into the new picture | `sunshine-input.json`: the mouse's reference is 608x1080, the new picture's size on the screen; 24 touches at the picture's middle (0.5, 0.5) |
| The frame record goes on at the new size | `times.csv`: 2930 rows in index order; 2929 decoded, 2883 handed off. Decoded frame by frame at each frame's own size: 564 handed-off frames at 1280x720 (IDs 5302 to 5905) and then 2319 at 720x1280 (IDs 5 to 2352), each run never going back |
| Sunshine encoded at the new size | `sunshine.log`, above |

The frame record's one repeat: units 584 and 585 both carry ID 5905, the
last picture at 1280x720 (`frame_record.sizes[0].repeated`). Sunshine
encoded it twice as its capture started again; the viewer cannot make a
unit. The earlier run on ibuki (run 4 below) also had one repeat at
1280x720 (742 frames, 741 distinct IDs); it did not record where.

D1 to D4's checks held with the size change: the tone played (48.6 s at
1000 Hz in the emulator's WAV file), the sound went on after Android's
audio server restarted, every key, touch, mouse and pad packet arrived,
the core's input gate closed before the core stopped (139.029 s), and Back
ended the stream with no crash.

## The runs before this one

Not kept. The viewer's code was the same in all of them; `run.py` changed
between them as the table says.

| Run | What happened |
| --- | --- |
| 1 | The sender's agent ended its session at the Android tests: 80 s without a request (the emulator's start and 104 tests), and the agent's limit is 75 s. `run.py` now keeps it alive from a thread (`0c0c1df36`). |
| 2 | zao's load was 18 to 20. The emulator at `nice` 10 lost most packets (`Network dropped …`, `Unrecoverable frame …`), the stream ended after 15 s (`stream terminated: -1`), and an Android sound test played too little. |
| 3 | aka was hot (91.4 °C); the fixture refused to start. |
| 4 | On ibuki: every check passed but `frame_record_ids_move`, which required strictly rising IDs; the sender's repeat above failed it. The check now fails a step back and counts a repeat (`d9b0efcad`). |
| 6 | 34 checks passed up to Back; then `input keyevent KEYCODE_BACK` failed while another tool on zao installed `com.simonwjackson.korri.debug.test` into this emulator through the shared adb server. The emulator now has its own adb server (`78ef93586`). |
| 7 | The emulator process crashed 5 s into the stream, before the size change (crashpad's `read out of range` in `emulator.log`), with zao's 5-minute load at 16 after a peak of 76; the stream had been losing frames since its start. |

## Not proven here

- The restart path on a device. Every decoder here had adaptive playback;
  the restart (stop, configure, start) is proven over the scripted codec
  only (`decoder::tests`).
- A real phone's decoders, their adaptive playback and their output
  format's crop, and the arm64 library: step H, on the Galaxy Z Fold 8.
- A size change that the viewer asks for: Route C's request, a later
  slice. Here the sender changed its own size.
- The emulator's times mean nothing (ADR 0063).
