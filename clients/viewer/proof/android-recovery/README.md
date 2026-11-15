# #490 — the picture goes on after the decoder service dies, on the Android emulator

Run on 2026-11-15 at 11:53 to 11:55 (aka local time), on aka, from the tree
of commit `ba98a189e` (the decoder of `4758d2121`, on #476's size change at
`65d7179a8`), with one line added on aka to `recovery.py` that kept a copy
of the fixture's full Sunshine journal (it holds info lines only):

```sh
nix run .#viewer-android-recovery-proof -- --stream-host 100.117.97.45 --port 5600 --kills 2 --out /var/tmp/fix-490/proof-m2
```

The tool ran from a copy of that tree in a scratch folder on aka, which was
deleted afterwards. No person took part. All 16 checks passed
(`result.json`).

- Emulator: Android 12 (API 31), google_apis, x86_64, headless, 2 cores,
  `nice` 10, with its own adb server (port 15600), on aka. It was stopped at
  the end, and no process of it was left.
- Sender: stream-cadence's private Sunshine fixture (stock Sunshine
  2026.516.143833, `h264_vaapi`) on aka, at aka's Tailscale address, port
  40459, 1280x720 at 60 fps. Its restore check passed. aka's `k10temp`
  read 78.6 °C at the start.
- APK: `/nix/store/z0423f6lp3nma8jxaw2lk3kvk24ywzyg-korri-viewer-android-apk-0.1.0`,
  built on aka. Decoder: `c2.goldfish.h264.decoder` (`decode mediacodec`),
  with adaptive playback, as #476 configures it.

## What the proof did

The app streamed the fixture's frame-ID bar. Three screenshots read rising
IDs. The tool then killed `android.hardware.media.c2@1.0-service-goldfish`
(SIGKILL; the process whose SIGSEGV ended #476's run), waited for init's new
service and for the app's recovery, and took four screenshots a second
apart. It did this twice, then pressed Home.

| Step | Screenshot IDs |
| --- | --- |
| Before (`before-1..3.png`) | 2207, 2296, 2387 |
| After kill 1 (`after-kill-1-1..4.png`) | 2612, 2786, 2939, 3105 |
| After kill 2 (`after-kill-2-1..4.png`) | 3330, 3465, 3595, 3716 |

## Each recovery, from `logcat.txt`

| | Kill 1 | Kill 2 |
| --- | --- | --- |
| `CCodec: Codec2 component "c2.goldfish.h264.decoder" died.` | 37.941 s | 51.252 s |
| `frame N not decoded: cannot queue the frame: AMEDIA_ERROR_UNKNOWN (-10000); recovery from a flush` | frame 425 | frame 1168 |
| `flush failed`, then `restart failed`, both `AMEDIA_ERROR_UNKNOWN (-10000)` | yes | yes |
| `reset done; the core asks for an IDR frame` | 38.225 s | 51.447 s |
| `the decoder returns frames again after a reset` | 38.321 s | 51.578 s |

The flush and the restart fail on a codec whose component has died; the
reset makes a new codec in init's new service. The output thread also saw
the failure (`output failed`); its ask joined the core thread's own
recovery, so each kill took one flush, one restart and one reset. The
decoder returned frames 0.38 s and 0.33 s after the component died.

## The frame record (`times.csv`)

1740 rows in order, 1419 handed to the display, all at 1280x720, with 1419
distinct IDs that never go back (IDs 2057 to 3978; the recording was decoded
on aka and deleted).

| | Last frame shown before | Next frame shown | Gap |
| --- | --- | --- | --- |
| Kill 1 | 37.922 s, frame 424 | 38.325 s, frame 446 | 403 ms |
| Kill 2 | 51.125 s, frame 1167 | 51.580 s, frame 1191 | 455 ms |

Between them, one unit each (425, 1168) reached the decoder and was lost in
the recovery: its row has neither time. The core dropped the others while it
waited for the IDR frame. These are the two largest gaps of the run; the
next are 279 ms and 212 ms. After kill 1, 1006 frames went to the display;
after kill 2, 466.

## The other runs on this tree

Three runs, one after the other, from the same tree and APK:

| Run | Result |
| --- | --- |
| 1 (`run-1-sender-disconnect/`) | 11 of 16 checks. The decoder recovered from kill 1 (reset done at 36.146 s, frames again at 36.312 s, 33 frames shown). Then the fixture's Sunshine logged `CLIENT DISCONNECTED` about 1 s after the kill and sent no more video; the core saw the disconnect at 41.691 s (`Control stream received unexpected disconnect event`, `viewer failed: stream terminated: -1`), before kill 2. Sunshine's log has info lines only and names no cause; the app logged no failure before the disconnect. Whether the viewer had a part in it is not known. |
| 2 (this record) | 16 of 16 |
| 3 | 16 of 16: screenshots 2124, 2213, 2300 before; 2459 to 2737 after kill 1; 2904 to 3173 after kill 2; each kill healed by a reset |

## Not proven

- A real device's decoder and its errors (step H). On the emulator every
  step below a reset fails, so these runs prove the reset and the ladder up
  to it; the flush and the restart that heal, the cap on recoveries, and a
  failed size restart are proven by the scripted-codec tests only
  (`decoder::tests`).
- A codec that the system reclaims (`AMEDIACODEC_ERROR_RECLAIMED`): no
  emulator step makes one. The scripted test proves that it goes straight to
  a reset.
- A size change and a recovery in one run: the fixture's stock Sunshine
  keeps one size here.
- The emulator's numbers mean nothing for speed (ADR 0063).
