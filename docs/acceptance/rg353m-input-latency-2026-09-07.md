# RG353M D-pad release latency, 2026-09-07

The user confirmed that physical controls work, but reported noticeable latency
on Pico's keyboard page. The investigation reproduced a native routing defect.
The user authorized the fix. A patched native provider is deployed and verified
below. The user then confirmed that physical keyboard navigation feels responsive
and stops promptly on D-pad release. This completes physical acceptance of the
release-delay fix, not the separate gameplay-isolation or press-to-photon checks.

## Result

The installed InputPlumber profile mirrors each D-pad event to the gamepad and
a DBus shortcut notification. InputPlumber 0.75.2 treats every multi-output
profile mapping as a timed chord, including this observation-only duplication.

In upstream `src/input/composite_device/mod.rs::handle_event`, release events
reverse the target list and start with `sleep_time = 80 * events.len()`.
Each output adds another 80 ms. With `[gamepad, dbus]`, DBus releases at 160 ms
and the gamepad releases at 240 ms. The first gamepad press has no such delay.

The portal starts repeating at 400 ms. A short source press held for about
220 ms becomes a roughly 460 ms gamepad press. The keyboard then moves a second
time after the source has already released. Sparse shelf navigation hides the
extra move because selection has already reached the end of its two-item row.

The triggering profile routing first appeared in legacy commit `4445a6b3`,
authored July 21 for SM8550. Commit `d83fefd6`, authored August 7, copied it into
the current shared stack. This session's RG353M normalization activated that
existing routing on this device. The introduction date of upstream chord
scheduling was not bisected.

## Evidence

The final run used 12 presses for each scenario on the unchanged deployed portal.
Measurements use the device's wall clock around `evemu-event` and browser
observation. They include event-writer startup overhead, not network transit.
The browser observer called the real APIs and returned their original results.

| Measurement | Shelf D-pad | Keyboard D-pad | Keyboard face button |
| --- | ---: | ---: | ---: |
| Source press command to browser detection, median | 20.5 ms | 23.5 ms | 22.5 ms |
| Source release command to browser detection, median | 257.5 ms | 264 ms | 21 ms |
| Browser detection to focus-call start, median | 1.4 ms | 5.3 ms | Not applicable |
| Focus calls for 12 source presses | 12 | 24 | 0 |
| Frame interval, median | 16.7 ms | 16.7 ms | 16.7 ms |

The control condition uses native `BTN_NORTH`, browser button 2. It has no
portal action and no duplicate DBus route. Its release lacks the extra delay,
as predicted by the native chord-handling code.

A concurrent root-only `evemu-record` on the normalized Xbox controller showed
D-pad holds of approximately 450–480 ms. Thus the release delay exists before
Chromium. Browser polling, geometric focus selection, and painting cannot cause
that upstream delay. Physical press-to-photon latency was not measured.

The live `ProfilePath` was read through DBus:

`/nix/store/6wlj66mlhck7zfzlq60w2kxx91gf092b-rg353m-inputplumber-resolved/share/inputplumber/profiles/korri-60-xbox_one_gamepad.yaml`

The profile contains the two-target D-pad mappings from
`services/inputd/nix/inputplumber-korri-dbus-shortcuts.yaml`. The pinned upstream
source was inspected directly, including `handle_event`'s release scheduling.

## Fix and verification

The native provider now sends one non-DBus output and its DBus notifications
without entering the timed chord scheduler. Genuine multi-native-output chords
and DBus-only sequences retain the existing scheduler and timing. The portal,
Pico, browser repeat settings, and shortcut profile did not change.

`services/inputd/nix/inputplumber-dbus-observer-timing.patch` contains the change
and six regression tests using real `NativeEvent` values. The package keeps its
upstream source and Cargo hash pins. Its provenance records the applied patch,
and `doCheck = true` requires the tests during builds.

