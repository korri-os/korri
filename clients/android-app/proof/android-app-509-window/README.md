# #509 — a fold that takes the app's window keeps the stream

**Passed on aka's emulator: 23 of 23 checks of `proof/fold.py`, and the
play proof, 153 of 153.** A new screen reading and then the loss of the
window, as on the Fold 8, kept the stream: its input was held, its layers
went to the next window, korrid read the same connection, and the picture
moved. A window that stayed away stopped the stream 10 s later. Home with
no new screen stopped it at once, as before.

**Not shown: a real fold.** aka's emulator cannot give one: its fold keeps
the panel's mode, and it does not take the window (below). The phone's
order of events is given by the fixture's proof trigger (`display WIDTH
HEIGHT`) and Home. A fold on the Fold 8 is the owner's test.

## The cause, on the Fold 8

The phone's logcat of 2026-11-19, read over adb, nothing changed on the
phone (`phone-logcat-fold.txt`, app pid 14246):

| Time | What |
| --- | --- |
| 16:21:28.447 | `DeviceStateManagerService: Committing state: … name='CLOSED'` |
| 16:21:28.459 to .461 | `Powering off display group due to device_folded`; `Started going to sleep... (why=OFF_BECAUSE_OF_FOLD)` |
| 16:21:28.471 | The launcher opens on the cover screen; the Korri task goes back (`scheduleTopResumedActivityChanged, onTop=false`) |
| 16:21:28.569 | The window's visibility goes (`handleAppVisibility … newVisibility=false`) |
| 16:21:28.594 | `korri-app: viewer screen now 1972x1248, not settled` |
| 16:21:28.621 | `ViewRootImpl >> surfaceDestroyed` |
| 16:21:28.687 | SurfaceFlinger takes the window's root with its 3 children, the viewer's layers, off the screen (`reparent to OffscreenRoot`) |
| 16:21:28.705 | `korri-app: viewer stops: the app's window went`, then Stop |
| 16:21:28.984 | `stopped(true)`: the activity stopped |
| 16:21:33.913 | The owner opens Korri from Recents; the window comes back at .992 (`surfaceCreated`, 1248x1972) |
| 16:21:35.057, 36.645 | `viewer resume for korrid: … failed: the stream is not in front` |

The phone's setting is `fold_lock_behavior_setting=sleep_on_fold_key`
(`settings list system`): a fold puts the phone to sleep on the cover
screen and sends the app back. **The activity was not recreated**: the same
process and `ViewRootImpl` (`VRI[NativeActivity]@a5e2d67`) before and after,
and its configuration changed in place (`scheduleConfigurationChanged …
state=RESUMED`, `sw704dp` and `sw475dp`, 420 dpi on both screens). Only the
window went. The manifest's `configChanges` already had Artemis's
`screenLayout`, `screenSize`, `smallestScreenSize` and `orientation`; the
Artemis Game's `locale` was the one missing, and is there now. Artemis
lists no `density`.

The Artemis app's Game stopped its stream in `onStop` and in
`surfaceDestroyed` (`Game.java` at `artemis-final`), so Artemis had no code
for this to port.

## The run

2026-11-20, 01:27:31 to 01:31:06 UTC, on aka, as root, from a copy of
commit `b8cef1a0b`, an earlier form of this record's commit: its code and
`proof/fold.py` are those of `fd1c5c836` (`run-on-aka.sh`, `lock.sh`,
`host.txt`):

```sh
nix run .#android-app-fold-proof -- --build-store auto \
  --sunshine /nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri \
  --port 5684 --adb-port 5184 --apk APK --fixture FIXTURE --out DIR
```

| Part | Value |
| --- | --- |
| App | The fixture APK of commit `204b1d26e`, `/nix/store/myhzrjmqa9pgicz1wgjbnw5f55fir2lq-korri-android-app-fixture-apk-0.1.0` |
| Holder | korrid's federation fixture, `/nix/store/dbxbq5pv6yl8pkvrb2nbddwd76fsh5n3-korrid-federation-fixture-0.0.0` |
| Sender | stream-cadence's sender fixture with `sunshine-korri`, output 1280x720 |
| Emulator | Android 12 (API 31), google_apis, x86_64, headless, 2 cores; the foldable profile of `proof/android-app-509/emulator-fold/` (a 1768x2208 panel, a hinge, `hw.displayRegion.0.1` 884x2208) |

No person took part. The run waited until `/var/tmp/korri-turns.lock` was
free, k10temp read below 65 °C and no emulator ran, then held the lock (a
noclobber file with its PID, a label and the time, and `flock`) and removed
it. 64 °C at the start, 90 °C at the end. This branch's builds waited for
the lock to go.

## Results

The times are the emulator's (`logcat-app.txt`).

| Step | Result | Evidence |
| --- | --- | --- |
| `adb emu fold` during the stream | The console answers OK; `wm size` gives an override of 884x2208, the mode stays 1768x2208. **The window does not go**: the app gets new sizes in place (884x354, then 2208x884), and the video layer follows | `emulator-fold` in `result.json`; frame IDs 5270, 5360, 5454; same connection `5623:143:874059626` |
| `adb emu unfold` | The same: 2208x1768, then 1768x1416 | frame IDs 5917, 6021, 6129 |
| **A new screen, then the window goes** (`display 1080 2520`, Home as soon as the app logged it, the app again 3 s later) | The app read 2520x1080 at 01:30:30.185, the window went at 31.462 (1277 ms later), the app waited for it, held the input, and the window came back after 3539 ms. The layers were in the new window at 35.904; the input went on. korrid read the same connection, `ready`, screen 2520x1080; the picture moves | `fold` in `result.json`; frame IDs 6744, 6849, 6963; `back-after-the-fold-1.png` |
| The same, and the app stays away (`display real`, Home) | `viewer stops: the app's window did not come back within 10 s` 10.0 s after the loss (45.662 to 55.670); Stopped, and the launch ended | `away` |
| Home with no new screen | `viewer stops: the app's window went` at once, as before #509 | `home` |
| No crash, panic or ANR; the fixture restored aka | yes | `result.json` |

The new screen and the loss of the window were 1277 ms apart here, and
1011 ms in an earlier run (`204b1d26e`'s predecessor, `d170d4642`, with a
1 s limit, which then stopped the stream). On the phone they were 111 ms
apart. So `FOLD_WINDOW` is 2 s. The video's recovery requests in the
session that went through the 10 s wait: 0 IDR.

Two earlier runs on 2026-11-20 are not kept: the first, on `d170d4642`,
failed the step above as said; the second failed when the sender's agent
ended its session, which it does after 75 s without word from the proof.
The proof now keeps the agent alive (`kept_alive`).

## The play proof

`nix run .#android-app-play-proof`, the same day, 00:39:20 to 00:45:41 UTC,
in the same lock, on the fixture APK of `d170d4642`
(`/nix/store/bdcv8kfsmmf560mpkplqsd3p7nn6l1k4-…`): **pass, 153 of 153
checks** (`play/result.json`, `play/host.txt`). `204b1d26e` differs from it
only in `FOLD_WINDOW` (1 s to 2 s) and its test, which the play proof does
not reach: it has no new screen.

## The tests

`clients/android-app/src/viewer/rules.rs`, run in the builds of
`204b1d26e` (`cargo test --test host`, x86_64 and arm64): 45 passed, 7 new:
a screen that replaced another within 2 s is a fold, and the first reading
is none; the window's loss reads the screen at once; a loss without a fold
stops the stream; a fold's loss that comes back keeps it; one that stays
away past 10 s stops it, once; the grace counts from the first loss; Home
after a fold's return stops at once.
