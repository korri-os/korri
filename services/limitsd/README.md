# limitsd

Root-only launch-limit effects for ADRs 0019 and 0020. korrid stays unprivileged.
The NixOS product starts `korri-limits.service` at boot, with one systemd-owned
socket at `/run/korri-limits/limits.sock` (`root:korrid`, `0660`). The server
checks Linux `SO_PEERCRED`: only the configured **korrid UID** can request an
effect. Group membership alone does not authorize a peer.

## Wire contract

One connection carries one JSON request line and receives one JSON response
line, then closes. A request is at most 4096 bytes including its newline.
Read and write deadlines are two seconds. Unknown fields, negative/fractional
percents, and unknown request tags are rejected. No request accepts a path,
filename, or command.

```json
{"_tag":"Describe","payload":{}}
{"_tag":"ApplyDevice","payload":{"cpu":{"min":0,"max":61},"gpu":{"min":0,"max":100},"governor":"schedutil"}}
{"_tag":"ApplySession","payload":{"unit":"korri-game-0123456789abcdef0123456789abcdef.service","cpu":{"min":0,"max":40},"gpu":{"min":0,"max":100}}}
{"_tag":"EndSession","payload":{"unit":"korri-game-0123456789abcdef0123456789abcdef.service"}}
```

`Describe` returns this shape (numbers and nodes are hardware-dependent):

```json
{"_tag":"Ok","payload":{"defaults":{"cpu":{"min":0,"max":61},"gpu":{"min":0,"max":100},"governor":"schedutil"},"cpuPolicies":[{"name":"policy0","cpus":[0,1,2,3],"cpuinfoMaxFreq":1800000,"availableFrequencies":[408000,600000],"availableGovernors":["schedutil","performance"]}],"gpuDevfreq":[{"name":"fde60000.gpu","availableFrequencies":[200000000,300000000]}]}}
```

`defaults` always means the Nix boot defaults, not the last `ApplyDevice`.
`governor` is always present. CPU policies report `related_cpus`, sorted
`scaling_available_frequencies` (empty when not provided),
`cpuinfo_max_freq` in **kHz**, and `scaling_available_governors`. GPU nodes
report sorted `available_frequencies` in **Hz**. Only configured GPU devfreq
nodes are managed; an empty node list means no GPU writes.

Every other successful request returns:

```json
{"_tag":"Ok","payload":{}}
```

Every error returns a plain diagnostic message, without a second error schema:

```json
{"_tag":"Err","payload":{"message":"peer uid is not korrid"}}
```

## Effects and recovery

- Percent `min` is an integer from 0 to 100. `max` is an integer from 1 to
  100. `min <= max`. A device governor must be offered by every CPU policy.
- CPU percentages use each policy's own `cpuinfo_max_freq`. Maximum rounds
  down to an available step, at least the lowest step. Minimum rounds up,
  at most the highest step. Without a CPU step list, integer frequencies
  round in the same direction and clamp to `cpuinfo_min_freq`/`cpuinfo_max_freq`.
  GPU percentages use the highest advertised frequency and the same step rules.
  Configured GPU nodes must advertise a nonempty frequency list.
- If rounding alone leaves no step inside the interval, the **rounded max
  wins** and min becomes equal to max. This owner-approved rule preserves
  the heat cap. limitsd logs this at debug level.
- `ApplyDevice` writes cpufreq limits/governor and GPU limits immediately when
  idle. During a session it only stores the next revert target. The running
  session retains its launch-time device baseline.
- A unit must match `^korri-game-[0-9a-f]{32}\.service$`. Its populated cgroup
  must resolve directly under `korri-games.slice`, which systemd nests at
  `/sys/fs/cgroup/korri.slice/korri-games.slice`. A second active session is
  rejected. A stale `EndSession` cannot end another session.
- `ApplySession` keeps the launch-time device **rounded CPU min**, raises each
  policy's max to the larger of device/session max, writes the session
  percentages directly to the unit's `cpu.uclamp.min`/`cpu.uclamp.max`, and
  applies session GPU limits globally. It does not cap other units with uclamp.
- `EndSession`, `cgroup.events` reporting `populated 0`, or a removed game cgroup
  restores device limits. A retained cgroup's uclamp returns to 0/100. Polling
  runs every 500 ms when idle; a stalled request can add up to two seconds.
- A failed session application or end enters recovery. Recovery retries until
  every revert write succeeds, even if the game remains populated. Errors are
  explicit; no successful response hides a failed write.
- Startup applies the Nix defaults. Every 30 seconds limitsd reasserts the
  current state after checking for an ended session. This also repairs
  suspend/resume resets. Failed sysfs writes remount `/sys` writable and retry
  once, following the existing clock-governor posture. Cgroups are not remounted.
- The helper enables the slice's CPU controller and writes its `cpu.uclamp.min`
  to `max` at startup (and reasserts that clamp periodically). This is intended
  to let each game unit's own min take effect. **Kernel clamp nesting remains
  unverified on RG353M hardware.** Kernel thermal throttling is the only guard.

The remaining clock-governor loop owns GPU governor selection and configured
cpuidle disables, never CPU governors or frequency limits.

## Nix and checks

`services.korri.limits` exposes `enable`, read-only `socketPath`,
`defaults.cpu.{min,max}`, `defaults.gpu.{min,max}`, `defaults.governor`, and
`gpuDevfreqNodes`. CPU/GPU defaults are 0/100. Governor and GPU node defaults
use the existing `services.korri.clockGovernor` hardware facts. Only the RG353M
sets a different CPU default (61% max). Runtime device values live in korrid;
limitsd does not persist a competing copy.

Run `nix run .#limitsd-check` for Rust tests, format and clippy checks.
`nix build .#korri-limitsd` also runs the filesystem/socket tests. Fixtures use
real temporary sysfs/cgroup trees and real Unix socket credentials. They need
no hardware and make no writes to the host's `/sys`.
