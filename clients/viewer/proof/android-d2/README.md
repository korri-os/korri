# Slice D2 — the picture, on the Android emulator

Run on 2026-11-14 at 10:56 to 11:00 (zao local time) from commit
`a7f13b5c804fc45074802e9e3dd8782f433d6db6`, with a clean tree:

```sh
nix run .#viewer-android-proof -- --out /tmp/korri-d2/proof-final2
```

No person took part. The tool built everything on zao. The emulator was
Android 12 (API 31), google_apis, x86_64, headless, 2 cores, `nice` 10, on
zao. The sender was stream-cadence's private Sunshine fixture on aka, at
aka's Tailscale address `100.117.97.45`, port 48119, streaming 1280x720 at
60 fps. A game was live on aka (`live_games` in `result.json`); the fixture
runs beside the product and does not touch it. The emulator was stopped at
the end; no `qemu-system` process was left.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/g02xqc2snvnahzffjbz118mfrmjkngxv-korri-viewer-android-apk-0.1.0` |
| Refusal probe (unsigned, test only) | `/nix/store/3y07vb7nnqdgdcy6h248bywlk9c9i0h7-korri-viewer-refusal-probe-apk-0.1.0` |
| Linux viewer, for pairing | `/nix/store/2h63i1fbicmmgbia32svakzw315ipxm8-korri-viewer-0.1.0` |
| Android tests (x86_64) | `/nix/store/5ff61c1gmhz0x35ky4s12h41y0v50s72-korri-viewer-android-check` |

## Checks

All 26 passed (`result.json`), and the fixture's restore check passed.

| Check | Evidence |
| --- | --- |
| The APK has no code and both ABIs; no backup; no Recents entry | `apk-contents.txt`, `apk-manifest.txt` (`hasCode` false, `allowBackup` false, `autoRemoveFromRecents` true) |
| The Android tests pass on the emulator | `android-test-https.txt` (4), `android-test-trust_anchor.txt` (1), `android-test-lib.txt` (25: the SPS rewrite, decoder choice and keys, frame record, placement) |
| The picture path | `logcat.txt`: `picture: decode mediacodec decoder c2.goldfish.h264.decoder … try 0 keys low-latency=1,vdec-lowlatency=1,priority=0`; `result.json` `viewer_ran` |
| The video layer, placed with one `setGeometry` | `logcat.txt`: `placed at (240, 0)-(2160, 1080) in the window 2400x1080`; `surfaceflinger-layers.txt`: `korri-viewer-video#0` and its `bbq-wrapper#0` |
| The picture moves | `streaming-1.png` to `streaming-4.png`, a second apart: frame IDs 2683, 2827, 2947, 3071, read with stream-cadence's `bar.py` |
| The startup on the screen | `startup/08.png` (black, 6.9–7.5 s after `am start`), `startup/09.png` (ID 2451, 7.6–9.0 s) |
| The frame record | `times.csv`: Linux's header, 1648 rows in index order, 1648 decoded, 1449 handed off; the recording decoded to 1648 units, and the 1449 handed-off units carry 1449 distinct rising IDs (2411 to 4074) |
| Start, Running, Stop, Stopped, the activity's end, no crash | `logcat.txt`, `resumed-while-streaming.txt`, `resumed-after-back.txt` |
| Sunshine had the session and saw its end | `sunshine.log`: `CLIENT CONNECTED` 17:59:21.717 UTC, `CLIENT DISCONNECTED` 17:59:50.534 UTC |
| An app without DUMP is refused | `logcat-refusal.txt`: `Permission Denial: starting Intent { cmp=com.simonwjackson.korri.viewer/android.app.NativeActivity } from ProcessRecord{… uid=10150} requires android.permission.DUMP` |
| An app that adb granted DUMP may start it | `logcat-refusal.txt`: `korri-refusal: started`, then the viewer's `viewer failed: no intent extra args`: it started and found no launch |

## The startup

stream-cadence's capture (`capture.c`) reads a wlr-screencopy compositor,
and the emulator's screen is not one, so it cannot read this screen. The
proof reads the bar from screenshots with stream-cadence's own decoder.
`screencap` took 0.4 to 1.4 s for each screenshot here, so the screen gives
the startup only to within that.

The frame record gives it on the device's own clock: from the system's
`START` of the activity (logcat, 30.016 s) to the first frame handed to the
display (`times.csv`, 37.085 s), **7069 ms**. In logcat: the activity
existed at 31.483 s and the layer at 31.556 s; Start went at 31.686 s; the
session's HTTPS requests ended at 33.753 s (RTSP port), the RTSP handshake
and stream start at 36.947 s (Running). The emulator had booted 6 s before;
the run before this one, on a quieter emulator, read 4302 ms. These numbers
mean nothing for speed (ADR 0063); they show that the startup can be read.

## Losses

The decoder asked for no IDR frame and gave up no frame. The core's decode
unit queue overflowed once, at the first frames (37.499 s), and the network
dropped none (`result.json`, `picture_losses`). Of the 1648 frames decoded,
199 were overtaken by a newer frame before the viewer handed them over, as
the lowest-latency pacing wants.

An earlier run of this branch, before the fix (not kept), froze after 7
frames on the screen: the decoder thread held a lock while it waited for an input
buffer, and the output thread needed that lock to release the frames that
would free one. `757edac16` carries the fix. The run before that one, which
gave up a frame after 100 ms instead of waiting, passed with four IDR
requests at the start; the same lock is the likely cause (inferred, not
proven).

## Not proven here

- The arm64 library and a phone's decoder, its composition (HWC) and any
  speed: step H, on the Galaxy Z Fold3. The emulator's decoder is backed by
  the host.
- Home and a window resize (a fold): the proof presses Back only.
- Sound and input: slices D3 and D4.
