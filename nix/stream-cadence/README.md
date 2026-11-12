# Stream cadence

Measure **distinct decoded source IDs**, not packets, duplicate encoded frames,
server timestamps, or the client's nominal frame rate. The installed Sunshine
plugin stays installed and running. The tool never restarts it or changes its
configuration, scheduling, firewall, output mode, or persistent Sway rules.

```sh
nix run .#stream-cadence -- run --device operator@device:SSH_PORT
nix run .#stream-cadence -- run --device operator@device --ssh-port SSH_PORT \
  --stream-host STREAM_HOST --out ./cadence-results
nix run .#stream-cadence -- compare old/results.json new/results.json
nix run .#stream-cadence -- compare old/results.json new/results.json \
  --max-fps-drop 0.2 --max-capture-misses-increase 1
```

Use a privileged SSH account that can read process environments, change to the
compositor identity, and connect as the certificate-control caller. Configure
and verify its SSH host key first. The SSH port can be a suffix or `--ssh-port`,
not both. An omitted port uses SSH configuration. `--stream-host` defaults to
the SSH host without its account and SSH port. It can select a separate media
link. No device profile or address list exists.

Use repeatable `--ssh-option KEY=VALUE` settings for a custom known-hosts file or
other OpenSSH settings. Each value is passed as one `ssh -o` argument on every
device connection, including the SSH transport used by `nix copy`. For example:

```sh
nix run .#stream-cadence -- run --device operator@device:SSH_PORT \
  --ssh-option UserKnownHostsFile=/path/to/known_hosts \
  --ssh-option GlobalKnownHostsFile=/dev/null
```

## Run options

| Option | Default / meaning |
| --- | --- |
| `--fps FPS` | Display refresh; repeat to test several requests |
| `--resolution WIDTHxHEIGHT` | Active compositor mode |
| `--codec h264\|h265\|av1\|auto` | `h264`; protocol codec, not an encoder assertion |
| `--bitrate KBPS` | 10000, matching the harvested harness; sent explicitly to Moonlight |
| `--seconds SECONDS` | 90 scored seconds; only complete windows count |
| `--warmup SECONDS` | First 10 seconds from the first received frame |
| `--window SECONDS` | 10 |
| `--runs COUNT` | 3 per requested FPS |
| `--out DIRECTORY` | New temporary host directory; never overwrites a run |
| `--keydir DIRECTORY` | `$XDG_STATE_HOME/korri/stream-cadence/keys`, or the standard home state directory |
| `--keep-recording` | Keep the encoded video after a valid run; invalid recordings always remain |
| `--require-vblank-lock` | Stop on fallback/safety guard, or a missing lock marker at first receive |
| `--max-torn-frames N` | No limit; when given, gate each scored window's absolute torn-frame count |
| `--max-unreadable-frames N` | 0; maximum total unreadable frames across all scored windows |

The display refresh is the ceiling. Each window records
`expected_new_frames = min(requested_fps, refresh_hz) * seconds`. A request to
Moonlight is an integer; a fractional refresh is rounded for that request but
retains its measured ceiling. Frame IDs do not wrap during a valid run.

Capture misses count **all presented IDs absent from the received recording**.
Source misses count absent compositor presentation sequences. Thus a deliberate
request below the display refresh can also produce intentional capture misses;
these diagnostic counters are not a claim that every display frame was requested.
The source always runs at the display cadence so the two loss classes remain
observable. New FPS and expected frames use the requested ceiling.

## Discovery and refusal

The controller queries the installed plugin status and manifest to locate its
active Sunshine process. It reads the compositor main PID, adjacent `swaymsg`,
process environment, and native `get_outputs` response. Exactly one active
output is required. Its model, mode, refresh, transform, power state, compositor
identity and runtime directory are retained. Device-tree/DMI, os-release,
current system path, exact plugin package identity, and reported Sunshine
version are retained too. Some plugin status versions do not expose a separate
plugin version: `plugin_version` is then null, not an invented version.

