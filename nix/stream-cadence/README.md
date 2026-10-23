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
Desktop application. The fresh session journal supplies the encoder and capture
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

`default.nix` exports `app`, `shim`, `motion`, `device`, `tests`, and `sourceCheck`.
Use the same expression with an architecture-appropriate `pkgs` to build each
output for either supported architecture. Tests compile the actual C bar,
round-trip it through lossy libx264 at three resolutions, inject source discards
and capture drops, test three display ceilings, parse recorded SP procfs/native
IPC/journal samples, exercise real SEQPACKET requests and compare limits, and
reject device constants in production source. A C regression test covers the
process-relative receive-clock bridge that otherwise causes an immediate stop.
The app build depends on the source gate. Tests also run Ruff and Mypy over all
six production Python modules.

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
