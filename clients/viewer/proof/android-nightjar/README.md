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

## The Android switch's proof under load, 2026-11-18

The proof ran with the default core after the Android switch commit, which
also makes a full input queue answer and log as the C core's ("Input
queue reached maximum size limit"), and with the proof counting those lines
(`input_queue_full_lines`). From that commit's tree on aka, under the lock
but not after the quiet gate: another session's game tests and builds held
aka busy for hours. The run started in a pause below 82 °C (the sender
fixture's heat guard refuses a start at 85 °C), 01:17 to 01:21 UTC; aka's
load and Tctl before and after are in `host-before.txt` and
`host-after.txt` (69 °C at the start, 92 °C at the end). **104 of 104
checks passed.**

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/2i0h3v7h9sr9w9crgphdxh5prc784qnx-korri-viewer-android-apk-0.1.0` |
| Android tests (x86_64) | `/nix/store/dp9nc58133ckmkhisr2n97qd2sdk3hv1-korri-viewer-android-check` |

The input report: `14 keys, 104 controller states, 3 arrivals, 2 mouse, 118
touches sent; 25 held; 0 unsupported, 0 outside the stream, 0 failed`, and
no "Input queue reached maximum size limit" line. This run's swipe sent 118
touches, against 1900 in the run above and 2322 in a run of the first
build of the switch (2026-11-17 20:41, record not kept here), so it did not
test the queue's bound. The emulator decides how many move events one swipe gives:
the C core's runs gave 123 to 1041 for the same steps.

Record: `switch-2026-11-18/run-load1/`: every file the proof wrote except
the APKs, the WAV file, the screenshots, `pair.txt` and the agent's log,
with the full logcat (`logcat-all.txt.xz`), Sunshine's log and its input
packets (`sunshine-input.json.xz`), and the proof's own output
(`proof.log.txt`).

### Can the proof's swipe fill the input queue?

Yes, when the sending thread stalls. At the sender, the swipe's touch moves
came at up to 680 in one 100 ms window with nightjar-core (`../android-nightjar/sunshine-input.json`) and up to 329 with the C core
(`../android-d4b/`). Neither core batches touch: the C core's input thread
batches mouse, pen and motion only (`InputStream.c`), and nightjar-core's
queue batches mouse, pen and text. Both queues hold 150 packets
(`MAX_QUEUED_INPUT_PACKETS`, nightjar-core's `MAX_QUEUED`), about 22 ms of
that swipe at 680 per 100 ms. nightjar-core's window hold
(`INPUT_HOLD_LIMIT`, 75) holds input only while fewer than 75 wait, so it
cannot fill the queue alone; a control thread that does not run for about
20 ms or more can. The C core's input thread sends each touch move at once,
unreliable, and does not wait on ENet's window for it; under the same
stall its queue fills the same way.

Both cores then drop the new packet and keep the queued ones. The C core
logs "Input queue reached maximum size limit" and answers
`LBQ_BOUND_EXCEEDED` (2). The adapter (`src/network_core/nightjar.rs`,
`answer`) now does the same: before, it answered -1 and logged nothing, so
the 24 failed sends of attempt 2 above cannot be told apart from another
failure. The policy is the C core's, through the adapter; no vendored edit.

## Five runs after the quiet gate, and one with `-networkcore c`, 2026-11-18

**Four of the five runs with the default core passed 104 of 104 checks.
Run 1 failed one, `input_report_has_no_failure`: a full input queue
dropped 2 of its 2060 touch sends.** The run with `--network-core c`
passed 104 of 104.

From the tree of the commit "the Android proofs wait for the sender to
cool before they prepare it", on aka. The six runs started after three
checks a minute apart found aka quiet, and held `/var/tmp/korri-turns.lock`
from 07:00 to 07:20 UTC. Each run's load, Tctl and busiest processes before
and after are in its `host-before.txt` and `host-after.txt`. Another
session's proof (`mkw3d`, about 137 % CPU) started at 07:01:56 during run 1
and ran into run 2, without the lock; run 1's swipe was over by then
(07:01:26 to 07:01:27 at the sender), and run 2's start waited 10 s for the
sender to cool.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/2i0h3v7h9sr9w9crgphdxh5prc784qnx-korri-viewer-android-apk-0.1.0` |
| Android tests (x86_64) | `/nix/store/dp9nc58133ckmkhisr2n97qd2sdk3hv1-korri-viewer-android-check` |

| Run | Core | Checks | Touch sends: in the app / at the sender | Failed sends | "Input queue reached maximum size limit" | Tctl before, after |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | nightjar | 103 of 104 | 2060 / 1534 | 2 | 2, at 64.959 s, during the swipe | 57.9, 69.0 °C |
| 2 | nightjar | 104 of 104 | 1115 / 727 | 0 | 0 | 70.5, 63.6 °C |
| 3 | nightjar | 104 of 104 | 1906 / 1497 | 0 | 0 | 63.6, 63.3 °C |
| 4 | nightjar | 104 of 104 | 1247 / 803 | 0 | 0 | 63.3, 64.4 °C |
| 5 | nightjar | 104 of 104 | 2062 / 1545 | 0 | 0 | 64.3, 63.9 °C |
| c | C | 104 of 104 | 1366 / 788 | 0 | 0 | 63.8, 64.5 °C |

The sender receives fewer touch moves than the app sends: Sunshine's log
keeps one line for each input packet it takes, and both cores send moves
unreliable. In each run with nightjar-core both streams logged `nightjar:
at the stop`; the run with `-networkcore c` logged none. Every run's
second stream ran in the same process as its first and its bar rose.

Records: `switch-2026-11-18/run-{1,2,3,4,5,c}/`, every file each run wrote
except the APKs, the WAV file, the screenshots, `pair.txt` and the agent's
log, with the full logcat (`logcat-all.txt.xz`), Sunshine's input packets
(`sunshine-input.json.xz`) and the proof's output (`proof.log.txt`).

**The full queue in run 1.** Both cores drop the new packet when their
150-packet queue is full, and now both log it; the C core's run here sent
fewer touches (1366) and had none. One difference may halve nightjar-core's
room for a burst (inferred, not shown): its control thread leaves input in
the queue while ENet has reliable data waiting to leave
(`vendor/nightjar/protocol/client/src/control/driver.rs`, `transmit`,
`INPUT_HOLD_LIMIT`, 75), and Android delivers a swipe's moves in bursts.
So the queue can hold 74 packets when a burst comes, where the C core's
input thread sends each packet as it comes and holds only for a reliable
packet's send (`InputStream.c`, `ControlStream.c`, at most 10 ms). The C
core holds no unreliable packet, and the touch moves are unreliable in
both. Showing this needs a test that queues a burst while a reliable
packet waits; it would take a vendored edit to change.

### The six runs before, refused

The same six runs from the previous commit, 02:43 to 02:44 UTC, each ended
within 15 s with "The device is hot. k10temp Tctl reads 85.1 to 85.5 °C. A
run does not start at or above 85.0 °C" (`refused-2026-11-18T0243/`). The
quiet gate had let run 1 start at 71.4 °C. The proof's own steps before
the sender's `prepare`, the builds' evaluation and apksigner, lifted aka
past 85 °C within 13 s, and the next runs started at 83.8 to 84.5 °C with
no pause between them. stream-cadence's `view` waits for the sender's
`heat` between runs; the proof did not. It now waits the same way, every
10 s for at most 600 s, and keeps the wait in `result.json`
(`sender_cooldown`: 0.1 s in five runs, 10.1 s in run 2).