Sunshine's own listening TCP sockets supply candidate ports. The client probes
`serverinfo`, obtains the host UUID and HTTPS port, and requires a reachable
Korri Stream or Desktop application. The fresh session journal supplies the encoder and capture
backend. Their names are recorded, never asserted. Live thread samples include
newly discovered TIDs, names, policies and priorities; no TID crosses sessions.

A game unit, a disabled/inactive plugin, a failed current startup encoder probe,
or an unreachable serverinfo port causes a refusal with a recovery instruction.
A failed probe must be repaired by an operator, not by this test. In particular,
wake the panel and restart the installed Sunshine service manually if its
startup encoder probe failed with the panel off. The tool can wake an otherwise
healthy service from idle-off: it sends compositor seat activity, waits for the
output's power state, and repeats activity every 60 seconds.

Pacing marker strings live only in `discovery.py:MARKERS`. The published plugin
may print none: that is `unmarked`, a valid measurement unless lock is required.
Fallback and safety-guard markers label a run `fallback`.

## Source, scoring and evidence

The motion source is a deliberately harvested `weston-presentation-shm`. It
paints a scaled sync/24-bit ID/CRC-8 bar with a complementary row and logs
SUBMIT, PRESENT (time, sequence), and DISCARD. There is no fixed decoder crop:
the decoder searches sync transitions and validates both rows and the CRC.
An unsupported transform or an unfindable bar invalidates the run and retains
the discovered transform. Each row must independently pass sync and CRC. A pair
with different valid IDs is `TORN`: the result records both IDs and counts the
frame in `torn_frames` for its receive-clock window. Either ID counts as present
in the stream, and each distinct ID contributes to new-frame accounting only
once. This is not a claim that a complete coherent frame arrived. The summary
prints the torn count and fraction. The bar is not shrunk to hide tearing.

A row that fails sync or CRC is still `UNREADABLE`. The default unreadable-frame
limit is zero. `--max-unreadable-frames N` accepts a run only when its total
scored unreadable count is at most N. Any decoder warning or receive/decode count
difference still invalidates the run, regardless of that limit. Presentation
feedback must bracket both boundaries of every complete scored window on the
receive clock. Missing coverage invalidates the run with the window numbers;
a stream repeating its last ID cannot turn a stopped source into a pass.

An unreadable frame contributes **no received IDs**, including an intact row in
that frame. A presented ID that appears only in an unreadable frame stays a
capture miss and never contributes to new-frame counts. Accepting an unreadable
budget does not recover those IDs. FPS is the readable-ID rate, not an estimate
of what the unreadable frames contained. The record and summary include
`unreadable_frames` and `max_unreadable_frames`.

An LD_PRELOAD adapter supplies Moonlight Embedded's real transport with its
fake renderer. It records every decode unit and its `receiveTimeUs`. Moonlight
uses a process-relative MONOTONIC_RAW epoch. The shim bridges each receive time
to host MONOTONIC by preserving the native receive-to-callback delay, then writes
the established `receive_ms` field. It never substitutes callback time for receive
time. FFmpeg performs real decoding after
recording; no GPU/video rendering performance is being tested. Server PTS is
retained as evidence, not used as a clock. Two bounded SSH monotonic bridges
map device presentation times onto the receive clock and report uncertainty.
Per-window latency is presentation-to-receive latency for new readable IDs,
not display scanout latency. Receive timestamps have millisecond quantisation.

Windows start at first receive, include a separately reported raw warm-up, and
then include every complete requested window. Capture attribution excludes all
discarded submissions. A recording tail allows delayed IDs to arrive; an ID is
absent only if it is absent from the whole recording.

Each host run directory contains `result.json`, receive CSV, source/journal/
client logs, decode diagnostics and live thread samples. `results.json` is the
list of completed records. The established field names are harvested from
`content-memory-phase-accept-usb1-result.json` and its source result in the
read-only scratch harness. `source_counter.windows` keeps `new_frames`,
`capture_misses`, `source_misses`, `undecodable_frames`,
`source_discarded_submissions`, and `captured_unpresented_ids`. It adds
`torn_frames` and `received_frames` per window; `torn_events` records both IDs
and receive time for every torn frame.
The single added `discovered` block holds runtime facts, validity, clock bridge,
run sizes and restoration evidence. Failed runs retain their own `result.json`.
A run above its unreadable limit also retains all per-window class counts,
`torn_events` and `unreadable_events`. Those counts are diagnostic when the run
is invalid. Counts within the limit still exclude all unreadable IDs.
`results.json` includes failed records;
comparison with an explicit limit cannot pass an empty or invalid measurement.

