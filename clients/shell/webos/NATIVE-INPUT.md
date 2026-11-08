# LG B7 native gamepad input

The owner verified pad navigation on the B7. The latency trace and TCP_NODELAY
change have no TV-side acceptance result. The parent owns TV operations.

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

## Latency investigation (workstation findings; TV measurement pending)

- `services/korrid/src/lib.rs` composes the embedded loopback server with
  `surface_host_input::low_latency_listener`. This now sets TCP_NODELAY on
  each accepted socket, including native input. Axum 0.8.9 (Cargo.lock),
  `src/serve/listener.rs::Listener for TcpListener`, only accepts the socket;
  `src/serve/mod.rs::handle_connection` does not set this option either.
  The regression test checks the default accepted socket has Nagle enabled
  and the configured accepted socket has it disabled. Missing NODELAY is a
  verified code defect for small interactive messages, **not yet a measured
  explanation** of the owner's delay. Inferred mechanism: Nagle can hold the
  final SYN message behind an unacknowledged field message, while the mapper
  must wait for that SYN. Delayed ACK can extend that wait. The change also
  affects this embedded server's HTTP and model sockets; it can increase
  small-packet count.
- `SurfaceHostInputSource::publish_frame` commits changed fields plus SYN_REPORT
  under one mutex. `broadcast` uses immediate `try_send` into each subscriber's
  64-event queue. No timer batches input. The writer drains one message at a
  time, with WebSocket write-buffer size zero and a flushing `send`. Its 250 ms
  write deadline and maximum queue age are failure bounds, not intentional waits.
  A full queue retires the connection instead of accumulating input. The
  evdev reader's one-second scan is discovery only; active reads wait on epoll,
  not on that interval.
- `input/native.rs::serve` blocks on socket reads. The socket's 20 ms read
  timeout is a lifecycle poll bound, **not** a 20 ms input delay: readable data
  wakes the blocked read. No active-path sleep follows each received message.
- Both native input **and SDL host keys** use `EventLoopPost::post` /
  `invoke_from_event_loop`. `Shell::attach` posts SDL's `HostEvent::Key`, so
  the remote is not mapped inline in SDL's callback. The native reader posts
  each field and SYN separately from its worker thread. SDL posts one key event
  on the UI thread. `window-sdl::Proxy` queues closures and wakes SDL;
  `run_jobs` drains a snapshot before render. A sample whose SYN arrives after
  that snapshot can wait through a render. The CPU-heavy animation can delay
  either source; traces must establish the actual queue/render cost.
- `input/mapper.rs::handle` commits both keys and HAT axes at SYN_REPORT.
  HAT values use sign/nonzero, with no 0.6 stick threshold and no time window.
  Stick metadata uses a normalized 0.6 threshold (or 16000 without both axis
  ranges), including flat dead zones. An isolated direction press emits at its
  SYN; its release stops it. Action buttons emit on rising keys, not release.
  Releasing one of several held directions can select the remaining direction.
  L1+R1 switches when both are held, without a chord timer. Baselines must
  rearm from neutral; direction repeat is 400 ms then 100 ms.
  SDL keys use the same repeat schedule, but no SYN barrier.

### Capture protocol

Set **`KORRI_INPUT_TRACE=1` before starting both binaries**. For a manual launch,
inside the app jail as its normal app UID, with the AppManager instance stopped:

```sh
KORRI_INPUT_TRACE=1 ./launch.sh
```

For an AppManager launch, temporarily add `export KORRI_INPUT_TRACE=1` to the
installed launcher before child startup, then remove it after the capture.
No package edit is required for manual environment inheritance. The existing
`exec 2>>"$tmp/korri-shell-launch.log"` precedes both subprocesses; korrid
redirects stdout only. Thus **both** processes already append stderr to
`/var/palm/jail/org.korri.shell/tmp/korri-shell-launch.log` from outside the jail
(`/tmp/korri-shell-launch.log` inside, when TMPDIR is unset).

Each line begins `INPUT_TRACE pid=… mono_us=… stage=…`. The exact value `1`
enables it; it is off by default. There are at most **300 lines per process**
(shared by the shell and its SDL adapter). Restart the app to reset the cap.
No credentials, device names, identity strings, or complete wire messages are
logged. Logging itself adds small bounded I/O overhead.

Press and release one D-pad direction at a time, with a pause, first on the pad
and then on the remote. Use several short taps before any held-repeat test.
Filter with `grep INPUT_TRACE /tmp/korri-shell-launch.log`. Match pad fields by
`type`, `code`, `value` and order; no new wire IDs/timestamps were introduced.

| Stage | Read it as |
| --- | --- |
| `evdev_read` | Kernel `event_realtime_us` versus `read_realtime_us`; their difference is kernel-event-to-reader age. Both are CLOCK_REALTIME, **not** `mono_us`. |
| `publish fields=N` | Validated sample committed, immediately before subscriber queue insertion; follows kernel SYN. |
| `socket_write` | About to send one native field/SYN; `queue_us` is its age in the subscriber queue. |
| `socket_written … ok=true` | Async WebSocket send/flush returned, not a TCP ACK or proof of peer receipt. |
| `native_receive` | Shell worker decoded the field/SYN, before posting to UI. |
| `native_ui` | UI thread is applying that native field/SYN to the mapper. |
| `sdl_key` | Remote key callback: `event_sdl_ms`, `read_sdl_ms`, wrapping `age_ms`, key numbers and press/repeat state. SDL ticks have their own origin. |
| `key_ui` | UI thread applies the posted SDL host key to the mapper. |
| `mapper source=pad/key press=Button(Right)` | Initial semantic output from the relevant mapper path. |
| `ui_apply` / `ui_applied` | Shell starts / finishes synchronous surface update for the semantic button. Repeat ticks also produce these lines. |
| `frame_presented` | First SDL_RenderPresent return after one or more semantic buttons. It is not measured panel scanout; multiple updates can share one frame. |

All `mono_us` values use the same device CLOCK_MONOTONIC origin across processes.
Subtract these values to separate producer, writer, socket, UI queue and surface
update costs. Compare pad `native_receive` → `native_ui` against remote
`sdl_key` → `key_ui`, then both `ui_applied` → `frame_presented` costs. The
kernel/SDL event ages require their respective same-clock fields above, not
subtraction from `mono_us`. This build does not claim a measured TV improvement.

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
