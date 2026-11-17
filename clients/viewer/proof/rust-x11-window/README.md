# #391 step F, slice F4: the Rust X11 window, in turns

Verdict: **equal on every number**.

`stream-cadence view` on aka, 2026-11-17 11:52 to 12:08 UTC: five runs of
each side in the order A B A B, at the defaults, Korri's patched Sunshine
(`--sender-sunshine
/nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri`),
`--build-store auto`, no `--net`. The set started only after three checks a
minute apart found aka quiet (1-minute load below 1.5, k10temp Tctl below
78 °C, no Nix build, no game and no other session's proof), and held
`/var/tmp/korri-turns.lock` on aka while it ran.

- A: `/nix/store/l85f52rx3cp6zdap6fbx5qf3wvb41sgz-korri-viewer-0.1.0`.
- B: `/nix/store/17wypgs4yipyxp8k9j95nzd58i3bs8gk-korri-viewer-rust-x11-window-0.1.0`.
- Both built by the step F worker (roots in
  `/nix/var/nix/gcroots/korri-391-step-f/` on aka). Both `x11_vaapi`,
  `decode=vaapi`.

All 10 runs valid; other work on aka 0.9 to 1.0 % of all CPUs.
Record: `turns-2026-11-17/` (`baseline.json`, `candidate.json`,
`verdict.txt` unedited, each run's `viewer.log`).

| ADR 0062 number | Field | A | B | Verdict |
| --- | --- | --- | --- | --- |
| 1 Packet arrival to display | `packet_to_display_ms` | not measured | not measured | not measured |
| 1 (the gate) Sender presentation to display | `latency_p50_ms` | 11.871 to 11.950 | 11.877 to 12.024 | equal |
| | `latency_p95_ms` | 19.067 to 19.143 | 19.049 to 19.190 | equal |
| 2 Late, repeated, dropped | `dropped_frames` | 0 | 0 | equal |
| | `repeated_frames`, `late_frames` | 232 to 235 | 231 to 235 | equal |
| | `torn_frames`, `unreadable_frames`, `output_repeats` | 0 | 0 | equal |
| 3 CPU | `cpu_seconds_per_second` | 0.011 | 0.011 | equal |
| 3 Battery | `battery_mw` | not measured | not measured | not measured |
| 4 Startup | `startup_ms` | 241.2 to 257.0 | 240.7 to 272.8 | equal |
| 5 Input | `input_ms` | 0.050 to 0.058 | 0.058 to 0.060 | equal |
| | `input_missed` | 0 | 0 | equal |
| 6 Stability | `stream_stalls`, `viewer_exits` | 0 | 0 | equal |
