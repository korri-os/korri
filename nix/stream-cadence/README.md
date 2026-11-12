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

With a product sender, the private half of the test key pair crosses SSH into
the viewer's run directory, because the viewer runs there. Cleanup removes
it. The sender still revokes the certificate on exit.

### The installed viewer on a handheld

On a device with the enabled `@korri:moonlight` plugin, `view` runs the
plugin's own `moonlight` on the device's own compositor and its real panel.
A plugin that is switched off, or whose `korri-plugin status` fails, is not
usable. What happens then depends on the chip:

- korrid's chip table names a hardware picture path for the device (every
  handheld on the bench). The tool refuses the run and says why the plugin is
  not usable. It never runs its software viewer there: that viewer decodes on
  the CPU, it is not the product's picture path, and it heats a passive
  device.
- The table names none. The device gets the copied viewer and the fixture.

The agent runs as root, changes to the compositor's user with that user's
groups, and starts the viewer with the arguments of korrid's `linux_viewer`
(`stream_args`). The `-platform` value comes from a copy of korrid's chip
table in `viewer.py`. `tests/test_korrid_copy.py` reads korrid's file and
fails when the two differ: the chips, each picture path, the default, the
answer for an absent device tree, and whole-value comparison. The source
gate runs it, so the app build depends on it. The plugin's `moonlight` is one
program and not a wrapper, so its CPU time is the viewer's.

This is not a korrid launch. The differences:

- No `korri-game-*` systemd unit runs: no unit sandbox, no cgroup, no unit
  credentials. The viewer is a child of the agent.
- The arguments carry the tool's request (`-width`, `-height`, `-fps`,
  `-codec`, `-bitrate`), not the settings that the plugin renders for a
  launch.
- `-app` is the sender's application: `Desktop` with the sender fixture.
  korrid asks for `Korri Stream`.
- `-viewonly` is set and no `-input` seat is given.
- Sound goes to SDL's dummy driver. The product's audio server exists only
  inside a korrid launch.
- `HOME`, `XDG_RUNTIME_DIR`, `XDG_CACHE_HOME`, `TMPDIR` and the key directory
  are in the run directory. The viewer's shader cache therefore starts empty
  in each run.
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
| Result | invalid: gaps that lose an ID | valid |
| Output-frame records | 1932 in 32.5 s, 59.47 each second | 1955 in 32.6 s, 60.00 each second |
| Refreshes with no record | about 15 | 0 |
| IDs lost at a gap in the record | 11 | 0 |
| Shortest interval between two records | 10.2 ms (0.61 period) | 10.4 ms (0.62 period) |
| Intervals from 0.50 to 0.75 period | 16 of 1931 | 5 of 1954 |
| `startup_ms` | 1990.5 | 1569.1 |
| Viewer CPU, CPU seconds each second | 0.147 | 0.135 |

