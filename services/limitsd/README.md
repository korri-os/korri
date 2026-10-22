# limitsd

Root-only launch-limit effects for ADRs 0019 and 0020. korrid stays unprivileged.
The NixOS product starts `korri-limits.service` at boot, with one systemd-owned
socket at `/run/korri-limits/limits.sock` (`root:korrid`, `0660`). The server
checks Linux `SO_PEERCRED`: only the configured **korrid UID** can request an
effect. Group membership alone does not authorize a peer. The server checks
credentials first, then reads one bounded request line even from a denied peer
before replying. A denied peer's bytes are never parsed as JSON. This avoids
closing the socket while the caller is still sending its single request.

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
- CPU percentages use each policy's own `cpuinfo_max_freq` as `top`. GPU
  percentages use the highest advertised frequency as `top`. Each available
  step has the label `round(step * 100 / top)`, **round half up**. This is the
  same label the portal offers. Max selects the highest step whose label is
  `<= max`, or the lowest step if none qualify. Min selects the lowest step
  whose label is `>= min`, or the highest step if none qualify. Selection
  compares these integer labels, not a raw percent-to-frequency threshold.
  On the RG353M, max 61 and min 61 both select 1104000 kHz; max 60 selects
  816000 kHz. Configured GPU nodes must advertise a nonempty frequency list.
- Without a CPU step list, the fallback is unchanged: compute `min * top / 100`
  rounded up and `max * top / 100` rounded down, then clamp both integer
  frequencies to `cpuinfo_min_freq`/`cpuinfo_max_freq`.
- If rounding alone leaves no step inside the interval, the **rounded max
  wins** and min becomes equal to max. This owner-approved rule preserves
  the heat cap. For steps 600/1200 with top 1800, min=max=50 selects 600
  for both bounds. limitsd logs this at debug level.
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
- Startup applies the Nix defaults. Every 30 seconds limitsd rediscovers CPU
  policies and reasserts the current state after checking for an ended session.
  Requests also refresh CPU policy information, so a late cpufreq registration
  cannot leave a new policy unmanaged. This also repairs
  suspend/resume resets. Failed sysfs writes remount `/sys` writable and retry
  once, following the existing clock-governor posture. Cgroups are not remounted.
- The Nix module enables CPU accounting/weight on **both** `korri.slice` and
  `korri-games.slice`. At startup the helper enables `+cpu` in both slices'
  `cgroup.subtree_control` and writes both slices' `cpu.uclamp.min` to `max`
  (and reasserts those clamps periodically). Each ancestor caps the child's
  effective min, so both slices must permit the game's own min. Sibling units
  retain their own default min of 0. **Kernel clamp nesting remains unverified
  on RG353M hardware.** Kernel thermal throttling is the only guard.

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
