# Native controller diagnostic (test only)

This variant observes the real inputd → local korrid WebSocket → NativeAdapter →
DOM focus route. It does not use CDP to deliver input. The old Gamepad/joydev probe
and Mini V2 iteration runner are unchanged. Do not use that runner for this trial.

## Build outputs (off-device, parent-owned)

| Attribute | Contents |
| --- | --- |
| `packages.aarch64-linux.korri-portal-native-input-probe` | Shell observer, normalized-target D-pad writer, parser, and a static portal variant with passive counters. No Chromium build. |
| `packages.x86_64-linux.korri-portal-native-input-probe.grabDeliveryTest` | Separate kernel-delivery test executable for an isolated off-device Linux VM. Not in the device closure. |
| `packages.aarch64-linux.korri-portal-native-input-probe.portal` | Same counter-enabled static assets as the probe's `share/korri-native-input-probe/portal` symlink. |

Build the first attribute on an approved off-device aarch64 builder. Build the
second on the off-device VM's architecture. Nothing here installs a service,
changes signatures, alters a production source file, or activates an image.
The parent owns deployment, temporary component mounts, rollback, and all tests.
Copy complete prebuilt store closures, not lone executables with missing libraries.

The observer binary retains the production name `korri-portal-shell`. Use its
existing private bridge/pipe startup arguments and credential handling. Select
the counter-enabled portal assets at the existing local asset-serving boundary
for the same trial. Do not change the origin, public listener or authentication.
No Chromium rebuild or flags that expose a debugging port are needed.

## Observation and acceptance

The observer emits at most 480 records, at least 250 ms apart, for 120 seconds
from shell startup. The only logged fields are monotonic timestamps, bounded
counts, booleans and the active element's backend node number. No binding reads,
network inspection, WebSocket payload reads, console forwarding, or Gamepad API
calls occur in this observer. CDP only reads page state and one screenshot.

A single JPEG at 15 seconds is stored, without overwriting an existing file, at
`$XDG_RUNTIME_DIR/native-input-diagnostic.jpg.b64`, mode 0600, under 900,000 bytes.
It contains visible user content. Treat it as private trial evidence, not a log
attachment to publish automatically. The image alone cannot prove navigation.

The static portal variant adds saturating counters at these existing sites:

- `native-adapter.ts`: `start`, `onInitialized`, and incoming `input` callbacks.
- `gamepad-adapter.ts`: `start` and its one `navigator.getGamepads()` call.

The existing production selection in `main.tsx:mountPortal` is unchanged. There
is no WebSocket, Gamepad, mapper, or semantic input replacement. No credential
is read by these counters. The instrumentation refuses source drift or another
production Gamepad call site. Counts describe this test-only build, not all
browser internals or an uninstrumented production binary. They add small work
per received native sample. If the counter asset variant is absent, the observer
still records focus but the acceptance parser refuses a native-only pass.

Save raw observer lines without journal prefixes. Copy logs off-device. Run:

```
python3 native-verdict.py observer.log --replay replay.log
```

For actual physical Right presses, record start/end `CLOCK_MONOTONIC` microseconds
on the device through the parent-owned trial and use:

```
python3 native-verdict.py observer.log --physical START_US END_US
```

A window must be at most 15 seconds, bracketed by observations with no gap above
one second. The parser requires a live initialized native adapter, zero browser
adapter starts/reads, native input samples, stable attachment, visible focused
controls and a DOM focus-node change observed **inside** the window. Keep keyboard,
mouse and other controller input idle. Counts plus focus movement establish the
controlled trial, not causal attribution in the presence of unrelated input.
It does not claim every pulse moved focus, measure input latency, or validate
session reservation/lifecycle behavior. A ready PID/socket is never a pass.

## Replay safety and provenance

The parent first discovers the live normalized target using the actual protected
InputPlumber composite/target mapping. Supply all three values, without guessing
an `eventN` by name or order:

```
korri-replay-native-dpad COMPOSITE_OBJECT TARGET_OBJECT /dev/input/eventN
```

The binary checks the unique InputPlumber system-bus owner, composite
`TargetDevices`, target `DeviceType` (`xb360`/`gamepad`) and protected `DevicePaths`.
The target must report exactly the nominated event node. It rechecks these reads
before opening and before each pulse. It does not replace production's physical
source discovery/identity algorithm; the caller must use that discovery result.
Protected metadata permission failure stops the test; do not relax policy.

The opened FD must still match the live node's device/inode, virtual sysfs
location, exact XB360 name, USB 045e:028e version 1, absent phys/uniq, exact 15-key
and eight-axis capabilities, and force feedback capability. These fields come
from `services/inputd/src/devices.rs`, not a new controller schema. Raw devices,
Korri seats, old portal/game targets, symlinks and arbitrary event paths fail.
SourceKind/path/framing/neutral-state self-tests exercise the non-device guards.

All buttons and all eight axes must be exactly neutral before replay and before
each press; there is no deadzone exception and no forced initial reset. Stick
noise can therefore refuse a valid controller. Do not hold or touch controls
during replay. It emits only four `ABS_HAT0X=1` / `ABS_HAT0X=0` pairs, each with
`SYN_REPORT`: 180 ms held, 650 ms released, plus 600 ms initial observation time.
It never grabs, opens joydev, sends confirmation, creates a device, or calls a
browser input method. It does not require inputd to stop capturing.

SIGINT, SIGTERM, SIGHUP and a 15-second alarm retire the loop. A dirty partial
write gets a best-effort neutral report through `atexit`. DBus calls each have a
one-second deadline. A stop can wait for an in-flight call. Disconnect, SIGKILL,
process/kernel failure, or a failed cleanup write cannot guarantee neutralization.
The parent must verify neutral/reconcile or restart the affected approved runtime
before reuse. No tool can promise exit cleanup after SIGKILL. Never compensate by
writing a raw controller or seat. The helper reports **writes complete**, not
kernel delivery or navigation success.

## Why the old joydev proof is insufficient

Capture now owns an exclusive evdev grab in
`services/inputd/src/devices.rs:EvdevProvider::open_capture`. A second observer's
lack of evdev/joydev events is not a failed inputd delivery proof.

Linux's expected path is evdev `write` → `input_inject_event` through the evdev
input handle → evdev's grabbed reader. Input-core handle grabs and evdev client
grabs are distinct. This is a source-based expectation, not a device result.
The separate `korri-native-grab-delivery-test --isolated-vm` creates only its own
uinput fixture, grabs it through a capture FD, writes with the **same encoder**
through a separate FD, checks four Right/release reports at the capture FD, and
checks a third reader receives nothing. It then destroys its fixture. Run this
only in an isolated off-device VM with uinput, never on the handheld or shared
host inputs. Do not infer this test passed merely because compilation succeeded.

Even a passing VM test does not prove the handheld kernel path. Actual native
sample counters and observed focus during the on-device trial remain mandatory.
If replay does not reach capture on that kernel, use actual physical presses and
the same observer. Do not remove the production grab or inject through CDP.
