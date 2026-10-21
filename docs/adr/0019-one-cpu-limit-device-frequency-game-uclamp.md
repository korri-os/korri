# One CPU limit: frequency on the device, uclamp on the game

**Owner decision, 2026-10-21.**

A person sets one launch setting, the CPU limit, as a `min` and `max` percent
on any layer: device, system, runner, game, release or profile. Korri applies
the device value as CPU frequency limits on every cluster, so it bounds
everything that runs, including the portal and compositor. Korri applies the
merged value from the other layers as a utilization clamp (`cpu.uclamp.min` /
`cpu.uclamp.max`) on the game's own unit only. A light game then cannot slow
the portal, a demanding game can be pinned with `min: 100`, and the clamp ends
with the game's cgroup even after a crash. The person never chooses between the
two mechanisms.

## Consequences

- Every Korri kernel needs `CONFIG_UCLAMP_TASK` and `CONFIG_UCLAMP_TASK_GROUP`.
  On 2026-10-21 only the nixpkgs mainline kernels (RG353M, RG DS) had them; the
  odin2portal, rpminiv2, h700 and r36tmax configs had them off.
- uclamp affects clocks only under `schedutil`. A `performance` governor makes
  the game-level limit meaningless.
- korrid runs unprivileged with `ProtectControlGroups`, and systemd 259 has no
  uclamp unit property, so a privileged step must write the values.
- A percent maps to each cluster's own top clock, rounded to a real step. The
  same percent is not the same amount of work on two chips. That is acceptable
  because CPU limits are device-only (ADR 0018).
- GPU devfreq has no per-process clamp. A GPU limit from any layer limits the
  whole GPU for the session.

## Considered options

- Two separate knobs (frequency and a game CPU share) on every layer: precise,
  but two similar settings everywhere.
- Frequency only, as ROCKNIX does: a capped game also slows the portal and
  compositor during its session.
