# #391 step E: the Android viewer app with nightjar-core, on the emulator

**Passed: 104 of 104 checks**, with nightjar-core as the network core:
picture, the size change in the middle of the stream, sound, input, the
overlay, the stop, and a second stream in the same process after the stop.

Run on 2026-11-17 at 18:27 to 18:31 UTC, on aka as root, from a copy of
`origin/main` `ee18c0d8a` with this record's proof change (`run.py`'s
second stream and its per-core stop line, commit "the Android proof streams
again after Home"):

```sh
HOME=/var/tmp/step-e-core/android-home XDG_STATE_HOME=/var/tmp/step-e-core/android-home/state \
  nix run .#viewer-android-proof -- --network-core nightjar --build-store auto \
  --sender local --stream-host 100.117.97.45 --port 5634 --out /var/tmp/step-e-core/android-proof-3
```

No person took part. The run started after three checks a minute apart
found aka quiet (1-minute load below 1.5, k10temp Tctl below 78 °C, no Nix
build, no game and no other session's proof), and held
`/var/tmp/korri-turns.lock` on aka while it ran. The emulator was Android
12 (API 31), google_apis, x86_64, headless, with its own adb server; it was
stopped at the end with no process left. The sender was stream-cadence's
private Sunshine fixture on aka (`sunshineFollowingOutput`), 1280x720 at
60 fps with its 1000 Hz tone and its input log; its restore check passed.

| Store path | |
| --- | --- |
| APK with `nightjar` (unsigned) | `/nix/store/6j7plijlqs2xqjyq0js3gn2hq8kax9q8-korri-viewer-android-nightjar-apk-0.1.0` |
| Android tests (x86_64) | `/nix/store/l5x8qy960zf92qlqz9q04n8x2ada26nv-korri-viewer-android-nightjar-check` |
| Linux viewer, for pairing | `/nix/store/09z53gmqbyh3k6rjkx7c8lpgfwdjgzwp-korri-viewer-0.1.0` |

The intent's `args` carried `-networkcore nightjar` (`am-start.txt`);
every stream line in `logcat.txt` is nightjar-core's (`nightjar: at the
stop: …`).

| Part | What happened |
| --- | --- |
| Tests on the emulator | `trust_anchor`, `https` and the session part's lib tests passed (`android-test-*.txt`) |
| Picture | `mediacodec`, `c2.goldfish.h264.decoder`. Running at 49.302 s (logcat's monotonic clock); the bar rose 3281, 3355, 3430, 3506 (`streaming-1.png` to `-4`) |
| Size change | The sender changed to 720x1280 at frame 456; the decoder followed and the bar read in place at the new size (`resized-4.png`) |
| Input | The sender received every key down and up in order, 45 controller packets with A, the full stick and trigger, the hat, an arrival, 2 absolute and 2 relative mouse moves, and 1519 touches. The app's report: `14 keys, 106 controller states, 3 arrivals, 2 mouse, 1900 touches sent; 25 held; 0 unsupported, 0 outside the stream, 0 failed`. The picture moved after the input (1061, 1136, 1210) |
| Sound | The emulator's WAV file holds 72.1 s at 1000 Hz (median RMS 274.8) and 2.0 s of the tests' 440 Hz. The app played 67.9 s (3259920 frames), 15 packets concealed, 0 decode errors; it went on after Android's audio server was killed 30.3 s in and came back |
| Overlay | Guide and Back opened and closed it; the keys, pad and touch sent while it was open never reached the sender (`Z Z` only) |
| Stop | Home: the input's gate closed, the feedback stopped, then `nightjar: at the stop`, then Stopped at 122.480 s and `viewer ended` |
| Second stream | The proof started the app again (`am-start-again.txt`). Android kept the process (pid 4147 before and after), so this stream ran in the process whose first stream had stopped. Running at 140.526 s, the bar rose 4840, 4914, 4989 (`again-1.png` to `-3`), and Home ended it: `nightjar: at the stop`, Stopped at 145.600 s, `viewer ended` (`logcat-again.txt`) |
| Logcat | No fatal signal, Java exception or panic, in either stream |

nightjar-core asked for 7 IDR frames in the first stream and none in the
second (`nightjar: recovery requests sent`). The proof does not count the
C core's requests the same way, so this is no comparison.

The frame record's startup, from Android's START of the activity to the
first frame handed to the display, was 10525.5 ms. The C core's earlier
records on this emulator range from 2223.7 ms (`../android-resize/`) to
8131.4 ms (`../android-d2/`), and this run's three attempts gave 4864.3,
8847.4 and 10525.5 ms. The emulator's startup varies too much for a
comparison; the proof is no measure.

## The two attempts before this one

Same build, 18:07 and 18:17 UTC, under the same gate and lock. Both failed
two checks: `input_gate_closed_before_the_core_stopped` and
`feedback_stopped_after_the_gate_and_before_the_core`. They looked for the
C core's first stop line, `Stopping input stream`, which nightjar-core does
not log. `run.py`'s `stop_order_checks` now takes each core's own line; with
it, attempt 1's logcat passes both, and attempt 1 passed every other check.

Attempt 2 also failed `input_report_has_no_failure`: 24 of its input sends
failed. Attempts 1 and 3 had no failed send. The input report
does not say which sends failed or why, and the proof does not record what
else ran on aka, so the cause is unknown.

The Korri app (`clients/android-app`) cannot choose the core, so no D10
play proof ran with nightjar-core.
