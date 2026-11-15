# Slice D4b — rumble, light and battery, on the Android emulator

Run on 2026-11-15 at 14:01 to 14:04 (UTC) from commit
`e3665481e0fba3c2e78a89513d6b2dc369cc25ed`, with a clean copy of the tree
(the tracked files) in `/var/tmp/android-d4b/proof-src` on aka: D4b with
the review's fixes (each controller's latest value, a stop that goes
first, no spin after a panic, battery reports tied to their stream). That
commit was then rebased onto `main` at `05fd8a897`, which brought #476's
slice 4 and #490's decoder recovery; the D4b code did not change in the
rebase, and this proof was not run again after it.

```sh
XDG_STATE_HOME=/var/tmp/android-d4b/state ANDROID_ADB_SERVER_PORT=5141 \
  nix run .#viewer-android-proof -- --sender local --stream-host 100.117.97.45 \
  --out /var/tmp/android-d4b/proof-aka-3
```

No person took part. By the owner's rule of 2026-11-14, nothing was built
and no emulator ran on zao: aka built everything and ran the emulator, and
was also the sender. The emulator was Android 12 (API 31), google_apis,
x86_64, headless, 2 cores, `nice` 10, with its own state directory and adb
server, and its sound to a WAV file. The sender was stream-cadence's
private Sunshine fixture (2026.516.143833), port 37903, at aka's own
Tailscale address, streaming 1280x720 at 60 fps with its 1000 Hz tone and
logging each input packet (`log_input`). Its controllers stay off, so it
sends no rumble and no light. The run started when aka read 60 °C and a
load of 0.9, with no other fixture and no other emulator running. No game was live (`live_games` empty), the fixture's restore
check passed, and no `qemu-system` process was left.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/7rzv7japwq9nbyjn5w7b7igdx91g3vd0-korri-viewer-android-apk-0.1.0` |
| Feedback probe (unsigned, test only) | `/nix/store/pplsli6hp04qzic2gwjpqhayvm8j18hr-korri-viewer-feedback-probe-apk-0.1.0` |
| Refusal probe (unsigned, test only) | `/nix/store/00fgx992pnszmr7i0vkmvw9ldjssngkk-korri-viewer-refusal-probe-apk-0.1.0` |
| Linux viewer, for pairing | `/nix/store/s9rcjb90wkj4g3brxlzllidcx6w5jfg6-korri-viewer-0.1.0` |
| Android tests (x86_64) | `/nix/store/hx5a3r2h57cqlr42sqscx6kidfjblnyy-korri-viewer-android-check` |

## Checks

All 86 passed (`result.json`). The checks of D1 to D5 are those of
`../android-d5/`. D4b's:

| Check | Evidence |
| --- | --- |
| The Android tests pass, the feedback's included | `android-test-lib.txt`: 130 tests, 24 of them `feedback::`, the review's six new ones among them (a slow backend under 2000 rumbles, the last rumble of 0, the stop before waiting work, no spin after a panic, a late battery read after a stop, why a value made no call) |
| The pad is seated with its vibrators and light | `logcat.txt`: `feedback: Sony DS4 (model CUH-ZCT1U)(Bluetooth Test) Motion Sensors (17) is controller 0: rumble Dual(Device), RGB light true`. Android 12 joins the pad, its touchpad and its motion sensors in one input device, here named after the motion sensors |
| Its battery is read and sent | `feedback: battery of … (17): status 3 capacity 0.6 sent to controller 0 as state 2 percentage 60: 0` |
| The sender got the arrival | `sunshine-input.json`: controller arrival, number 0, type 02 (PlayStation), capabilities 00C3 (analog triggers, rumble, battery, RGB light), buttons 0000F7FF |
| The sender got the battery | controller battery: number 00, state 02 (discharging), percentage 3C (60), 1 ms after the arrival |
| The pad left its seat when it went | `input: controller … (17) removed`, then `feedback: … (17) stopped` |
| The feedback stopped between the gate and the core | `input: the core takes no more input` at 116.104, `feedback: ended` at 116.128, `feedback: vibration and lights stopped` at 116.129, the core's `Stopping input stream...` at 116.168, Stopped at 116.310 |
| No feedback call failed | `feedback: ended: 0 rumbles, 0 trigger rumbles, 0 lights, 2 calls to Android; 3 battery reads, 1 battery reports sent, 0 outside the stream; 0 failed` (the two calls: the DualShock 4's cancel when it went, and the phone's at the stop; the three reads: D4's pad, the DualShock 4 and D5's pad) |
| The probe ran every step | `result.json`, `feedback_probe.probe_lines`: seat, rumble low c000 high 4000, light 446688, trigger rumble ffff ffff, rumble off, rumble low 4000 high c000, stop, `stopped true`, `ended` |
| Rumble reached the pad | `feedback-probe-reports.json`, the pad's output reports 0x11: weak motor 64 and strong 192 (high 0x4000 to the weak motor, low 0xc000 to the strong one), then 0 and 0, then 192 and 64, then 0 and 0 at the stop |
| The light reached the pad | red, green, blue 68, 102, 136 (0x44, 0x66, 0x88) |
| Android vibrated the pad and set its light | `dumpsys input` in each step: `Vibrating: true` in both rumble steps, `false` after rumble off and after the stop; the RGB light `0xff446688` from the light step on |
| Trigger rumble made no call | Two vibrators take none, as in Artemis, and the log says so: `trigger rumble controller 0 left ffff right ffff: no call: it has no trigger vibrators (four vibrators rumble the triggers)` |
| The stop stopped the pad | The last reports have both motors at 0; Android turned the light off when the session closed (`0x0`, and 0, 0, 0 in the reports), as it does for Artemis's session |
| The probe read the battery | `battery of … (19): status 3 capacity 0.6 … : -2`: the closed gate refused it, as outside a stream it must |
| No probe call failed | `feedback: ended: 3 rumbles, 1 trigger rumbles, 1 lights, 7 calls to Android; 1 battery reads, 0 battery reports sent, 1 outside the stream; 0 failed` |

The probe's calls came from a thread that is not attached to the VM, as the
core's are: `feedback::rumble`, `rumble_triggers` and `set_controller_led`,
the functions that the stream gives the core.

The other checks held with the feedback in the stream: the tone played
(67.7 s at 1000 Hz), the sound went on after Android's audio server
restarted (67.9 s played, 25.9 s before the restart), the frame record was
written (3996 rows, 3985 handed off, every ID new), D4's input arrived,
D5's overlay held the input, and the network dropped no frame.

The emulator cannot show the review's backlog: its probe sends one value
at a time and Android's calls return at once. The tests show it, over a
scripted Android whose calls take 50 to 300 ms.

## The runs before this one

Fourteen earlier runs are not kept. None failed a check of the app's code.

| Run | Where | What failed | Cause |
| --- | --- | --- | --- |
| aka 1 | aka | Nothing: all 86 passed, from `6d644b135`, before the review | — |
| aka 2 | aka | The fixture refused to start: aka read 87.4 °C, 13 s after the wait saw it below 82 °C | Environment: other work on aka. The wait now wants 75 °C, no other fixture and no other emulator, for three minutes |

On zao, before the owner's rule:

| Run | Where | What failed | Cause |
| --- | --- | --- | --- |
| 1 | zao | The fixture refused to start: aka read 95.5 °C | Environment: aka's heat guard |
| 2 | zao | `nix build` of the feedback probe | Mine: a `git reset` had dropped the new files from the index, so the flake did not see them |
| 3 | zao | 2 of 68: the two seat checks | Mine, in the proof: they matched the pad's name exactly, and Android 12 named the joined device after its touchpad. The app was right |
| 4, 5, 8 | zao | Stopped by me | 4: to run again from a clean tree; 5: my wait script counted its own `pgrep`; 8: another agent's emulator |
| 6 | zao | The sender's agent ended the session: the emulator took 131 s to boot on a loaded zao, longer than the agent's 75 s; D3's AAudio test also fell short of sound | Environment (zao's load), and a weakness of the proof, which sent the agent nothing while the emulator booted (fixed) |
| 7 | zao | 3 of 68: D3's sound after the restart, D2's frame IDs, D4's heartbeat | D4's heartbeat check wanted the first repeat 250 ms after the release, but the heartbeat keeps its own clock (fixed). D3's: mine, in the proof: the DualShock 4's 6 s came before the restart and lengthened the time the check compares with (moved), with heavy sound drops on a loaded zao. D2's: environment: the network dropped 90 frames and the sender encoded pictures twice; D5 has since relaxed that check on `main` |
| 9 | zao | 2 of 68: D3's sound, D2's frame IDs | As in run 7 (before the fixes landed in the run) |
| 10 | zao | The stream ended itself (`stream terminated: -1`) | Environment: zao's load was 43, the network dropped frames from the start |
| 11, 12 | zao | Never started; stopped by me | They waited for a cool aka and a quiet zao; the owner's rule then moved the run to aka |

## Not proven here

- Rumble or a colour that a host sends: the fixture's Sunshine sends none.
  The probe proves the path from the core's callbacks on.
- Trigger rumble on a device: Linux gives no input device four vibrators.
  The tests only.
- A phone's own vibrator for a built-in controller: the emulator has none.
  The tests only.
- A battery that changes during a stream (read every 120 s): the tests
  only.
- Android 14's rule for the light: the emulator runs Android 12.
- A real pad over Bluetooth or USB, its motors and battery, and the arm64
  library: step H.