Against the original `events.len() > 1` decision, three new tests failed and
three passed. With the patch, the complete upstream suite passed on x86_64 and
aarch64: 16 library tests, 16 binary tests, and 4 documentation tests. Native
package checks passed on both architectures, including the existing negative
schema-drift cases. Nix and Rust formatting checks passed.

The identical on-device probe produced these results:

| Keyboard measurement | Before | After |
| --- | ---: | ---: |
| D-pad release command to browser detection, median | 264 ms | 20.5 ms |
| D-pad release command to browser detection, maximum | 270 ms | 30 ms |
| Focus moves for 12 short presses | 24 | 12 |
| Face-button release command to browser detection, median | 21 ms | 19 ms |

The normalized controller recorded 12 presses and 12 releases. Its holds were
213.2 to 236.8 ms, without the previous extension. The comparison script asserted
all 12 browser press, focus, and release matches, exactly 12 moves, and a release
median below 60 ms.

A separate live DBus capture verified all 12 shortcut presses and 12 matching
releases, in source order, both before and after the fix. Inputd reported Ready.
InputPlumber, inputd, korrid, kiosk, compositor, and Sunshine remained active.
Temporary CDP was removed and port 9224 was closed after measurement.

The selected immutable bundle is:

`/nix/store/l3z5y3cp2s55la6ykn6lyyixmgq70882-korri-rg353m-release-timing-candidate`

The running executable is:

`/nix/store/wf0jkcz99qgh5f1ckvm6z95z11gppcz4-inputplumber-korri-0.75.2/bin/inputplumber`

Only that executable changed within the bundle. The candidate builder verified
that inputd, the seat receiver, korrid, InputPlumber data, and the selected
shortcut profile still resolve to their previous immutable paths. The OS and
portal selections also remained unchanged. The initial candidate build failed
because copying preserved a read-only output directory mode. It failed before
activation. Making the new output directory writable corrected the build.

Rollback uses the existing bundle selector to restore the preceding native
bundle, `/nix/store/0zgdj7h08aj1mkqc0gmdbinyyw83fpy1-korri-rg353m-input-candidate`.
It does not require an OS switch. Physical press-to-photon latency, real-game
acceptance, and the pre-existing raw joydev access gap remain outside this proof.

Verification requirements from the diagnosis:

- Exercise real native scheduling with one gamepad output plus one DBus output.
  Both transitions must arrive without the artificial 240 ms extension.
- Verify genuine multi-button chords retain their intended timing.
- Repeat the source-to-browser probe. Twelve short keyboard presses must produce
  twelve moves, with no late repeats after release.
- Verify inputd remains Ready, shortcuts still work, and raw-device access stays
  unchanged. Obtain physical acceptance after deployment.

The existing tests validated profile shape and browser gesture semantics. They
did not measure the native provider's timing, so they did not catch this defect.

## Diagnostic artifacts

Local temporary files retain the full evidence and reproducible probe:

- `/tmp/rg353m-latency-probe.py`
- `/tmp/rg353m-latency-observer.js`
- `/tmp/rg353m-latency-keys.sh`
- `/tmp/rg353m-latency-target.sh`
- `/tmp/rg353m-latency-baseline-report.json`
- `/tmp/rg353m-latency-fixed-report.json`
- `/tmp/rg353m-latency-compare.py`
- `/tmp/rg353m-shortcut-probe.py`
- `/tmp/rg353m-shortcut-baseline.log`
- `/tmp/rg353m-shortcut-fixed.log`
- `/tmp/rg353m-release-final-check.log`
- `/tmp/rg353m-latency-keyboard-target.log`
- `/tmp/rg353m-latency-keyboard-face-target.log`
- `/tmp/rg353m-latency-active-profile.yaml`

The temporary loopback CDP override was removed after every run and the normal
kiosk restarted. No raw permissions or OS selections changed. The approved
provider patch was the only runtime change after diagnosis.
The native-event helper's `options` probe was corrected to inject `BTN_WEST`,
which the real browser exposes as button 3. `BTN_NORTH` exposes button 2.
