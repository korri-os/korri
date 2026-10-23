# Eden / Super Mario Odyssey on the Retroid Pocket Mini V2 — not accepted, 2026-10-23

Super Mario Odyssey (SMO) does not run in Eden on the Retroid Pocket Mini V2
as configured. Eden exhausts the device's memory before gameplay. The 30 FPS
target is not met, and no FPS result exists.

## Scope

| Item | Value |
|---|---|
| Device | Retroid Pocket Mini V2, SM8250, Adreno 650 |
| System | kernel 7.2.0, Mesa 26.2.3 (Turnip), Gamescope 3.16.18 |
| Memory | 5,559 MiB, no swap or zram |
| Plugin | the same personally signed build as the Odin acceptance |
| Eden memory layout | native 4 GB (the Switch minimum), unchanged |

## What worked

- Signed install through `korri-plugin inspect`, `install` and `enable`.
- Offline Switch source pairing, and discovery of SMO.
- The user's save, recorded into history through session-end capture.
- First light on Turnip at the title screen.

## What failed

| Attempt | Outcome |
|---|---|
| NCE | Frames stopped before the end of warm-up. Peak summed session RSS 5,215,624 KiB; minimum MemAvailable 36,248 KiB; full memory pressure (PSI avg10) peak 30.63% |
| Dynarmic | Kernel OOM killed Eden at 17:23:34 UTC. Shmem RSS 3,983,652 KiB and anon RSS 870,232 KiB. korrid was then `HostRecoveryBlocked` on that launch until korrid restarted |
| Half resolution (0.5×), NCE | The device stopped answering SSH, though ping and TCP still worked; memory thrashing is inferred. It did not recover within 45 minutes |

These Eden memory settings were already at their lightest:
- VRAM usage mode is Conservative.
- ASTC textures stay native on the A650 (all 28 formats).
- The disk shader cache is on.

The remaining ladder rungs were not reached: clocks, mods, accuracy, big-core pinning and Turnip flags.

## Open state on the device

The Mini V2 needs a physical power-button reset, and only the user can do it.
After the reset, and before Eden runs again:

1. Prove that launch `1fdb894c480392ffb90c1d63df2cea0a` is gone (empty cgroup).
2. Restore the test account's Eden INI with korri-plugins
   `plugins/eden/bench/samples/rpminiv2-memory/RESTORE.sh.txt`, which checks
   it by hash. The 0.5× resolution and guest-P1-to-Seat-P2 test values must not
   stay.

## Decisions for the user

These are system changes, so none were made:

- **zram:** compressed swap in RAM, the usual fix on low-RAM ARM handhelds. It costs CPU time and some latency.
- **A smaller memory footprint for the portal and services** while a game runs.

Either choice needs a fresh run of this acceptance.

## Evidence

korri-plugins `plugins/eden/bench/samples/rpminiv2-memory/` and
`rpminiv2-staging/` (redacted). The raw evidence is in the private archive, by
hash.
