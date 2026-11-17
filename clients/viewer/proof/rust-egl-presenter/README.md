# #391 step F, slice F3: the Rust EGL presenter, in turns

Verdict: **equal on every number**, the Rust port of the carried C EGL
presenter (`src/egl_presenter/`) against `picture/video/vaapi_egl.c`.

`stream-cadence view` on aka, 2026-11-17 13:21 to 13:36 UTC: five runs of
each side in the order A B A B, at the defaults with `--viewer-output
1920x1200` (a picture with bars, which goes through EGL, #476), Korri's
patched Sunshine (`--sender-sunshine
/nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri`),
`--build-store auto`, no `--net`. The set started only after three checks a
minute apart found aka quiet (1-minute load below 1.5, k10temp Tctl below
78 °C, no Nix build, no game and no other session's proof), and held
`/var/tmp/korri-turns.lock` on aka while it ran.

- A: `/nix/store/l85f52rx3cp6zdap6fbx5qf3wvb41sgz-korri-viewer-0.1.0`
  (the carried C presenter).
- B: `/nix/store/wi0qpfa4fw85sgx0g0zvb0mja7hbf3i2-korri-viewer-rust-egl-presenter-0.1.0`.
- Both built by the step F worker (roots in
  `/nix/var/nix/gcroots/korri-391-step-f/` on aka). Both `x11_vaapi`,
  `decode=vaapi`.
- Every B run's log says "picture: the Rust EGL presenter shows VA-API
  surfaces" once, and none says "EGL cannot present here".

All 10 runs valid; other work on aka 0.9 to 5.2 % of all CPUs.
Record: `turns-2026-11-17/` (`baseline.json`, `candidate.json`,
`verdict.txt` unedited, each run's `viewer.log`).

| ADR 0062 number | Field | A | B | Verdict |
| --- | --- | --- | --- | --- |
| 1 Packet arrival to display | `packet_to_display_ms` | not measured | not measured | not measured |
| 1 (the gate) Sender presentation to display | `latency_p50_ms` | 13.598 to 13.791 | 13.688 to 13.822 | equal |
| | `latency_p95_ms` | 20.863 to 20.989 | 20.868 to 21.032 | equal |
| 2 Late, repeated, dropped | `dropped_frames` | 0 to 2 | 0 to 4 | equal |
| | `repeated_frames` | 234 to 236 | 233 to 238 | equal |
| | `late_frames` | 234 to 237 | 233 to 238 | equal |
| | `torn_frames`, `unreadable_frames`, `output_repeats` | 0 | 0 | equal |
| 3 CPU | `cpu_seconds_per_second` | 0.014 | 0.014 to 0.019 | equal |
| 3 Battery | `battery_mw` | not measured | not measured | not measured |
| 4 Startup | `startup_ms` | 240.7 to 273.1 | 256.6 to 304.9 | equal |
| 5 Input | `input_ms` | 0.050 to 0.056 | 0.054 to 0.063 | equal |
| | `input_missed` | 0 | 0 | equal |
| 6 Stability | `stream_stalls`, `viewer_exits` | 0 | 0 | equal |