`compare` accepts one result record or a results list. It prints each paired
run/window's new-FPS, capture/source miss, torn-frame and median latency deltas. It does not
choose a product regression threshold. Optional limits are `--max-fps-drop`,
`--max-capture-misses-increase`, `--max-source-misses-increase`, and
`--max-latency-increase-ms`. `--max-torn-frames N` gates the new result's absolute
per-window torn count, not just its change. It also works on `run`. A valid
measurement can fail that explicit gate; no torn limit applies by default.
Without limits, numerical regressions exit zero.
Run/window pairing is positional; compare the same requests in the same order.

## Viewer measure

`run` measures the sender and the network: its client has no renderer. `view`
measures a viewer from outside, with its real renderer, and no person looks at
a screen.

```sh
nix run .#stream-cadence -- view --sender operator@sender --viewer operator@viewer
nix run .#stream-cadence -- verdict baseline/results.json candidate/results.json
```

The sender runs Sunshine and the frame-ID source, as in `run`. On the viewer
device, `capture.c` reads the ID bar from the compositor through
wlr-screencopy. It samples the 36 cells of both bar rows on every output frame
and writes one `display.csv` record (`index,display_ns,copy_ns`) for each
frame. `copy_ns` is the time at which the capture asked for that frame.
`bar.py` decodes the samples, so the bar has one decoder.

Each run starts the viewer twice. The first start only finds the bar on the
viewer's output. The second start is the measure: the capture already runs, so
the time to the first frame is visible. Windows start at the first frame that
belongs to the second start: the first frame with an ID that the sender
presented at or after that start. A picture that the first start left on a
real panel has an older ID and does not count.

`display_ns` is the compositor's commit time for the output frame. It is not
panel scanout, and it tells nothing about panel response or how a stream
feels. While the capture runs, the compositor composes every frame, so direct
scanout is off. A baseline and its candidate are measured the same way, so
they stay comparable.

### The fixture

The viewer and the capture client run inside a fixture: a second, headless
sway process started from the device's installed sway binary, with the pixman
renderer, inside the run directory. The product compositor is read and never
changed, and the test viewer never appears on the display that Sunshine
captures. The fixture viewer is nixpkgs `moonlight-embedded` with its `sdl`
system, copied with `nix copy`. A server-only device has no viewer plugin.

The fixture proves a viewer's session part and network library. It does not
prove the picture path of a handheld or a phone.

The viewer's agent reads the product output only for the installed viewer.
The fixture has its own output.

### The installed viewer on a handheld

On a device with the enabled `@korri:moonlight` plugin, `view` runs the
plugin's own `moonlight` on the device's own compositor and its real panel.
A plugin that is installed but switched off counts as absent: the device then
gets the copied viewer and the fixture.

The agent runs as root, changes to the compositor's user with that user's
groups, and starts the viewer with the arguments of korrid's `linux_viewer`
(`stream_args`). The `-platform` value comes from a copy of korrid's chip
table in `viewer.py`. A test reads korrid's file and fails when the two
differ.

This is not a korrid launch. The differences:

- No `korri-game-*` systemd unit runs: no unit sandbox, no cgroup, no unit
  credentials. The viewer is a child of the agent.
- The arguments carry the tool's request (`-width`, `-height`, `-fps`,
  `-codec`, `-bitrate`), not the settings that the plugin renders for a
  launch.
- `-viewonly` is set and no `-input` seat is given.
- Sound goes to SDL's dummy driver. The product's audio server exists only
  inside a korrid launch.
- HOME and the key directory are the run directory.
- The agent, not korrid, asks sway for fullscreen and focus. It tries every
  0.1 s until sway knows the window. `startup_ms` contains that step.
