# #509 items 3 to 5 — the app's screen is the display, goes with korrid's prepare, and follows a new screen

**Passed on the emulator, except the one step that only a phone with two
panels can show.** The app's korrid sent the viewer's screen with its
prepare (item 4). The screen is the display's physical size, also in a
smaller window (item 3). When the app's reading of the display changed in
the middle of the stream, the app reported the new screen at once, the
viewer's shape named it and was not `ready` for 400 ms, and korrid read the
settled screen in the same connection while the picture went on (item 5).
All 55 checks passed (`result.json`): the 35 of #476's resize proof and 20
for #509.

**Not shown: a real fold.** aka's Android 12 emulator cannot make one for
this reading ("The emulator's fold", below). So the new reading came from
the fixture APK's proof trigger (`display WIDTH HEIGHT`, `src/proof.rs`),
in the place of Android's. Whether the display's mode changes when a Galaxy
Z Fold 8 folds is read in AOSP only, and is the owner's test.

**Not run live:** what a holder with display authority does with the
screen. The holder is korrid's federation fixture, which has no compositor
and no configured display: it shapes nothing at prepare and refuses the
viewer's shape reports. So this run does not show that a stream starts in
the phone's shape, nor that it takes a new shape after a fold. Those are
korrid's paths of #476 (`launcher/linux_plugin.rs`, `apply_client_shape`;
`upstreams/live_shape.rs`), which its unit tests cover and this change does
not touch.

**Screenshots kept: one.** `result.json` names 19 screenshots. Only
`other-panel-1.png` is in this folder. The other 18 were not kept:
`stream-first.png`, `stream-1.png` to `stream-4.png`, `portrait-1.png` to
`portrait-4.png`, `landscape-1.png` to `landscape-4.png`,
`other-panel-2.png`, `other-panel-3.png` and `first-panel-1.png` to
`first-panel-3.png`. Their frame IDs and bar positions in `result.json` are
what the run read from them; the record keeps those as the run wrote them.

## The emulator's fold

