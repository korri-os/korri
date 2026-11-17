# #391 step F, slice F1: the Rust audio output, in turns

Verdict: **equal on every number, but other work disturbed half the runs.**
Other work took 5.5 to 40.4 % of all CPUs in four of the five B runs and
14.5 % in one A run, so B's ranges are wider than a quiet aka gives. A
second set on a quiet aka would make the result firm.

`stream-cadence view` on aka, 2026-11-17 14:25 to 14:43 UTC: five runs of
each side in the order A B A B, at the defaults with `--tone 1000`, Korri's
patched Sunshine (`--sender-sunshine
/nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri`),
`--build-store auto`, no `--net`. The set started only after three checks a
minute apart found aka quiet (1-minute load below 1.5, k10temp Tctl below
78 °C, no Nix build, no game and no other session's proof), and held
`/var/tmp/korri-turns.lock` on aka while it ran. The other work started
after that.

- A: `/nix/store/k497w9l1wb5q0vcjwwh2zx1gm448y73m-korri-viewer-0.1.0`
  (the C audio output).
- B: `/nix/store/gmx12hwdkl5p1mivhdxdx436i16b0zng-korri-viewer-rust-audio-output-0.1.0`.
- Both built on aka from `origin/main` `124e06840`. Both `x11_vaapi`,
  `decode=vaapi`.
- Every B run's log says "sound: the Rust audio output chose ALSA"; no A
  run's log does.

All 10 runs valid. Record: `turns-2026-11-17/` (`baseline.json`,
`candidate.json`, `verdict.txt` unedited, each run's `viewer.log`).

| ADR 0062 number | Field | A | B | Verdict |
| --- | --- | --- | --- | --- |
| 1 Packet arrival to display | `packet_to_display_ms` | not measured | not measured | not measured |
| 1 (the gate) Sender presentation to display | `latency_p50_ms` | 11.957 to 12.087 | 11.993 to 13.113 | equal |
| | `latency_p95_ms` | 19.060 to 19.290 | 19.155 to 20.595 | equal |
| 2 Late, repeated, dropped | `dropped_frames` | 0 | 0 to 1 | equal |
| | `repeated_frames` | 232 to 238 | 230 to 238 | equal |
| | `late_frames` | 232 to 238 | 229 to 237 | equal |
| | `torn_frames`, `unreadable_frames`, `output_repeats` | 0 | 0 | equal |
| 3 CPU | `cpu_seconds_per_second` | 0.014 to 0.018 | 0.015 to 0.030 | equal |
| 3 Battery | `battery_mw` | not measured | not measured | not measured |
| 4 Startup | `startup_ms` | 144.5 to 160.8 | 128.6 to 160.6 | equal |
| 5 Input | `input_ms` | 0.051 to 0.229 | 0.052 to 0.097 | equal |
| | `input_missed` | 0 to 2 | 0 | equal |
| 6 Stability | `stream_stalls`, `viewer_exits` | 0 | 0 | equal |

The viewer's CPU follows the other work, run by run:

| Run | A: other work | A: `cpu_seconds_per_second` | B: other work | B: `cpu_seconds_per_second` |
| --- | --- | --- | --- | --- |
| 1 | 0.9 % | 0.014 | 5.5 % | 0.019 |
| 2 | 0.9 % | 0.014 | 18.6 % | 0.025 |
| 3 | 3.1 % | 0.016 | 2.5 % | 0.015 |
| 4 | 14.5 % | 0.018 | 40.4 % | 0.030 |
| 5 | 1.1 % | 0.014 | 24.4 % | 0.025 |

In the quiet runs (other work below 5 %) A's CPU is 0.014 to 0.016 and B's
0.015 (one run).