- korrid's own launch path and its lifecycle reports are not used.

With the sender fixture, the device pairs by PIN (`moonlight pair` with the
plugin's binary), and `KORRI_MOONLIGHT_SERVER_CERTIFICATE` pins the fixture
Sunshine's certificate. With the sender on the host that runs the tool, the
viewer's agent reports the far end of its own SSH connection, and the stream
goes to that address: it takes the link that the SSH target names.

**State on 2026-11-11: works once on each bench device, not proven.** One
short run (10 scored seconds, 640x480) on each handheld:

| | RG353M (RK3566) | RG35XX Pro (H700) |
| --- | --- | --- |
| Result | invalid: 2 capture gaps | valid |
| Output-frame records | 1932 in 32.5 s, 59.47 each second | 1955 in 32.6 s, 60.00 each second |
| Refreshes with no record | about 15 | 0 |
| IDs lost at a gap in the record | 11 | 0 |
| Shortest interval between two records | 10.2 ms (0.61 period) | 10.4 ms (0.62 period) |
| Intervals from 0.50 to 0.75 period | 16 of 1931 | 5 of 1954 |
| `startup_ms` | 1990.5 | 1569.1 |
| Viewer CPU, CPU seconds each second | 0.147 | 0.135 |

The next run on the RG353M ended 5 s into the measured stream: the device
left the USB link and did not answer again. The cause is not known. No
handheld baseline exists, and device runs stopped there.

The gap rule below does not hold on these panels; read "Capture gaps".
The private half of the test key pair crosses SSH into the viewer's run
directory, because the viewer runs there. Cleanup removes it. The sender
still revokes the certificate on exit.

### The sender fixture

```sh
nix run .#stream-cadence -- view --sender local --viewer local --sender-fixture
nix run .#stream-cadence -- view --sender local --viewer operator@viewer \
  --sender-fixture --viewer-stream-host ADDRESS_OF_THIS_HOST
```

`--sender-fixture` gives the measure a private sender. The sender target
then needs no product Sunshine, no Korri compositor, no certificate socket
and no root. A game on a device does not stop the measure, because the
fixture does not use the device's display.

The agent `sender.py` owns one run directory under `/tmp`. In it run:

- a hidden compositor: a headless sway from nixpkgs with the `gles2` renderer
  on a render node. With a software renderer Sunshine captures one frame only;
- the frame-ID source, shown on that compositor alone;
- a stock Sunshine from nixpkgs with its own ports, state, key and log. It
  captures the hidden compositor through the Wayland protocol.

The fixture discovers what it uses:

| Value | How |
| --- | --- |
| Ports | A port that the kernel reports free, with Sunshine's whole span free around it. The fixture never takes the span of Sunshine's default port. That keeps it away from a Sunshine that uses the default; it knows nothing of a Sunshine on other ports. |
| Render node | Each node in `/dev/dri` that the user can open, in order. The first node on which Sunshine finds a hardware encoder wins. With none, the first node that encodes at all. |
| Encoder | Sunshine's configuration names none. Sunshine tries hardware encoders first. The result records `encoder`, `working_encoders` and `sender_fixture.hardware_encoder`. |
| Display mode | The requested stream size, at the steady refresh below. |

Pairing replaces the certificate grant. The viewer side runs `moonlight pair`
with the viewer binary of that run, and makes its own key pair in its run
directory. Moonlight prints a PIN. The sender's agent posts the PIN to its
Sunshine on loopback, with a web UI login that it made for the run. The host
key directory is not used.

Sunshine announces each host through Avahi and has no setting against it.
The fixture gives its Sunshine a system bus address that does not exist, so
the private sender does not appear in the host lists of the network.

**The steady refresh.** A headless output has no clock. After each frame it
waits a whole number of milliseconds: an output that says 60 Hz waits 16 ms
and shows about 62 frames each second. Sunshine takes one frame for each
period of the stream's frame rate, so it would skip about two source frames
each second. The fixture sets the fastest mode whose whole-millisecond period
is not shorter than the stream's period: 58.823 Hz (17 ms) for a request of
60. The output is then the slower side, Sunshine takes every frame, and the
source shows about 57.8 frames each second. `requested_fps` stays the
request, and `display.refresh_hz` is the mode.

Two results follow from that rate.

- **A floor on a 60 Hz panel.** The panel refreshes about 2.3 times each
  second more often than the fixture sends a frame. Each of those refreshes
  repeats a picture, so a viewer on a 60 Hz panel shows about 2.3
  `repeated_frames` and `late_frames` each second that come from the fixture,
  not from the viewer. Compare two viewers against the same fixture, and
  subtract nothing: the floor is the same for both.
- **`expected_new_frames` is a ceiling, not the real rate.** It is the mode's
  refresh times the window: 588.23 for 10 s. The output starts its timer again
  after each frame, so a clean window has 576 to 582 new frames. No gate reads
  the field.

A headless output also counts no vblanks: every presentation has sequence 0.
For such a sender `source_misses` is derived, not counted. A gap above 1.5
refresh periods between two presentations gives `round(gap / period) - 1`
misses, and `source_events` has no `missing_vblank_sequence` for them. A
sender with a vblank counter is scored by sequence, as before.

**The host that runs the tool.** The target `local` names it. It uses no SSH
and no root. As a sender, `local` can host the fixture only. As a viewer,
`local` runs the viewer fixture as the calling user with a sway from nixpkgs,
and reads no Korri unit, even when Korri runs on that host. A viewer on
another device reaches the sender through `--viewer-stream-host`. The tool
does not change a firewall.

**Cleanup.** Each child of the agent ends when the agent dies, however it
dies. `close` stops the source, Sunshine and the compositor, then looks for
any process that still carries the run directory in its arguments,
environment or working directory. It ends each one and lists it in
`restore.stray_processes_killed`. The restore check fails the run when a
process or the directory is left.

**What the fixture does not prove.** It is not the product's sender. It does
not use the product's KMS capture, its encoder settings, its pacing or its
display. Sound is off. With both targets on one host the stream uses
loopback, so the network is not measured, and the sender and the viewer
share one machine's CPU. The numbers compare an old and a new viewer against
the same test sender. `run` still measures the product sender. `verdict`
refuses to compare a fixture result with a product-sender result: the
capture backend and the display mode differ. It also refuses two fixture
results from different sender hosts, render nodes or viewer devices.

Not tested yet: the fixture on an SSH target, and on aarch64.

**First proof, 2026-11-11.** Both fixtures ran on the build host (an XPS 17
9710) at 1280x720 with a request of 60 and 20 scored seconds. Sunshine used
the Intel VAAPI H.264 hardware encoder. Two sets of five valid runs gave
`equal` for each measured number. This is not a baseline, for two reasons.

- The host was not quiet. Other work held its CPU package at 100 °C, and the
  host then stops all CPUs together for tens of milliseconds at a time. Each set
  needed four attempts, because a run with a capture gap is invalid and ends
  its set.
- The spread is wide. Eight runs have at most 1 source miss and at most 3
  dropped frames in 1156. Two runs have 5 and 6 source misses and 64 and 15
  dropped frames. In each run the sender's median presentation interval is
  17.3 to 17.4 ms. Of about 21 800 intervals, 29 are above 25.5 ms.

In a quieter moment on the same host, a 20 second stream to the `run`
client had no presentation interval above 18 ms. Record a baseline only on a
host that is quiet for the whole set.

`tests/intervals.py RESULTS.json` prints, for each run, the interval
distribution of the sender's presentations and of the new IDs on the viewer's
display. A long tail in the first is the sender's own.

### The six numbers of ADR 0062

| Number | Fields | Method |
| --- | --- | --- |
| 1 Packet arrival to display | `viewer.packet_to_display_ms` | Not measured: today's viewer reports no arrival time. The gate is `latency_ms`, sender presentation to viewer display, on the same sender and link. |
| 2 Late, repeated, dropped | `capture_misses`, `repeated_frames`, `late_frames`, `torn_frames`, `undecodable_frames` in each window; `viewer.output_repeats` | A dropped ID was presented and never displayed. A repeated frame shows no new ID. A late frame is a new ID more than 1.5 refresh periods after the previous new ID. An output repeat is a refresh for which the compositor committed no frame. |
| 3 CPU and battery | `viewer.cpu_seconds_per_second`, `viewer.battery_mw` | Viewer process CPU time from procfs over the scored time. Battery power only while a battery discharges in every sample; else not measured. |
| 4 Startup | `viewer.startup_ms` | Viewer process start to the first displayed frame of that start: the first ID that the sender presented at or after it. korrid's launch path is not included. On a handheld the time contains the agent's fullscreen and focus step. |
| 5 Input | `viewer.input_ms` | Not measured: the press-to-packet measure is not built. |
| 6 Stability | `viewer.stream_stalls`, `viewer.viewer_exits` | A stall is one second without a new ID in the scored time. A viewer exit ends the run as invalid. |

`viewer.not_measured` gives the reason for each number that has no value. No
number is estimated. An unreadable output frame is a count, not an invalid
recording: a viewer can show a frame without a whole bar.

**Capture gaps.** A gap is an interval above 1.5 refresh periods between two
records in the scored time. `score.py:capture_gaps` splits the gaps by
`copy_ns`:

- The capture asked more than 0.75 period after the previous frame. The
  request can arrive after the next commit, so the gap counts as a missed
  frame in `viewer.capture_gaps`. The default limit is zero, and a run above
  `--max-capture-gaps` is invalid.
- The capture asked in time. The compositor committed no frame, so the gap
  counts in `viewer.output_repeats`, which `verdict` judges.

The split assumes that two commits are at least one period apart. That holds
on the headless fixture. **It does not hold on a real panel.** After an idle
output, sway commits at once, and its next commit can follow at the next
vblank. Both bench handhelds show intervals down to 0.61 period. A frame that
the capture missed can then count as an output repeat, or leave no gap at
all, and its ID becomes a dropped frame of the viewer. On the RG353M, 11 of
15 gaps lost an ID, and 4 of those 11 count as output repeats. Until a rule
holds there, `capture_gaps`, `output_repeats` and `capture_misses` of a
handheld run are not proven, and a valid handheld run does not prove a
complete display record.

### Verdict

`verdict` compares two `view` results, five valid runs each by default. Every
metric is lower-is-better. A metric is **worse** when every candidate run is
worse than every baseline run, and **better** when every candidate run is
better. Otherwise the candidate is inside the run-to-run spread, and the
metric is **equal**. A metric with no value in a run is **not measured**. The
command exits non-zero when a metric is worse. With five runs each, two
results from the same viewer separate by chance in about 0.8% of comparisons
for each metric.

`verdict` refuses two results that were not measured under the same
conditions: the sender's display mode, Sunshine version, encoder and capture
backend, the sender's model and render node, the viewer device's model, and
the requested resolution, frame rate, codec, bitrate and viewer refresh. A difference there would judge the sender, not the viewer. An unwatched
display keeps the mode of the last viewer that shaped it, so check the mode
before a candidate run. `--allow-different-conditions` overrides the refusal.

Baselines live in `baselines/<device>/results.json`. They are measurement
data and are not part of the tool's Nix source.

| Baseline | Recorded | Viewer | What it proves |
| --- | --- | --- | --- |
| `baselines/aka` | 2026-11-11, five runs | nixpkgs `moonlight-embedded` 2.7.1, `sdl` system, in the fixture | The session part and network library on aka. Record it again with the published `@korri:moonlight` x86_64 plugin before stage 1 of #391 compares against it. |

In that baseline the viewer's delay grows in steps of one refresh period
during the first 20 seconds and then stays near 51.5 ms, about three periods
above the first frames. The viewer queues frames and never drops one to catch
up. This is the fixture viewer's own behaviour, not a fault of the measure.

## Pairing and cleanup

Only the public test certificate reaches the device. The bounded native
certificate-control socket is discovered from the service environment. Requests
use the same `operation`, `hostUuid`, `certificate` contract as
`services/korrid/src/host/moonlight_certificate.rs`. A credential-restricted
child connects as the unit's permitted caller and checks the server peer.
The controller revokes any stale grant for this certificate before provisioning,
then revokes the same certificate on exit. It never removes another grant.
A private key pair stays on this host, under a private directory with an
exclusive invocation lock. Never commit it or the cache signing key.

Moonlight Embedded's native HTTP implementation loads `client.pem` and
`key.pem`, and does not load a server certificate file. Its transport disables
TLS verification. The harness's own HTTPS application discovery **does** trust
only the server certificate returned by the authenticated control socket; it
does not persist that certificate. This does not change Embedded's transport
security. Use a trusted test link.

The device agent has one run directory under `/tmp`. SIGINT/SIGTERM/SIGHUP are
blocked during cleanup and delivered after revocation, folder removal and restore
checks finish. SIGINT/SIGTERM, SSH EOF,
and a 75-second host inactivity watchdog all unwind the source and certificate
grant. Cleanup removes that directory and checks source processes and the
Sunshine/compositor/korrid main PIDs plus process start ticks. A failed restore
check invalidates the result and stops the batch: do not continue device work.
The agent itself exits after reporting cleanup. Requests carry correlation IDs,
so cleanup cannot mistake a delayed sample reply for its restoration result after
a signal interrupts a request. A power loss or SIGKILL cannot
run cleanup; the stable certificate lets the next invocation retire its grant.
Do not delete `sunshine_state.json` if provision creates it: it also holds the
server identity. Its prior existence is recorded.

## Builds and focused checks

```sh
# Substitute the locked nixpkgs path returned by this command below.
nix eval --raw --impure --expr '(builtins.getFlake (toString ./.)).inputs.nixpkgs.outPath'
nix build --no-link --impure --expr \
  'let pkgs = import NIXPKGS_PATH { system = "x86_64-linux"; }; in (import ./nix/stream-cadence { inherit pkgs; }).tests'
```

`default.nix` exports `app`, `shim`, `motion`, `capture`, `device`, `tests`, and `sourceCheck`.
`runtime` holds what an agent needs on its target, including the sender
fixture's `sway` and `sunshine` from nixpkgs.
Use the same expression with an architecture-appropriate `pkgs` to build each
output for either supported architecture. Tests compile the actual C bar,
round-trip it through lossy libx264 at three resolutions, inject source discards
and capture drops, test three display ceilings, parse recorded SP procfs/native
IPC/journal samples, exercise real SEQPACKET requests and compare limits, and
reject device constants in production source. A C regression test covers the
process-relative receive-clock bridge that otherwise causes an immediate stop.
Sender-fixture tests bind real ports, parse recorded Sunshine startup logs,
end real stray processes, and run the host-mode viewer fixture with stand-in
programs. The app build depends on the source gate. Tests also run Ruff and
Mypy over every production Python module.

Every device executable is built on this host or configured remote builders.
The controller selects by remote `uname -m`, signs the immutable closures with
`~/.config/korri/cache-key.secret` when available, and uses `nix copy`. A device
must already trust the signing key; the tool never weakens signature checks or
changes Nix configuration. It creates no GC root or profile entry. Each result's
`discovered.imported_store_paths` records the immutable closure roots submitted
to `nix copy`; Nix also copies their referenced dependencies. Imported store
objects can remain until normal GC; all mutable test files live in the one
temporary run directory. First use can
wait for the configured architecture builder. No build is dispatched by a device.

`tests/record-device.py` reads acceptance fixtures. `tests/prepare-device.py` is
an explicit, separate operator utility for the authorized one-time restart or
for blanking a panel before acceptance. The cadence command never invokes it.
Neither helper contains a device address or account. `tests/signal-run.py`
exercises controller SIGINT/SIGTERM after video arrives. The read-only
`tests/verify-device-clean.py` checks persisted revocation, retained server
identity, service PIDs and the removed run folder. To audit an existing recording
without contacting a device:

```sh
./nix/stream-cadence/tests/rescore.py /path/to/run-1/result.json \
  --max-unreadable-frames 200 --out /path/to/rescored.json
```

The audit writes a new result and leaves the original recording and result unchanged.
