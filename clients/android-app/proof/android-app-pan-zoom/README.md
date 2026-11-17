# Pan & zoom (#434, toward D7′): the play proof on aka

Run on 2026-11-17 at 12:51 to 12:55 (aka's clock), on aka as root, from a
copy of this branch with `proof/play.py` as it is committed here. The
fixture APK was built on aka from the same Rust sources:
`/nix/store/zpi1iz2fmysyq52rx0yrdkln0y3vkhd4-korri-android-app-fixture-apk-0.1.0`.

```sh
# in that copy, with HOME, XDG_STATE_HOME and XDG_CACHE_HOME under /var/tmp/d7-controls-run:
nix run .#android-app-play-proof -- --build-store auto \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5674 --adb-port 5174 --out /var/tmp/d7-controls-run/proof \
  --apk /nix/store/zpi1iz2fmysyq52rx0yrdkln0y3vkhd4-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

No person took part. At the start aka read no turns lock, k10temp 60 °C, a
load of 2.21, no emulator and no session or flow of korri-sunshine;
`korri-sunshine-watch.log` read it every 5 s, 54 times, with none of
korri-sunshine (k10temp 96 °C at its highest). The emulator was Android 12
(API 31), google_apis, x86_64, headless, 2 cores; it and its adb server
stopped with no process left.

**Result: all 125 checks passed** (`result.json`): every earlier step's
(D10, D11, D5b, #495, #494, Toggle keyboard, Mouse mode), and these.

| Part | What happened |
| --- | --- |
| The listing | korrid's snapshot lists Toggle keyboard, Crop to fill, Pan & zoom (a toggle, Off), Mouse mode, Flip A/B and X/Y; the overlay reads Toggle keyboard and Pan & zoom (`overlay-controls.png`) |
| On | korrid's apply of `toggle true`: `korri-app: viewer control set-zoom-mode is true` |
| The gestures' settings | The device's `ViewConfiguration`: touch slop 21, double-tap slop 263, minimum span 446 pixels, long press 400 ms, double tap 300 ms (`logcat-app.txt`) |
| A pinch | Two fingers, 600 then 1000 window pixels apart about the middle, sent with `sendevent` to the first display's touch screen, `virtio_input_multi_touch_1` (`getevent.txt`, `pinch.sh`; as root: the shell may read the input devices, not write them). The app logged the picture zoomed 1.47 times at (-212, -254)-(2611, 1334); in SurfaceFlinger the video layer's crop went from [0, 0, 1280, 720] to [96, 115, 1184, 605] (`surfaceflinger-zoom-*-video.txt`, `zoom-pinched.png`) |
| A drag | `input swipe 1200 540 900 540`: the zoomed picture moved 283 pixels left, to (-495, -254)-(2328, 1334), still 1.47 times (`zoom-panned.png`) |
| The host | The sender's Sunshine received no input packet during the pinch and the drag |
| Off | korrid's apply of `toggle false`, and its snapshot then said Off. The zoom stayed: a tap at the window's middle reached the sender as an absolute mouse move at 0.6004 of the picture's width and 0.5 of its height, the point under the finger in the zoomed place (0.6004, 0.5). Unzoomed it would have been 0.5 |
| Mouse mode | With Pan & zoom on again, korrid's apply of mouse mode "1": its snapshot said Pan & zoom Off and Mouse mode "1" (`Game.applyMouseMode`, line 4178) |

The 1.47: the scale begins once the span has changed by more than twice the
touch slop (at 680 pixels), and each move scales by its span over the last:
1000 / 680.

Not proven: a phone (D12); a double tap and drag (quick scale) on a device
(host tests only, `gestures.rs`); the zoom across a turn or a new stream size
(host tests only, `pan_zoom.rs`).
