# A captured touch pad (#434, toward D7′): the play proof on aka

Toggle local cursor made the stream's view capture a touch pad as well as a
mouse, as Artemis does. A captured touch pad sends `SOURCE_TOUCHPAD`
events, which the viewer passed to Android unhandled, so the pad stopped
working in the stream. This run proves the port of Artemis's handling of
them (`clients/viewer/src/android/touch.rs`, `Touchpad` and `Mouse`).

Run on 2026-11-18 at 19:06 to 19:11 (aka's clock), on aka as root, from a
copy of this branch with `proof/play.py` as it is committed here, on
nightjar-core. The fixture APK was built on aka from the same Rust sources:
`/nix/store/rnid19dgdawv2fcnklsnif8ygzy4qgz0-korri-android-app-fixture-apk-0.1.0`.

```sh
# in that copy, with HOME, XDG_STATE_HOME and XDG_CACHE_HOME under /var/tmp/d7-controls-run:
nix run .#android-app-play-proof -- --build-store auto \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5674 --adb-port 5174 --out /var/tmp/d7-controls-run/proof \
  --apk /nix/store/rnid19dgdawv2fcnklsnif8ygzy4qgz0-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

No person took part. At the start aka read no turns lock, k10temp 61 °C, a
load of 1.02, no emulator and no session or flow of korri-sunshine; the
first reading of `korri-sunshine-watch.log`, a second later, said 62 °C.
The sender's agent read 80 °C at its own cooldown check, below its 85 °C
start limit. The watch read aka every 5 s, 60 times, with none of
korri-sunshine and one emulator process at most; k10temp reached 96 °C.
The emulator was Android 12 (API 31), google_apis, x86_64, headless, 2
cores; it and its adb server stopped with no process left.

**Result: all 141 checks passed** (`result.json`): the 138 of
`../android-app-local-cursor/`, and these three.

| Part | What happened |
| --- | --- |
| The pad | The emulator has no touch pad. Android's own `uinput` command (`frameworks/base/cmds/uinput`, as root) made one for a few seconds from `touchpad.uinput`: Linux's multi-touch protocol B, a button under the pad (`INPUT_PROP_POINTER`, `INPUT_PROP_BUTTONPAD`), axes 0 to 1000. Android took it for a pointing device (`DeviceType: POINTER`), and while the stream's view had the pointer capture (the PS/2 mouse's, since the stream's start) it was a touch pad in unscaled mode: `Sources: 0x00100008`, `Touch Input Mapper (mode - UNSCALED)` (`touchpad-input.txt`) |
| A move | One finger, 10 steps of 20 units to the right, still for 150 ms before it lifted. The sender received 7 relative mouse moves summing to (271, 0): 20 units are 27 of the host's mouse at Artemis's `TrackpadContext` acceleration (the cube root of 20 / 8), for each of the 10 steps; Android batched some of them. No absolute move |
| A tap | 50 ms down: the left button down, and up 230 ms later |
| The pad's button | One finger on the pad, the button pressed and released: the left button down and up. With two fingers on the pad: the right button down and up (Artemis's "Two finger click"). The fingers' own lifts sent nothing: the second lifted first |

The sender's buttons, in order: left down, left up, left down, left up,
right down, right up.

Not proven: a real touch pad, and one that Android has not captured (the
local cursor shown), which Android makes a mouse; a two-finger scroll and
a tap and drag on the pad (the same `TrackpadContext` as mouse mode 2,
host-tested there).
