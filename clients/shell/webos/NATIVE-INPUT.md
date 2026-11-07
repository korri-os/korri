# LG B7 native gamepad input

This change has no TV-side acceptance result. Do not contact the TV overnight.

## Design and grounding

- Linux uses InputPlumber's normalized target, then inputd's
  `CaptureRuntime` (`services/inputd/src/capture.rs`) and
  `PrivateInputChannel` (`producer.rs`). The existing `SeatRequest::Physical*`
  treaty in `contracts/input/src/lib.rs` reaches korrid's private input route,
  the seat receiver, and `host/input_coordination.rs::NativeSources`.
  `NativeSources` publishes baselines and frames to `SurfaceHostInputSource`.
- The reusable in-process producer already exists:
  `services/korrid/src/surface_host_input.rs::{publish_device,publish_frame,publish}`.
  It publishes both the authenticated root native WebSocket and
  `surface/controllers.rs` presence/heard facts. No new wire format is needed.
  The approved composition seam is `EmbeddedPlatform::native_input`: one
  optional callback, called on korrid's runtime, returning a task that startup
  aborts and joins. None retains Android/JNI's empty baseline.
- The TV edge supplies that callback in `bin/webos-korrid.rs`.
  `bin/webos-korrid/evdev.rs` uses the same evdev 0.13.2 as inputd. Inputd's
  capture provider is deliberately not reused: it requires an authenticated
  InputPlumber-normalized Xbox target, D-Bus topology, and an exclusive grab.
  Those are Linux effects, not the TV's raw xpad source. Inputd/core handles
  host shortcuts, not the shell navigation mapper. Importing its mapping would
  add a second normalization stage rather than reuse the navigation mapping.
- The edge opens event nodes read-only. Positive BTN_SOUTH or BTN_TRIGGER
  capabilities use inputd's existing `devices.rs` classification rule. No USB
  ID/name/event-number match, uinput target, exclusive grab, or SDL controller
  subsystem is used. One directory scan per second matches inputd's
  `runtime.rs::RECONCILE_INTERVAL`. Reads wait on Tokio/epoll between samples.
  Native device IDs preserve inputd/physical_sources.rs's uniq-then-phys
  reconnect identity; missing/invalid or duplicate identities fail closed.
  Failed opens retry; diagnostics are suppressed until the node changes or an
  open succeeds. A disconnect drops the fd and removes the native device.
- Keys and absolute-axis metadata/current values form a measured arrival
  baseline. Changed fields commit at SYN_REPORT, coalesced per field. Key
  autorepeat and non-input records do not become presses. SYN_DROPPED immediately retires
  the device and discards the incomplete frame. The next SYN_REPORT installs
  a fresh measured baseline. The existing shell mapper rearms from neutral; a
  recovery baseline does not supply new heard evidence.
- `clients/shell/src/main.rs` starts `NativeInput` without a webOS exclusion.
  `input/native.rs` decodes this existing socket; `input/mod.rs::Router` and
  `input/mapper.rs` own semantic mapping, held-baseline suppression and
  400 ms / 100 ms direction repeat. SDL still supplies remote/window keys.
  `window-sdl/src/lib.rs` initializes VIDEO only, not JOYSTICK or
  GAMECONTROLLER. Thus this change adds no second SDL gamepad delivery path.
  A controller configured externally to emulate keyboard keys is outside this
  guarantee.
- This follows the recorded decision in
  `docs/briefs/2026-10-30-lg-b7-native-input-report.md`, 2026-11-06 update:
  LG SDL joystick init calls `registerScreenSaverRequest(true)` before device
  enumeration. The TV input edge must not enter that power path.

## Linux 4.4 / glibc 2.19 review

The reader uses evdev's `libc::input_event`, not a hand-written record layout.
On ARM EABI the kernel event record is two 32-bit timeval fields plus type,
code and value (16 bytes), checked by a target-only compile-time assertion. The reader uses the established evdev ioctls:
EVIOCGBIT, EVIOCGNAME/PHYS/UNIQ/ID/VERSION/PROP, EVIOCGKEY and EVIOCGABS
(and capability-dependent query ioctls in `RawDevice::from_fd`). EVIOCGPROP
exists since Linux 2.6.38. These predate 4.4. Async setup uses fcntl O_NONBLOCK
and Tokio's existing epoll path. No io_uring, statx, pidfd, event masks,
EVIOCSCLOCKID, revoke, or exclusive grab is added. Baseline timestamps follow
Linux korrid's existing producer (zero; navigation uses the shell's clock).
The final package checker must inspect all archived ELF version requirements;
source review alone does not prove target execution.

