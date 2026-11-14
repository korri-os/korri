# Slice D2 — the picture, on the Android emulator

Run on 2026-11-14 at 11:56 to 11:59 (zao local time) from commit
`c62e8e796ad674a681fdd1b73fd4f68643f7a87a`, with a clean tree, after the D2
review's fixes:

```sh
nix run .#viewer-android-proof -- --out /tmp/korri-d2/proof-review2
```

No person took part. The tool built everything on zao. The emulator was
Android 12 (API 31), google_apis, x86_64, headless, 2 cores, `nice` 10, on
zao. The sender was stream-cadence's private Sunshine fixture on aka, at
aka's Tailscale address `100.117.97.45`, port 58501, streaming 1280x720 at
60 fps. A game was live on aka (`live_games` in `result.json`); the fixture
runs beside the product and does not touch it. The emulator was stopped at
the end; no `qemu-system` process was left.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/pzkzd27nxqrlhd4cz769q9dl6r0wh5f0-korri-viewer-android-apk-0.1.0` |
| Refusal probe (unsigned, test only) | `/nix/store/3y07vb7nnqdgdcy6h248bywlk9c9i0h7-korri-viewer-refusal-probe-apk-0.1.0` |
| Linux viewer, for pairing | `/nix/store/3b4plhadfp2y12xh0m074li9bhj2818h-korri-viewer-0.1.0` |
| Android tests (x86_64) | `/nix/store/9hm5mcfky2iirgbwmxlsjyf93041rpkf-korri-viewer-android-check` |

## Checks

All 26 passed (`result.json`), and the fixture's restore check passed.

| Check | Evidence |
| --- | --- |
| The APK has no code and both ABIs; no backup; no Recents entry | `apk-contents.txt`, `apk-manifest.txt` (`hasCode` false, `allowBackup` false, `autoRemoveFromRecents` true) |
| The Android tests pass on the emulator | `android-test-https.txt` (4), `android-test-trust_anchor.txt` (1), `android-test-lib.txt` (36: the SPS rewrite, decoder choice and keys, the decoder's loops over a scripted codec, the callback guard, frame record, placement) |
| The picture path | `logcat.txt`: `picture: decode mediacodec decoder c2.goldfish.h264.decoder … try 0 keys low-latency=1,vdec-lowlatency=1,priority=0`; `result.json` `viewer_ran` |
| The video layer, placed with one `setGeometry` | `logcat.txt`: `placed at (240, 0)-(2160, 1080) in the window 2400x1080`; `surfaceflinger-layers.txt`: `korri-viewer-video#0` and its `bbq-wrapper#0` |
| The picture moves | `streaming-1.png` to `streaming-4.png`, a second apart: frame IDs 2950, 3105, 3273, 3424, read with stream-cadence's `bar.py` |
| The startup on the screen | `startup/13.png` (black, 7.4–7.8 s after `am start`), `startup/14.png` (ID 2692, 7.9–9.2 s) |
| The frame record | `times.csv`: Linux's header, 1745 rows in index order, 1745 decoded, 1603 handed off; the recording decoded to 1745 units, and the 1603 handed-off units carry 1603 distinct rising IDs (2690 to 4450) |
| Start, Running, Stop, Stopped, the activity's end, no crash | `logcat.txt`, `resumed-while-streaming.txt`, `resumed-after-back.txt` |
| Sunshine had the session and saw its end | `sunshine.log`: `CLIENT CONNECTED` 18:58:22.461 UTC, `CLIENT DISCONNECTED` 18:58:52.992 UTC |
| An app without DUMP is refused | `logcat-refusal.txt`: `Permission Denial: starting Intent { cmp=com.simonwjackson.korri.viewer/android.app.NativeActivity } from ProcessRecord{… uid=10152} requires android.permission.DUMP` |
| An app that adb granted DUMP may start it | `logcat-refusal.txt`: `korri-refusal: started`, then the viewer's `viewer failed: no intent extra args`: it started and found no launch |

## The startup

stream-cadence's capture (`capture.c`) reads a wlr-screencopy compositor,
and the emulator's screen is not one, so it cannot read this screen. The
proof reads the bar from screenshots with stream-cadence's own decoder.
`screencap` took 0.3 to 1.3 s for each screenshot here, so the screen gives
the startup only to within that.

The frame record gives it on the device's own clock: from the system's
`START` of the activity (logcat, 33.747 s) to the first frame handed to the
display (`times.csv`, 41.878 s), **8131 ms**. In logcat: the activity
existed at 34.565 s and the layer at 34.617 s; Start went at 34.754 s; the
session's HTTPS requests ended at 37.313 s (RTSP port), and the stream ran
at 41.732 s (Running). The earlier D2 runs read 4302 and 7069 ms. These
numbers mean nothing for speed (ADR 0063); they show that the startup can
be read.

## Losses

The decoder asked for no IDR frame and gave up no frame. The core's decode
unit queue overflowed once, at the first frames (42.158 s), and the network
dropped none (`result.json`, `picture_losses`). Of the 1745 frames decoded,
142 were overtaken by a newer frame before the viewer handed them over, as
the lowest-latency pacing wants.

## The run before this one (failed, `failed-run/`)

From commit `9823ece11`, the review's fixes before the last two commits.
The app got its window at 1x1, 31 ms before Android turned it to landscape
(`failed-run/logcat.txt`: `placed at (0, 0)-(1, 1) in the window 1x1`). The
layer stayed in that 1x1 rectangle, so the screen stayed black
(`failed-run/startup-05.png`). The startup screenshots waited 60 s for a
bar, the sender's agent heard nothing for 75 s and ended its session, and
the run ended in a broken pipe (`failed-run/result.json`). The earlier runs
got the landscape window at once; the fault was in the code from the
start. Commit `75fa8b452` makes the layer follow the window's size, and
`c62e8e796` keeps the sender's agent alive and waits at most 45 s for the
bar. In this run the window was 2400x1080 at once, so the run did not
exercise the new path that places the layer again.

## Not proven here

- The arm64 library and a phone's decoder, its composition (HWC) and any
  speed: step H, on the Galaxy Z Fold3. The emulator's decoder is backed by
  the host.
- A decoder error on a device. The host tests prove the back-off, the end
  of the stream, the empty give-back and the callback guard over a
  scripted codec; the emulator's decoder raised no error.
- Placing the layer again after a window resize, Home, and the wait for
  the layer cut short by Destroy: the proof presses Back only.
- Sound and input: slices D3 and D4.
