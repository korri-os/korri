# A narrow root helper applies launch limits

**Owner decision, 2026-10-21.**

CPU and GPU launch settings (ADR 0019) end as writes to root-owned files:
cpufreq `scaling_min_freq`/`scaling_max_freq` and governor, devfreq
`min_freq`/`max_freq`, and `cpu.uclamp.min`/`cpu.uclamp.max` in a game unit's
cgroup. korrid runs as the unprivileged user `korrid`, sees `/sys/fs/cgroup`
read-only, and holds polkit rights only to manage `korri-game-*` and
`korri-acquire-*` units. systemd 259 has no uclamp unit property.

A small root service applies these limits. korrid sends it only a game unit
name and percents, never a path, file name or command. The helper checks the
unit name and the ranges, maps percents to each cluster's real frequency steps,
writes the files, and puts the device-layer values back when the session ends,
after a crash, and at boot. It owns the CPU governor too: it applies the
device-layer governor instead of the fixed `schedutil` loop in
`nix/base/clock-governor.nix`.

## Considered options

- **Extend the clock-governor shell loop** to read a state file korrid writes.
  Rejected: root shell code would parse values korrid wrote, and limits could
  take up to 30 s to apply.
- **Delegate the game's cgroup** so the launch process sets uclamp itself.
  Rejected: the game's own process could raise its limit, and frequency and GPU
  limits would still need root.
- **Give korrid broader polkit rights** (for example a root `ExecStartPre=+` on
  game units). Rejected: korrid could then run any command as root.
