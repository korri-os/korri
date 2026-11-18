# The overlay with a captured mouse (#434, toward D7′): the play proof on aka

Toggle local cursor's review, P0: Artemis keeps the pointer capture while
its overlay is open, and the overlay then hears only a captured mouse's
moves, which it cannot use. Korri's input lets the capture go when the
overlay opens and takes it again when it closes
(`clients/viewer/src/android/input.rs`, `Input::hold` and `release`). This
run proves it, and reruns the touch pad's step after the review's P1 fix.

Run on 2026-11-18 at 19:58 to 20:03 (aka's clock), on aka as root, from a
copy of this branch with `proof/play.py` as it is committed here, on
`origin/main` `07b0221b7` and nightjar-core. The fixture APK was built on
aka from the same Rust sources:
`/nix/store/4v97ifsqbhbwynkl7jd2m31d7bda635b-korri-android-app-fixture-apk-0.1.0`.

```sh
# in that copy, with HOME, XDG_STATE_HOME and XDG_CACHE_HOME under /var/tmp/d7-controls-run:
nix run .#android-app-play-proof -- --build-store auto \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5674 --adb-port 5174 --out /var/tmp/d7-controls-run/proof \
  --apk /nix/store/4v97ifsqbhbwynkl7jd2m31d7bda635b-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

No person took part. At the start aka read no turns lock, k10temp 59 °C, a
load of 0.39, no emulator and no session or flow of korri-sunshine. The
sender's agent read 80 °C at its own cooldown check, below its 85 °C start
limit. `korri-sunshine-watch.log` read aka every 5 s, 66 times, with none
of korri-sunshine; k10temp reached 96 °C. It counted one emulator process
for the run, and two once, at 20:03:52, its last reading, as the run ended
(inferred: this run's emulator as it stopped; none ran before or after).
The emulator was Android 12 (API 31), google_apis, x86_64, headless, 2
cores; it and its adb server stopped with no process left.

**Result: all 151 checks passed** (`result.json`).

| Part | What happened |
| --- | --- |
| Before | The emulator's PS/2 mouse was captured: `CurrentWindowWithPointerCapture: … com.simonwjackson.korri.app/android.app.NativeActivity` (`pointer-capture-before-overlay.txt`) |
| Guide | The overlay opened, and the viewer let the capture go: `pointer: [ShowIcon, Release] on the stream's view, which had pointer capture: true`; `CurrentWindowWithPointerCapture: None` (`pointer-capture-overlay-open.txt`) |
| The pointer | After four presses of Down, OCR found the row "Crop to fill" at (284, 853). The mouse's own events, sent with `sendevent` as root: one move of (−5000, −5000), which takes the pointer to the top left corner, and 0.8 s later one of (284, 853). Each comes more than 500 ms after the last, so Android's pointer acceleration leaves it as it is (`VelocityControl`'s `STOP_TIME`). SurfaceFlinger's pointer layer (`Sprite`) showed at (270.9, 839.9): 13 pixels up and left of the row's point, the arrow's hot spot (`overlay-mouse-first-pointer.png`) |
| Clicks | A left click there: `korri-app: viewer control set-fill-mode is true`, through korrid. The click moved the overlay's focus to the row and scrolled the list, so the proof looked again: the row at (285, 755), the pointer at (271.9, 741.9) (`overlay-mouse-second-pointer.png`), and a click: `set-fill-mode is false`. The overlay stayed open, and the sender received nothing |
| Back | The overlay closed through korrid's Resume, and the viewer took the capture again: `pointer: [HideIcon, Request] … false`, and the dispatcher gave the app's window the capture (`pointer-capture-overlay-closed.txt`). Five moves of 8 to the right reached the sender as relative moves summing to (40, 0) |
| The touch pad, again | As in `../android-app-touchpad/` (`touchpad.uinput`, `touchpad-input.txt`): relative moves summing to (271, 0); the buttons left down, left up, left down, left up, right down, right up |

The first run of this step, at 19:49 to 19:54 with the same APK, passed
144 of 149 checks. Four failures were the step's own: it clicked the second
time where the row had been before the list scrolled, which was Pan &
zoom, and Pan & zoom closes the overlay. The step now looks for the row
before each click. The fifth was Toggle keyboard's "the shown keyboard is
drawn": SurfaceFlinger still had the keyboard's leash hidden when it was
read, about 2 s after the toggle, although a tap on the keyboard 5 s later
reached the sender as a key (Y). It passed in this run and in the three
runs before; it reads once, and the input method can take longer to draw
(inferred: timing).

Not proven: a real mouse; the overlay with a captured touch pad (the same
hold, host-tested with a mouse); a click on Resume, which OCR does not read while it
has the focus.
