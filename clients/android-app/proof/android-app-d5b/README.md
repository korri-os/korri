# Slice D5b — Korri's gameplay overlay over the stream

Run on 2026-11-16 at 02:45 to 02:48 (aka's clock, UTC), on aka as root, from
a copy of `main` at `6f94d9273` with this commit's `proof/play.py` (the run's
file is byte for byte this one) and README text. The fixture APK was built
from `6f94d9273`:
`/nix/store/kd2qr3rk9ayxj3mcw33v625ymkalqxp1-korri-android-app-fixture-apk-0.1.0`.

```sh
# in that copy, with HOME and XDG_STATE_HOME under /var/tmp/android-d5b:
nix run .#android-app-play-proof -- \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5680 --adb-port 5181 --out /var/tmp/android-d5b/proof-5 \
  --apk /nix/store/kd2qr3rk9ayxj3mcw33v625ymkalqxp1-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

No person took part. A poller started the run when aka's k10temp read
62.5 °C, with no emulator, no fixture and no outside client of the product's
korri-sunshine; it read 85.9 °C at the end. It watched korri-sunshine's
ports every 5 s during the run: no outside client connected. An attempt 11
minutes earlier met the sender's heat guard at 85.4 °C and stopped before
the stream. The emulator was Android 12 (API 31), google_apis, x86_64,
headless, 2 cores, console port 5680, its own adb server on 5181, its data
wiped; it and its adb server were stopped at the end, with no process left.
All 63 checks passed (`result.json`), D10's and D11's included; the sender's
restore check passed.

The D5b steps, before any letter key (logcat times, `logcat-app.txt`):

| Step | What happened |
| --- | --- |
| Guide opens it | `input keyevent KEYCODE_BUTTON_MODE` at 02:47:27.311: `overlay: open on Guide; the stream's input is held` |
| The shell takes the layer, korrid hears the leave | 27.393 `the shell took the layer's Surface`; 27.466 `korrid's session.leave for e721a2b2…: ok` |
| The shell draws korrid's overlay | 27.761 `the shell drew it; the layer shows`; the main thread applied the layer's visibility in 0.57 ms. 450 ms from Guide to the overlay on screen |
| Over the moving stream | `overlay-1.png`: Velvet's paused panel, ACCEPTANCE-GAME, the battery (100 %), latency (8 ms), the stream (7.1 Mbit/s) and frames dropped (6) from the viewer's Snapshot, then Resume (focused), Disconnect, Open Korri and End. The panel covers the left of the frame-ID bar, which then does not read; beside the panel the picture changed by a mean 47.7 and 47.8 of 255 between screenshots 1 s apart |
| D11's controls in it | tesseract read Disconnect and Open Korri; after four D-pad downs it read "Stream controls", "Crop to fill" and "Flip A/B and X/Y", both Off (`overlay-controls.png`). No controller that can rumble took part, so Rumble was not offered |
| Crop to fill, chosen in it | A tap on the row (154, 879). korrid's overlay sent the treaty's Apply at 02:47:44.003 (the proof trigger's own applies came later, at 02:48:09, for D11's steps): `viewer:e721a2b2…:1:1:set-fill-mode`, toggle true. 44.193: the video layer `placed at (0, -135)-(2400, 1215) in the window 2400x1080, cropped to fill`; SurfaceFlinger's `korri-viewer-video` went from `crop=[0, 0, 1280, 720]` to `crop=[0, 72, 1280, 648]`, under `korri-viewer-overlay` at z 1 (`surfaceflinger-layers.txt`). The window's right 200 columns, beside the panel, changed by a mean 112.0 (`overlay-fit.png`, `overlay-fill.png`). The overlay stayed open and showed Crop to fill On |
| Chosen again | 02:47:49.110, toggle false: the letterbox `(240, 0)-(2160, 1080)` and `crop=[0, 0, 1280, 720]` again |
| Input held | The sender's Sunshine logged no input packet between the open and the close (`sunshine-input.json`) |
| Back closes it | 53.941 `overlay: layer hidden`; 53.964 `the shell let the layer's Surface go`, then `overlay: closed on korrid resumed the stream after 26.7 s; it took 9 keys and 4 motion events`, then `viewer resume for korrid: e721a2b2…: the stream is in front, its input goes on` |
| The stream goes on | `after-overlay-1.png` to `-3.png`: frame IDs 7011, 7101, 7223; `after-overlay-3.png` is kept. Then D10's keys after the marker (M, K, O, R, I) and its tap reached the sender |

The rest, as in D10 and D11: Velvet listed the holder's game after 61.6 s;
the tap played it in 2.7 s; the picture rose (4906, 5026, 5165, 5301 in
`stream-1.png` to `stream-4.png`); D11's trigger steps passed; Home ended the
stream and the return started it again. The emulator recorded 68.3 s of
sound, 63.2 s of it at 1000 Hz. Logcat holds no fatal signal, panic or ANR.

Two earlier runs with the same APK, not kept, met the proof's own checks,
which this commit corrects in `proof/play.py`; the product's checks passed
in both:

- At 01:00, three checks failed. The frame-ID bar does not read with the
  panel over its left, so the picture's motion is now measured beside the
  panel. tesseract did not read Resume, dark on the focused row, so the
  check reads Disconnect and Open Korri (and the negative of each
  screenshot as well). The stream controls stand below End, out of the
  first screenshot, so the focus first takes the list down to them.
- At 01:46, one check failed: the panel dims the window's left, where the
  fill was measured (a mean 7.8). The right edge, beside the panel, shows it
  (112.2 there in that run).

Not proven here: a real phone and its camera cutout (the overlay covers the
cutout; README, "Not carried"); a controller's Guide, sticks and touchpad
(Guide came from `input`, through the virtual keyboard; the sticks' and the
touches' rules are host-tested in `korri_viewer::overlay`); "Open Korri",
which this app does not carry; and the overlay's failure paths (a refused
leave, a shell that cannot draw, a stop while a resume waits), which are
host-tested in `src/viewer/hosted.rs` and `src/viewer/rules.rs`.