## Workstation checks

Focused tests cover sample decoding/coalescing, autorepeat filtering,
SYN_DROPPED recovery, event-node discovery, and embedded input startup through
real authenticated native/model sockets, including heard facts and shutdown.
Existing shell tests cover the actual semantic mapping and repeat.

An opt-in real virtual-pad test uses the shipped evdev sample reader and
measured baseline. The virtual fixture has no physical uniq/phys identity, so
it supplies a fixture native ID directly; production discovery has no such
fallback. The device has a per-process unique name, BTN_SOUTH and two gamepad
stick axes only. RAII destroys it even on assertion failure. It needs write
access to `/dev/uinput` and read access to the event node created by udev:

```sh
CARGO_BUILD_JOBS=2 nix develop .#korrid --command cargo test \
  --manifest-path services/korrid/Cargo.toml --bin webos-korrid \
  virtual_pad_baseline_live_sample_and_disconnect -- --ignored --nocapture
```

Do not run this on the TV. Do not relax host input ACLs just to run it. If access is unavailable, keep
it ignored on that host; do not change groups or udev rules.

## TV verification tomorrow (operator, not automated)

Use the built IPK path and SHA-256 in the handoff. The existing webOS deployment
transport and app-jail entry mechanism remain operator-owned.

1. With the TV on and a person watching, stop `org.korri.shell`. Uninstall only
   `org.korri.shell`, then install the new IPK. Do not touch
   `org.korri.slint-spike`, firmware, vendor SDL, or the persistent allocation
   `/media/developer/org.korri.shell`. Uninstall first: webOS upgrades retain
   removed payload files. Check the package contains no `lib/` directory.
2. Launch `org.korri.shell` normally (not `--fixture`). Confirm Zone renders.
   Inside this app's jail, check the following without printing credentials:

   ```sh
   ps | grep -E 'webos-korrid|korri-shell'
   ls -l /dev/input/event*
   tail -n 60 /tmp/korri-shell-launch.log
   # As the app UID, not the SSH/root user:
   id
   ```

   Expect app UID 6236 with supplementary gid 505 and the xpad node mode 0660,
   root:compositor. The reader discovers the current event number; event2 is
   the previously observed node, not configuration. No SDL joystick-init or
   `registerScreenSaverRequest(1) failed` message should appear.
3. In a root diagnostic shell, use the two PIDs from `ps` to inspect their
   jail fd links (substitute the observed PIDs):

   ```sh
   ls -l /proc/KORRID_PID/fd | grep /dev/input
   ls -l /proc/SHELL_PID/fd | grep /dev/input
   ```

   Expect the gamepad fd only in `webos-korrid`, never `korri-shell`. No fd is
   opened for an evdev keyboard or mouse after classification.
4. Before pressing any pad button: if setup is unfinished, enter Zone's Controllers step with the remote.
   Arrival alone must not prove a pad was heard. Press a physical pad button;
   the pad should become heard and Continue should become available. Do not
   run `setup finish` for this test. If setup was already finished, do not
   erase its marker or change identity merely to force this screen; report
   this acceptance case untested. Workstation socket tests verify the facts.
5. Leave the pad neutral, then press D-pad Right once. Expect one movement.
   Hold it for two seconds: expect the initial move, then repeat after about
   400 ms at about 100 ms intervals. Release: movement must stop immediately.
   Check all D-pad directions, the left stick, A/confirm, B/back, Y/options (BTN_WEST 0x134),
   Start/menu, Select/back, and L1+R1 switching Zone/Pico once per chord.
   The Xbox/Home button does not become a surface system button.
6. Unplug the pad while holding a direction. Expect navigation to stop and
   the korrid fd to disappear. Plug it back in; allow one second for the scan.
   Keep a button/direction held during arrival: it must not activate until
   the pad returns to neutral and is pressed again. Test normal input again.
   Confirm the remote still works and a single pad press does not move twice.
7. Stop/restart the app, then repeat input and fd checks. The pad fd must close
   on stop. Record idle CPU/RSS, reconnect latency, and the launch log. Check
   the existing public `identity status` while stopped if persistence is in
   doubt; never print or copy device keys or RPC capability credentials.

Unverified risks: jail permissions at runtime, actual LG kernel ioctls, ARM
execution, physical-controller behavior, idle CPU/RSS, and the first-run screen
on an already-completed installation. No claim about TV screensaver behavior
is added: this reader neither calls TV power services nor resets idle.
