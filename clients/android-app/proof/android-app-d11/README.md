# Slice D11 — the stream's controls and status through korrid

Run on 2026-11-15 at 18:09 to 18:13 (aka local time), on aka as root, from a
copy of this branch as it was on `5f3d2294d` (D8 to D10), at the commit "the
D11 proof reads korrid's own snapshot reads". The fixture APK was built from
the commit before it, which differs only in `proof/play.py`, the README and
a comment in `src/proof.rs`. The branch was then rebased onto `673872282`,
which changes only how `picture.rs` copies a system property (arm64 clippy).

```sh
# in that copy, with HOME and XDG_STATE_HOME under /var/tmp/android-d11:
nix run .#android-app-play-proof -- \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5660 --adb-port 5161 --out /var/tmp/android-d11/proof-2 \
  --apk /nix/store/5aq7a5fc0rmw3cfdvbxkc8mxpik4mwm7-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

No person took part. aka's k10temp read 58 °C at the start and 59 °C after.
The fixture APK (x86_64 only, the library built with the cargo feature
`proof`) was signed with a debug key made in the run's home. The emulator was
Android 12 (API 31), google_apis, x86_64, headless, 2 cores, console port
5660, its own adb server on 5161, its data wiped; it and its adb server were
stopped at the end, with no process left. All 50 checks passed
(`result.json`), D10's included.

D10's steps ran as before (the tap, Running, the frame-ID bar, the overlay
stand-in, keys and a tap at the sender, the tone, Home, the return). The D11
steps came after the input and before Home:

| Step | What happened |
| --- | --- |
| korrid's own reads | korrid's live-shape poll (`upstreams/live_shape.rs`, `synchronize_shape`) read the viewer's snapshot by itself; the adapter logged it at 18:12:00.516 with the two controls, `viewer:28bdd814…:1:1:set-fill-mode` ("Crop to fill") and `…:set-face-button-flip` ("Flip A/B and X/Y"), both toggles at Off, and `batteryPercent` 100. Generation 1, lease 1 |
| The trigger's snapshot | At 18:12:27.343 korrid's adapter answered the proof trigger with the same controls, `batteryPercent` 100.0, and `streamHealth` `{"rttMs":28.0,"deliveredBitrateKbps":6942.7,"framesDropped":7.0}` sampled at 142512 ms of CLOCK_MONOTONIC. No controller that can rumble took part, so Rumble was not offered |
| Apply: Crop to fill on | The treaty's request `{"kind":"Apply","value":{"control":"…:set-fill-mode","value":{"kind":"toggle","value":true}}}` at 18:12:28.922; the loop thread placed the video at `(0, -135)-(2400, 1215) in the window 2400x1080, cropped to fill`, and Android's main thread applied it; the reply `{"kind":"Ok"}` at 18:12:29.038 |
| The effect | SurfaceFlinger's entry for `korri-viewer-video` went from `pos=(240,0)`, `crop=[0, 0, 1280, 720]` to `pos=(0,-135)`, `crop=[0, 72, 1280, 648]`: the stream's middle 576 rows across the whole window (`surfaceflinger-video-layer.txt`). The window's left 200 columns, the shell before, show the picture (mean change 131.2 of 255; `controls-fit.png`, `controls-fill.png`), and the frame-ID bar still reads (7029) |
| korrid sees it | korrid's own read at 18:12:29.150 and the trigger's snapshot at 18:12:31.336 both show Crop to fill On, with fresh health (`rttMs` 13 and 8) |
| A stale control | A control id with another generation (`…:01:1:set-fill-mode`) got `Unknown or stale stream control`; nothing moved |
| Crop to fill off | Apply with Off: the letterbox `(240, 0)-(2160, 1080)` again, crop `[0, 0, 1280, 720]`, and the snapshot shows Off |
| After the return | korrid's recovery started a new runtime; korrid's own read at 18:12:50.499 lists the controls of generation 2 |

The picture rose through the run: 5437, 5559, 5677, 5787 in `stream-1.png` to
`stream-4.png`. The emulator recorded 43.1 s of sound, 39.0 s of it at
1000 Hz. The sender (`sender-facts.json`) was Korri's Sunshine
`2026.914.233613-korri` on port 45153, offering `Korri Stream`; its restore
check passed. Logcat (`logcat-app.txt`) holds no fatal signal, panic or ANR.

Not proven here: the gameplay overlay showing these controls, and an Apply
from korrid's own overlay. In this app korrid's overlay arrives with slice
D5b, so the apply went through the fixture APK's trigger (`src/proof.rs`),
which calls the adapter object that korrid holds. korrid's overlay
projection and its `InvokeGameplayControl` path are covered by korrid's unit
tests (`surface/viewer.rs`, `surface/overlay.rs`). Flip A/B and X/Y and Rumble
are proven by host tests only: the run had no game pad, and the emulator has
no vibrator.

An earlier run with the same APK (`proof-1`, not kept) passed every check but
one: it looked for the adapter's snapshot log after its own trigger, while
the adapter logs only a read whose controls changed, and korrid's live-shape
poll had read them first. The check now looks for korrid's own reads.