**The next run on the RG353M was not this path, and it reset the device.**
In that run `korri-plugin status` gave no enabled plugin, to the tool and to
the agent alike. The tool kept no reason, so the cause is not known. The
tool then did what it did for a server-only device: it copied the nixpkgs
viewer and ran it in the fixture, at 1920x1080 with software decode, on an
RK3566. That viewer used about 1 to 2 CPU seconds each second. The device
left the USB link 5 s into the measured stream and rebooted. No cause is
proven; a thermal reset fits. With nothing of the tool running, that device
reads 69 to 74 °C, and its first passive trip point is 70 °C (its korrid
uses one whole CPU, issue #165). The tool now refuses that case, and keeps
the reason when the plugin is not usable. No handheld baseline exists.

By the gap rule of "Capture gaps", the RG353M run stays invalid: 14 of the
15 gaps of its record lose an ID. The RG35XX Pro run has no gap.

### The heat guard

A viewer measure can heat a passive device, and a hot host must not get more
load from this tool. The viewer's agent guards the viewer side of every run,
and the sender's agent guards the sender fixture with the same rules. Each
reads the thermal zones and their trip points from `/sys/class/thermal` at
run time. No temperature of a device is in the tool. The product sender
(`device.py`) starts nothing but the source and has no guard.

The tool's own rules, in `heat.py` for both agents and in `viewer.py` for the
viewer alone:

| Rule | Limit |
| --- | --- |
| A run does not start | at or above a zone's lowest passive trip point. With no passive trip point: within 20 °C of its critical trip point. |
| A run ends at once | 10 °C below a zone's critical trip point, or half-way between its highest passive trip point and its critical trip point when that is lower. |
| A run does not start, and a running viewer is stopped | when the guard cannot read: a zone that has a limit is gone or gives no temperature, a zone's trip points cannot be read, or the guard's own thread fails. |
| A device with a hardware picture path does not run | when no zone has a critical trip point: the guard has no limit for it. |
| A device with a device tree (`/proc/device-tree`) hosts no fixture | ever: no software viewer and no sender fixture. |
| A viewer on a hardware picture path is stopped | when any line of its log says that a frame did not come from the hardware decoder, or when it uses more than 1.0 CPU second each second for 2 s. |
| A valid run on a hardware picture path | needs the log line of the measured start that says hardware decode started. |

**A device with no thermal zone.** The decision: a device with a hardware
picture path does not run without a zone that gives a limit, because such a
device is a passive handheld. Any other device with no such zone runs
unguarded, and `discovered.viewer.heat_guarded` is then false. aka is such a
device: it has no zone under `/sys/class/thermal` at all and no hardware
picture path, so a viewer measure on aka runs with no heat guard.

A zone with readable trip points and no critical one is recorded and not
judged. A trip point at or below zero is a placeholder and counts as absent.

The tool asks the viewer's agent for the start rules before it starts
anything, the sender fixture included. A thread of each agent then looks
four times each second and stops what that agent runs by itself: the viewer,
or the fixture's Sunshine, compositor and source. A slow host or link cannot
delay the stop. The run ends as invalid with the reason, also when the stop
comes after the last sample, and cleanup runs as after any run.

A hot host is therefore refused as viewer and as sender. That is intended.

Each viewer start writes its own `viewer.log`; the log of the locating start
is kept as `viewer.earlier.log`. Each viewer sample holds `thermal` (a list:
zone, type and temperature in millidegrees, or no temperature when it cannot
be read) and `hardware_decode`, and the samples of the locating start are
kept too. Each run prints the lowest and highest temperature of the guarded
zones. `discovered.viewer.thermal` in the result holds the zones, their trip
points and the limits that the tool derived.

`--test-heat-end-margin DEGREES` is for a test of the guard only. It moves
the end limit down, so that the guard ends a run on a cool device. It cannot
move the limit up: a margin below the tool's own 10 °C is not taken.

For the RG353M the limits are: start below 70 °C, end at 85 °C (critical 95).
For the RG35XX Pro: start below 60 °C, end at 90 °C (critical 110). The build
host's CPU zone has a critical trip point of 110 °C and no passive one; it
read 99 °C on 2026-11-11, so the guard refuses the viewer on that host until
it is cooler.

### The sender fixture

```sh
nix run .#stream-cadence -- view --sender local --viewer local --sender-fixture
nix run .#stream-cadence -- view --sender local --viewer operator@viewer \
  --sender-fixture --viewer-stream-host ADDRESS_OF_THIS_HOST
# The second measure of a private pair: the viewer's queue.
nix run .#stream-cadence -- view --sender local --viewer local --sender-fixture \
  --queue-measure
```

`--sender-fixture` gives the measure a private sender. The sender target
then needs no product Sunshine, no certificate socket and no product
display.

**A game does not stop a private pair.** A private pair is the sender
fixture with the fixture viewer. It uses neither the device's display nor the
product Sunshine, so a `korri-game-*` unit neither refuses its start nor ends
its run. Every other rule stays on. Everything that uses the device's display
or the product Sunshine keeps the refusal: `run`, `view` against a product
sender, and the installed viewer with any sender.

The evidence for this is one trial on aka on 2026-11-12: a 10 s run and a
60 s run of the pair while the owner played a game there and watched. Both
runs were valid, the product PIDs and `/dev/input` were unchanged, and the
owner noticed nothing in the game. What the pair costs a player is not
measured: the tool reads nothing of the game. One person, one game and one
device is not proof for another game or device.

Every result names the game units that were live on each device at the start
and at the end of the run (`discovered.live_game`), without their command
lines.

**A game changes the pair's numbers.** In the 60 s run of that trial the
delay p50 of the six windows was 23.9 to 26.0 ms. On aka with no game, the
same pair at the same defaults gave 15.4 to 17.0 ms over 60 windows (sets A
and B below), when aka's load was near 0.3. Later in the night of the trial,
with no game and a load of 3 to 7, three runs of that pair gave 21.8 to
23.0 ms, and one run beside the game gave 25.2 to 26.9 ms. So the game
added about 3 ms that night, and aka's other load moved the number by more
than that. The size of the difference is not a rule; that there is a
difference is the reason for this rule:
`verdict` does not compare a result that was recorded beside a game with one
that was not. A game at the start or at the end of any run, on either
device, makes the result "beside a game" (condition `beside_a_game`). A game
that starts and ends between those two reads is not seen. Two results that
were both recorded beside a game are compared; the tool does not know
whether it was the same game or how much of the machine it used, so prefer
results with no game for a verdict.

**Where the fixtures run.** Both fixtures, sender and viewer, are for a
machine that can carry a software load: a server, a desktop or a laptop.
A passive handheld runs only its installed viewer: it has no fan, and a
software compositor, viewer or encoder heats it. The fixtures do not start:

- on a device that has a device tree (`/proc/device-tree`). The tool takes
  every such device for a passive handheld. Nothing else marks them all:
  korrid's table of hardware picture paths does not name every chip, and a
  handheld's kernel can show no power supply. The R36T Max is the case: its
  chip (`rockchip,rk3326`) is not in the table, and its device tree leaves
  the charger out, so `/sys/class/power_supply` is empty;
- on a hot host, or where the heat guard cannot read (see "The heat guard").

The refusal says which of these it is and what the rule is for. korrid's
chip table still decides the rules of the installed viewer (the heat guard
and the decode rules); a chip in it also refuses the fixtures by itself.

A laptop, a desktop or a server with ACPI firmware has no device tree, so
the rule lets it through. A laptop has fans, and thermal zones that the
heat guard reads; that guard decides there. The build host is such a
laptop. Today the heat guard refuses it, because it reads within 20 °C of
its critical trip point.

The rule can be wrong both ways, and the cost is accepted. An ARM builder,
board or laptop with a device tree is refused, also on mains power. A
handheld with x86 firmware has no device tree and is let through; only the
heat guard protects it. No such device has been tried.

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
| Render node | Each node in `/dev/dri` that the fixture's user can open, tried as that user, in order. The first node on which Sunshine finds a hardware encoder wins. With none, the first node that encodes at all. |
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
60, which is the default. (`--queue-measure` asks for 67 and gets 66.666 Hz,
15 ms; see "Two measures for a private pair".)
The output is then the slower side, Sunshine takes every frame, and the
source shows about 57.8 frames each second. `requested_fps` stays the
request, and `display.refresh_hz` is the mode.

Two results follow from that rate.

- **A floor on a 60 Hz panel.** The panel refreshes about 2.2 times each
  second more often than the fixture sends a frame (60 minus 57.8). Each of
  those refreshes repeats a picture, so a viewer on a 60 Hz panel shows about
  2.2 `repeated_frames` and `late_frames` each second that come from the fixture,
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
another device reaches the sender at the far end of its agent's SSH
connection, or at `--viewer-stream-host`. The tool does not change a
firewall.

**On a Korri device, beside the product.** With an SSH target the agent is
root, as for a product measure. Then:

- Nothing of the fixture runs as root. The compositor, the source, Sunshine
  and `swaymsg` take the product compositor's user, with only the
  supplementary groups that own a render node. As root on a host with no
  `korri-compositor`, or where that compositor itself runs as root, the
  fixture refuses; so does the viewer's agent.
- The fixture makes no input device. Stock Sunshine makes a virtual mouse
  and keyboard when it starts, and a touch screen and a pen tablet for each
  client, all through `/dev/uinput`; the product's input stack would see
  them. The fixture does not start where its user can open `/dev/uinput`;
  the open is tried as that user, so ACLs count.
  Its Sunshine has `mouse`, `keyboard` and `controller` off. A new entry
  under `/dev/input` during a run ends the run and fails the restore check
  (`restore.new_input_devices`), whoever made it.
- A `korri-game-*` unit does not stop the fixture; the result names the
  units that are live (see above).
- The tool itself asks the fixture's Sunshine nothing. With both targets on
  one device the stream uses loopback, which a device's firewall admits.
- The product's Sunshine, compositor and korrid are not read by the
  sender's agent, except for the compositor's user. The viewer's agent
  compares the compositor's PID as before.

**Files.** The agent owns the run directory and writes only there: its
logs, `sway.conf`, `sunshine.conf`. The fixture's programs write in `home`
and `rt`, which the agent hands to their user as its last step of preparing.
Afterwards the agent creates and writes no file in them. It reads there, and
it removes names there: a socket that an earlier start of the compositor
left in `rt`, and the last Sunshine log in `home`, before each start of the
sender's stack. To remove a name follows no link: a link in that place is
removed itself, and what it points to is not touched. Every file
that an agent creates, appends, reads or hands over in a run directory is
opened without following a link, so a program of the run cannot send a root
agent to a file outside. The viewer's agent keeps the same layout: the
viewer's keys and caches and the capture's record are in `home`.

The product sender's agent (`device.py`) has the same layout: `motion.log`
is the agent's, in the run directory, and the frame-ID source has `home`
for its own files. Before, that root agent gave the whole run directory to
the compositor's user and then wrote and read `motion.log` there through
calls that follow a link. `run` and `view` against a product sender have
not run since this change; a test with no product covers the files only.

**Cleanup.** Each child of the agent ends when the agent dies, however it
dies. Each program of the fixture starts in a session of its own. `close`
stops the source, Sunshine and the compositor, then looks for any process
that still carries the run directory in its arguments, environment or
working directory. It kills one only when the fixture's user owns it and it
is in a session that the agent started, and lists it in
`restore.stray_processes_killed`. Any other match is listed in
`restore.other_processes` and left alive: an operator's shell in the run
directory is not the agent's to kill. The restore check fails the run when a
fixture process or the directory is left, and also when a listed process
belongs to the fixture's user, because it may be a fixture process that left
its session.

**What the fixture does not prove.** It is not the product's sender. It does
not use the product's KMS capture, its encoder settings, its pacing or its
display. Sound is off. With both targets on one host the stream uses
loopback, so the network is not measured, and the sender and the viewer
share one machine's CPU. The numbers compare an old and a new viewer against
the same test sender. `run` still measures the product sender. `verdict`
refuses to compare a fixture result with a product-sender result: the
capture backend and the display mode differ. It also refuses two fixture
results from different sender hosts, render nodes or viewer devices.

Not tested yet: the fixture on aarch64, and two encode sessions at one time
on one graphics card.

**The private pair on aka, 2026-11-12.** Sender fixture and viewer fixture
both on aka (`view --sender root@aka --sender-fixture --viewer root@aka`), at
the defaults: 1920x1080, a request of 60, 60 scored seconds. This is the
base measure of "Two measures for a private pair" below. The fixture's
Sunshine started beside the product's and encoded with the AMD card's VAAPI
H.264 encoder, as the user `korri`. Two sets of five runs. Set B's build is
commit `6c092579f`. Set A's build came from the tree just before that commit
and lacks three lines of it, which remove a stale compositor socket before a
second render-node attempt; no run on aka made a second attempt.

| | Set A | Set B |
| --- | --- | --- |
| Valid runs | 5 of 5 | 5 of 5 |
| `source_misses`, each run | 0 | 0 |
| Dropped frames, each run | 0 | 0 |
| Sender presentation interval, p50 | 17.18 to 17.21 ms | 17.17 to 17.21 ms |
| Sender presentation interval, p99 | 17.55 to 17.59 ms | 17.54 to 17.60 ms |
| Longest sender interval | 17.7 to 23.0 ms | 17.8 to 19.8 ms |
| Sender intervals above 1.5 periods | 0 | 0 |
| Latency p50 | 15.59 to 16.14 ms | 15.55 to 16.27 ms |
| Viewer CPU, CPU seconds each second | 1.26 to 1.30 | 1.25 to 1.30 |
| `startup_ms` | 675 to 802 | 673 to 703 |
| aka's load before and after | 0.30 and 2.22 | 2.04 and 1.53 |

`verdict` between the two sets says `equal` for each measured number. The
sender is steady on aka: of 44 453 presentation intervals none is above 1.5
periods. On the hot build host the same fixture had 29 such intervals in
21 800 and needed four attempts for each set. The 138 to 149 repeated and late
frames of each run are the floor of the fixture against the viewer
fixture's 60.5 output frames each second.

The product did not notice: the main PIDs and start times of
`korri-sunshine`, `korri-compositor` and `korrid` were the same before and
after, `/dev/input` had the same 27 entries through every run, and
`korri-sunshine` and `korri-compositor` logged nothing in that time. The
product's Sunshine had no stream then, so two encode sessions at one time
are not tested. aka has no thermal zone, so these runs had no heat guard.
These sets are evidence, not a baseline: the viewer is the nixpkgs viewer in
the fixture, not the product's.

**Two measures for a private pair.** A comparison of two viewers needs two
results, because no single setup of the pair shows everything.

| | Base measure | Queue measure |
| --- | --- | --- |
| How | the default | `--queue-measure` |
| Rates | The sender is the slower side: a request of the viewer's refresh, which gives a 17 ms sender output against the viewer fixture's 16 ms output at 60. | The sender is the faster side: its output period is one millisecond below the viewer fixture's, 15 ms against 16 ms at 60 (a request of 67; `cli.py:pair_rate` knows the number). |
| Shows | The delay with no queue, startup, CPU, and the viewer's own dropped frames: the sender adds none. | How deep the viewer's queue gets and what the delay settles at. |
| Hides | The queue. The viewer always waits for a frame, so two viewers that differ only in how they queue measure equal. | The viewer's own dropped frames. The sender shows more frames than the viewer can show, and something must drop them: 87 to 294 in a run of 60 s on aka, more or fewer with the sender's rate. |
| Floor from the pair | About 110 to 149 `repeated_frames` and `late_frames` in a run of 60 s. | The dropped frames above. No repeats. |
| `discovered.measure` | `base`: the sender's output has the longer frame period | `queue`: the sender's output has the shorter frame period |

A baseline for a private pair has both parts: one result of each measure,
for the baseline viewer and for the candidate. `verdict` compares base with
base and queue with queue. It refuses a base result against a queue result
and says that this is the cause.

In a queue result the dropped frames are the sender's floor and not a viewer
number. The result says so in `viewer.not_measured.dropped_frames`, and
`verdict` gives `dropped_frames` no verdict there. The count stays in the
windows (`capture_misses`) as evidence.

The two output periods name the measure, not the option. The tool takes the
mode that the sender's compositor reports and the mode that it gives the
viewer fixture, and compares their whole-millisecond frame periods
(`cli.py:pair_measure`). So a private pair with `--fps 67`, or with
`--fps 63 --viewer-refresh 58.823`, is a queue result with the note on its
dropped frames, like one that `--queue-measure` asked for. `verdict` still
keeps two results with different rates apart.

A private pair whose two periods are equal does not run: `--fps 63` at the
default viewer refresh (16 ms against 16 ms), or `--viewer-refresh 58.823`
alone (17 ms against 17 ms). The refusal says that an equal pair gives no
steady measure, and it comes before the sender starts. The tries below are
the reason.

`--queue-measure` is the named way to ask for the queue pair. It is accepted
only for a private pair, and not with `--fps`: the measure sets the rate.
With an installed viewer the sender is asked for the panel's rate, as
before, and the result is a base result whatever the rate: a real panel has
its own clock, and the tool does not compare it with the sender's period.

**Why two: the tries on aka.** Against the product sender on aka, the same
fixture viewer's delay climbed from about 18 ms in steps of one refresh
period and stayed near 51.5 ms: it queues frames and never drops one to
catch up. Whether a pair shows that depends on the two rates. Both fixtures
show on a headless output with a period of whole milliseconds, and each
compositor's own work adds about 0.2 to 1.4 ms to its period, more or less
with the machine's load. Five pairs ran on aka on 2026-11-12 with the tool's
options alone, 60 scored seconds each. "Game" says whether a game ran on aka
(`discovered.live_game`).

| Pair (sender, viewer) | Options then | Game | Runs | Sender, each second | Viewer output, each second | Delay p50, each 10 s window | Dropped, each run |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 17 ms, 16 ms (base measure) | `--fps 60` | yes | 1 | 57.6 | 60.0 | 25.2 to 26.9 ms | 0 |
| 17 ms, 16 ms (base measure) | same | no | 3 | 57.9 | 60.2 | 21.8 to 23.0 ms | 0 |
| 17 ms, 17 ms | `--fps 60 --viewer-refresh 58.823` | yes | 3 | 57.0 to 57.3 | 56.5 to 56.7 | 50.8 to 67.1 ms | 20 to 52 |
| 17 ms, 17 ms | same | no | 1 | 56.9 | 56.8 | 66.9 to 67.4 ms | 5 |
| 16 ms, 16 ms | `--fps 63` | yes | 4 | 60.3 to 60.5 | 59.8 to 60.0 | 46.1 to 63.3 ms | 15 to 57 |
| 16 ms, 16 ms | same | no | 2 | 59.9 to 60.3 | 60.1 to 60.3 | 30.2 to 63.6 ms | 3 to 8 |
| 16 ms, 17 ms | `--fps 63 --viewer-refresh 58.823` | yes | 1 | 60.8 | 56.6 | 62.2 to 63.9 ms | 250 |
| 16 ms, 17 ms | same | no | 1 | 60.9 | 56.8 | 62.6 to 63.4 ms | 247 |
| 15 ms, 16 ms (queue measure) | `--fps 67`, and once no option | no | 7 | 61.3 to 64.4 | 59.5 to 60.6 | 57.3 to 63.2 ms | 87 to 294 |

These runs are from the builds of that night. `--queue-measure` did not
exist then; the 15 ms runs asked for the same rate with `--fps 67`, and one
with no option, from a build in which that rate was the default. Today's
tool refuses the two pairs with equal periods. One more run of the 17 ms
pair began with a game and ended with none; it is in no row.

One run of each measure by its name followed on aka, from the build of
`f5333d99a`, with no game, run by the coordinator of this work
(`/tmp/korri-432/tip-base` and `tip-queue`). Both are valid. The base run
has a delay p50 of 20.3 to 21.3 ms in its windows, 0 dropped frames and 110
repeated frames. The `--queue-measure` run has 61.4 to 61.8 ms, 142 dropped
frames and 1 repeated frame. In that build the option named the measure;
the periods name it since.

What the tries show:

- **The climb.** With one-second windows the delay of the 17 ms pair went 32,
  49, 49, 59, 67 ms in its first five seconds and stayed at 67. The 16 ms
  pair stayed at 30 ms for four seconds, then went 46, 61, 63 and stayed.
  Both were beside a game. The steps are one frame period.
- **Equal periods do not give a steady measure.** The two rates are then
  equal within what the load changes. In one run of the 16 ms pair on aka
  with no game the two rates differed by 0.02 frames each second: the delay
  p50 was 30 ms in two windows, 47 ms in the next two, and 64 ms in the
  last. The same viewer gives 30 or 64 ms by the moment of the window.
- **A sender that is one millisecond faster fills the queue in every
  window.** In all nine runs of the last three rows the delay p50 of every
  scored window is between 57.3 and 63.9 ms. `verdict` between two sets of
  three runs of the 15 ms pair said `equal` for each measured number.
- **The slower sender adds no drops.** All four runs of the first two rows
  have 0 dropped frames, as have the ten runs of sets A and B.

Open points of the queue measure:

- The sender's rate at 15 ms moved from 64.4 to 61.3 frames each second
  over seven runs in 25 minutes. The cause is not known. At 60.6 the sender
  is no faster than the viewer and the queue is no longer sure; the result's
  sender intervals show the rate (`tests/intervals.py`).
- It has not run beside a game. The 16 ms sender against the 17 ms viewer
  has, once.
- It has not run with a viewer that drops frames to catch up. The
  expectation that such a viewer shows a short delay here is not tested.
- Two of the seven runs at 15 ms have one sender interval near 142 ms (8
  `source_misses` each). Sets A and B had no interval above 1.5 periods in
  44 453. Over that night, runs with a sender interval above 1.5 periods
  were: 1 of 10 with a 17 ms sender (35.9 ms, as a game ended), 2 of 9 with
  a 16 ms sender (79 and 350 ms), 2 of 7 with a 15 ms sender. A set of three
  runs of the 17 ms pair after the 15 ms sets, with no game, had none in
  13 298 intervals. So a faster sender output may stall more often; the
  counts are too small to say. Such a run stays valid, and its
  `source_misses`, `repeated_frames` and longest delay show the stall.
- All of this is one machine in one night.

A headless output cannot run at 60.0, so no pair of headless outputs has
equal rates by construction.

For a real 60 Hz panel (not tested): a 17 ms sender shows about 58 frames
each second, adds about 2 repeats each second and never fills a viewer's
queue. A 16 ms sender with `--fps 63` showed 59.9 to 60.8 on aka, which is
too near 60 to say which side is faster. A source with its own clock at the
panel's refresh would add neither repeats nor drops. The source has no such
mode yet.

History: on 2026-11-11 two sets of five runs on the build host gave `equal`
for each number, at 1280x720 on a host at 100 °C; that proof is void, because
four of its ten records hold a gap that loses an ID by today's gap rule, and
it is not recorded again while that host is hot.

`tests/intervals.py RESULTS.json` prints, for each run, the interval
distribution of the sender's presentations and of the new IDs on the viewer's
display. A long tail in the first is the sender's own.

### The six numbers of ADR 0062

| Number | Fields | Method |
| --- | --- | --- |
| 1 Packet arrival to display | `viewer.packet_to_display_ms` | Not measured: today's viewer reports no arrival time. The gate is `latency_ms`, sender presentation to viewer display, on the same sender and link. |
| 2 Late, repeated, dropped | `capture_misses`, `repeated_frames`, `late_frames`, `torn_frames`, `undecodable_frames` in each window; `viewer.output_repeats` | A dropped ID was presented and never displayed. A repeated frame shows no new ID. A late frame is a new ID more than 1.5 refresh periods after the previous new ID. An output repeat is a gap in the display record that loses no ID: the panel showed one picture across it. |
| 3 CPU and battery | `viewer.cpu_seconds_per_second`, `viewer.battery_mw` | Viewer process CPU time from procfs over the scored time. Battery power only while a battery discharges in every sample; else not measured. |
| 4 Startup | `viewer.startup_ms` | Viewer process start to the first displayed frame of that start: the first ID that the sender presented at or after it. korrid's launch path is not included. On a handheld the time contains the agent's fullscreen and focus step. |
| 5 Input | `viewer.input_ms` | Not measured: the press-to-packet measure is not built. |
| 6 Stability | `viewer.stream_stalls`, `viewer.viewer_exits` | A stall is one second without a new ID in the scored time. A viewer exit ends the run as invalid. |

`viewer.not_measured` gives the reason for each number that has no value. No
number is estimated. An unreadable output frame is a count, not an invalid
recording: a viewer can show a frame without a whole bar.

**Capture gaps.** A gap is an interval above 1.5 refresh periods between two
records. `score.py:capture_gaps` judges each gap by the display times and the
IDs of the record, and by nothing else:

- **A gap that loses an ID** counts in `viewer.capture_gaps`. The display
  record is then incomplete, because nothing can tell a frame that the
  capture missed from a frame that the viewer dropped. The default limit is
  zero, and a run above `--max-capture-gaps` is invalid.
- **A gap that loses no ID** counts in `viewer.output_repeats`, which
  `verdict` judges, and the run stays valid. The panel showed one picture
  across the gap. At worst the record lacks one output frame that repeated
  that picture.

How a gap is judged:

- **Which IDs are lost.** The source counts its IDs up by one. An ID is lost
  when it is nowhere in the record and the sender did not discard it itself.
  The sender's presentation log is not asked, so a log that lacks a line
  hides nothing.
- **Which lost IDs belong to a gap.** Display order places a lost ID only
  between its neighbours. Each gap is judged by the two readable records
  before it and the two after it: an ID between their oldest and newest that
  is lost makes the gap lose an ID. The one record of margin on each side is
  on purpose. A lost ID right beside a gap can belong to it, and a torn frame
  shows two IDs. IDs far from a gap decide nothing about it.
- **Order.** Those four records must be in order. A gap where they are not,
  or with no readable record on one side, cannot be judged and counts as
  losing an ID.
- **The scored time.** A gap counts when any part of it lies in the scored
  time. A gap after the scored end also counts when it loses an ID that the
  sender presented inside the scored time, because `score()` books a lost ID
  by the sender's time. A gap there that cannot be judged counts when any ID
  that the records around it span was presented inside the scored time.

A lost ID with no gap within one record of it is a dropped frame
(`capture_misses`) and no gap.

**What this costs.** A viewer that skips a late frame to catch up makes a
gap that loses an ID. The record cannot tell that from a missed capture, so
with the default limit such a viewer gives no valid run, and no set of five.
The default stays at zero; a method that counts vblanks is the way out, not
a higher limit.

**What the rule cannot see.**

- The capture misses a frame between two commits that are less than 1.5
  periods apart in sum. No gap exists, and the missed ID counts as a dropped
  frame of the viewer. With intervals down to 0.61 period that can happen.
- The capture misses an output frame that repeated a picture, with no gap.
  The record lacks one repeat and nothing shows it.
- An ID that the sender's log wrongly marks as discarded is never lost.

`copy_ns` stays in the record as evidence. No rule and no verdict metric
reads it. An earlier rule used it and assumed that two compositor commits are
at least one period apart. On a real panel they are not: after an idle
output, sway commits at once, and its next commit can follow at the next
vblank. The commit times of both bench handhelds scatter between about 0.6
and 1.25 periods (one short run each on 2026-11-11, `tests/intervals.py`):

| Interval between two records, in periods | RG353M, of 1931 | RG35XX Pro, of 1954 |
| --- | --- | --- |
| 0.50 to 0.75 (shortest: 0.61 and 0.62) | 16 | 5 |
| 0.75 to 1.00 | 923 | 978 |
| 1.00 to 1.25 | 951 | 970 |
| 1.25 to 1.50 | 23 | 1 |
| above 1.50: a gap | 18 | 0 |

In that RG353M run the earlier rule called one gap an output repeat although
an ID was lost in it. From the first readable frame on, 14 of 15 gaps lose
an ID by the rule above; 3 more gaps lie before that frame.

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
the requested resolution, frame rate, codec, bitrate and viewer refresh,
the limit `max_capture_gaps` of the run, whether a game ran on a device
during a run, and the measure of a private pair (base or queue). A
difference there would judge the sender or the device, not the viewer. The
model is the device's own name for itself; on a board whose firmware gives a
placeholder (aka says "System Product Name"), two such machines look alike.
When only one result has the condition `viewer_model`, `verdict` says that
this result was recorded before the tool kept it. For every other condition
an absent value is a value: a product sender has no render node, and a
result with no `live_game` or `measure` ran with no game and is a base
result. An unwatched
display keeps the mode of the last viewer that shaped it, so check the mode
before a candidate run. `--allow-different-conditions` overrides the refusal.

Baselines live in `baselines/<device>/results.json`. They are measurement
data and are not part of the tool's Nix source.

| Baseline | Recorded | Viewer | What it proves |
| --- | --- | --- | --- |
| `baselines/aka` | 2026-11-11, five runs | nixpkgs `moonlight-embedded` 2.7.1, `sdl` system, in the fixture | The session part and network library on aka. Record it again with the published `@korri:moonlight` x86_64 plugin before stage 1 of #391 compares against it. |

That baseline must be recorded again before any comparison. It predates the
condition `viewer_model`, so `verdict` refuses it against every result of
today's tool. It also predates the gap rule above and
three things that now run on the measured device during a measure: the check
for a `korri-game-*` unit (one `systemctl list-units` every 2 s), the
thermal reads of the heat guard, and the guard's thread.

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
