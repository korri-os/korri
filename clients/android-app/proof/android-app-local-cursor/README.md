# Toggle local cursor (#434, toward D7′): the play proof on aka

Run on 2026-11-18 at 17:55 to 18:00 (aka's clock), on aka as root, from a
copy of this branch with `proof/play.py` as it is committed here, on
`origin/main` `e208ebcfc`. The viewer streamed with nightjar-core, the
app's default network core since `1da4206e9`. The fixture APK was built on
aka from the same Rust sources:
`/nix/store/sg8bcx7dp412vj52lniwvrks36w8n0wf-korri-android-app-fixture-apk-0.1.0`.

```sh
# in that copy, with HOME, XDG_STATE_HOME and XDG_CACHE_HOME under /var/tmp/d7-controls-run:
nix run .#android-app-play-proof -- --build-store auto \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5674 --adb-port 5174 --out /var/tmp/d7-controls-run/proof \
  --apk /nix/store/sg8bcx7dp412vj52lniwvrks36w8n0wf-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

No person took part. At the start aka read no turns lock, k10temp 61 °C, a
load of 0.09, no emulator and no session or flow of korri-sunshine. The
sender's agent read 81 °C at its own cooldown check, below its 85 °C start
limit. `korri-sunshine-watch.log` read aka every 5 s, 61 times, with none of
korri-sunshine; k10temp reached 96 °C. It counted one emulator process for
most of the run, and two once, at 18:00:36, two seconds before the run
ended (inferred: this run's emulator as it stopped; none ran before or
after). The emulator was Android 12 (API 31), google_apis, x86_64,
headless, 2 cores; it and its adb server stopped with no process left.

**Result: all 138 checks passed** (`result.json`): every earlier step's
(D10, D11, D5b, #495, #494, Toggle keyboard, Mouse mode, Pan & zoom, the
network core), and these.

| Part | What happened |
| --- | --- |
| The mouse | The emulator's PS/2 mouse, `ImExPS/2 Generic Explorer Mouse` at `/dev/input/event14` (`getevent.txt`): relative X and Y and a left button. With it, korrid's snapshot lists Toggle local cursor, a command, after Mouse mode |
| The stream's start | The viewer hid the pointer and asked for the capture on NativeActivity's view: `pointer: [HideIcon, Request] on the stream's view, which had pointer capture: false` (`logcat-app.txt`). Android's dispatcher gave the app's window the capture: `CurrentWindowWithPointerCapture: 58d3c0b com.simonwjackson.korri.app/android.app.NativeActivity` (`pointer-capture-start.txt`) |
| Captured | The mouse's own events, sent with `sendevent` as root (`mouse.sh`): five moves of 8 to the right, five of 6 down, a left click. The sender received 10 relative mouse moves summing to (40, 30), exactly what was sent, 2 mouse buttons, and no absolute move |
| Shown | korrid's apply: `korri-app: viewer local cursor shows; the mouse points`, then `pointer: [ShowIcon, Release] … which had pointer capture: true`. The dispatcher: `CurrentWindowWithPointerCapture: None` (`pointer-capture-shown.txt`). The same moves and click reached the sender as 15 absolute mouse moves and 2 mouse buttons, with no relative move |
| Hidden again | korrid's apply again: `pointer: [HideIcon, Request] … false`, and the dispatcher gave the app's window the capture again (`pointer-capture-hidden.txt`). The same moves: 10 relative moves summing to (40, 30), 2 buttons, no absolute move. The snapshot still lists the command |
| Home | The stream's end showed the pointer and released the capture: `pointer: [ShowIcon, Release] … false` (the window had already lost the focus, and the capture with it). The stream that korrid started again asked for the capture again |

Not proven: a phone (D12); a mouse or a touchpad that comes or goes while
the stream plays, and a regained focus (host tests only, `capture.rs` and
`input.rs`); a controller with mouse emulation, which offers the control
without a mouse (host tests only); ChromeOS.