Two boots of the proof's emulator image with other profiles, on 2026-11-19,
17:49 to 17:51 UTC (`emulator-fold/`: `explore.sh`, each profile's
`config.ini`, the console's answers in `commands.txt`, and `wm size`,
`dumpsys display` and SurfaceFlinger's modes after each step):

| Profile | Step | Logical display (`wm size`) | The default display's mode |
| --- | --- | --- | --- |
| Resizable (`hw.resizable.configs`: phone, foldable, tablet, desktop) | `adb emu resize-display 1`, `2`, `0` | 1080x2400, unchanged | The console answers "KO: Failed to resize display" each time; one mode, 1080x2400 |
| Foldable (a hinge, `hw.displayRegion.0.1` 884x2208 on a 1768x2208 panel) | `adb emu fold` | Override size 884x2208 | One mode, 1768x2208, unchanged |
| Foldable | `adb emu unfold` | 1768x2208 | The same |
| Foldable | `wm size 884x2208` | Override size 884x2208 | The same |

The emulator has one panel with one mode. Its fold forces another logical
size on that panel, as `wm size` does. The viewer's screen is Artemis's
reading, the mode's physical size (`Display.getMode().getPhysicalWidth()`),
which stays. A phone with two panels has two display devices, and Android
gives its default display the one in use (AOSP, `LogicalDisplayMapper`), so
there the mode changes: inferred, not run.

## The run

2026-11-19, 18:26:47 to 18:30:11 UTC, on aka, as root, from a clean copy of
commit `a38e7403e` (`run-on-aka.sh`, `lock.sh`, `host.txt`):

```sh
nix run .#android-app-resize-proof -- --build-store auto \
  --sunshine /nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri \
  --port 5684 --adb-port 5184 --apk APK --fixture FIXTURE --out DIR
```

| Part | Value |
| --- | --- |
| App | The fixture APK of `a38e7403e`, `/nix/store/gciv09qx346n3m2fs5qww8f7jbmj9w1k-korri-android-app-fixture-apk-0.1.0`, with the cargo feature `proof` |
| Holder | korrid's federation fixture of the same commit, `/nix/store/dbxbq5pv6yl8pkvrb2nbddwd76fsh5n3-korrid-federation-fixture-0.0.0`, as uid 59034 with no account |
| Sender | stream-cadence's sender fixture with `sunshine-korri`, the package of #476's record (patches 0020 and 0046; `sender-facts.json`). Output 1280x720 |
| Emulator | Android 12 (API 31), google_apis, x86_64, headless, 2 cores; one panel, mode 1080x2400 |

No person took part. The run waited until `/var/tmp/korri-turns.lock` was
free, k10temp read below 65 °C and no emulator ran, then held the lock (a
noclobber file with its PID, a label and the time, and `flock`) and removed
it. At the start aka read a load of 3.23 and 64 °C; at the end 15.21 and
94 °C. This branch's last build on aka ended at 18:26:13, before the lock.
The emulator and its adb server stopped with no process left (`result.json`,
`emulator_stop`). The exploration above held the same lock from 17:49:04
(64 °C) to 17:51:30. Its count of emulators matched command lines and read 1
after each stop; which process that was is not known. A check at 18:01
found no emulator, QEMU, netsimd, crashpad or adb process. `lock.sh` here
counts by program name, as the proof's run did.

## Results

The times are the emulator's (`logcat-app.txt`).

| Step | Result | Evidence |
| --- | --- | --- |
| The app reads its screen when its activity starts | 2400x1080 | 18:27:33.626 `viewer screen now 2400x1080, not settled`, then `settled`; `dumpsys display`: mode 1080x2400 (`result.json`, `display_mode`) |
| **Item 4: korrid's prepare carries the screen** | **yes** | 18:28:51.327 `korrid: viewer display sent with prepare: 2400x1080`. #476's record of 2026-11-17 has `… sent with prepare: none` at the same step |
| The viewer's Start names the screen, and korrid's snapshot reads it | yes | 18:28:52.214 Start with `display` 2400x1080; the trigger's snapshot: `screen` 2400x1080, `encoded` 1280x720, `ready: true` |
| #476's steps: 720x1280 and back through korrid | as before | 35 checks; the picture's frame IDs rise at each size |
| **Item 5: another reading of the display in the middle of the stream** (`display 1080 2520`) | The app reports 2520x1080 at once, and settled 418 ms later | 18:29:33.128 `viewer screen now 2520x1080, not settled`; 18:29:33.546 `…, settled` |
| The lifecycle's shape follows, and waits for the settle | yes | Two Snapshot events, both in connection `3215:122:44427298` with `encoded` 1280x720: `screen` 2520x1080 `ready: false`, then `ready: true` (`result.json`, `other_panel`) |
| korrid reads the settled screen | yes | The trigger's snapshot through korrid's own adapter: `screen` 2520x1080, `ready: true`, the same connection |
| The stream keeps its size and plays on | yes | `other-panel-1.png`; frame IDs 7160, 7267, 7367 |
| Back to Android's own reading (`display real`) | The same, 437 ms | 18:29:41.384 and .821; the two events; frame IDs 7633, 7739, 7837 |
| **Item 3: the display, not the window.** Home, `wm size 1080x1920`, the app again | The Start of korrid's recovery names 2400x1080 in the window 1920x1080 | 18:29:56.302 Start with `display` 2400x1080; 18:29:56.373 `placed at (0, 0)-(1920, 1080) in the window 1920x1080`; the panel's mode is still 1080x2400 |
| A smaller window is no new screen | yes | No `viewer screen now` line after Home |
| No crash, panic or ANR; the fixture restored aka | yes | `result.json` |

What the fixture cannot show after the settled screen: korrid's live-shape
then offers the holder that screen (`ViewerDisplayEvent::Report`), and the
fixture's holder refuses the report, as in #476's record. No line of either
side names the offer.

## The tests

Written before the code, and run on aka in the builds of `a38e7403e`
(`korri-viewer`, and the app's fixture and product libraries):

| Where | Tests | Result |
| --- | --- | --- |
| `clients/viewer/tests/lifecycle.rs` | A new screen is in the shape at once and `ready` once settled, one Snapshot event for each change, none for the same report or an invalid size; a settled screen does not make a fixed picture ready; a screen that changed before the connection is the first shape's | 23 passed (3 new) |
| `clients/android-app/src/viewer/rules.rs` (`tests/host.rs`) | The screen is read ten times a second; a new screen is reported at once and settled 400 ms later; each new screen starts the settle time again, also one that comes back; an unreadable screen keeps the last one; no report before a first reading | 37 passed (6 new) |

No run of the tests against the old code was made: the calls they test are
new, so that run would only fail to compile.

## On the Fold 8

The lines to read in the app's logcat (`adb logcat -s korri korri-viewer`):

| When | Line | If it is missing |
| --- | --- | --- |
| The app starts | `korri-app: viewer screen now WxH, not settled`, then `settled` | `status: the screen is unreadable: …` says why |
| Play | `korrid: viewer display sent with prepare: WxH` | `none`: the app had read no screen yet |
| A fold or unfold in a stream | `korri-app: viewer screen now WxH, not settled`, about 400 ms later `settled`, then the holder's new display and a new `picture: the picture shown is WxH` | No `viewer screen now` line: the display's mode did not change, and Artemis's reading does not see this fold. `korri-app: viewer stops: the app's window went` at the fold: Android restarted the activity, which a density change does (the manifest does not take `density`) |
