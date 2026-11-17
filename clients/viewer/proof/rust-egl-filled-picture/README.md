# #391 step F, slice F5: the filled picture through the Rust EGL presenter, in turns

Verdict: **worse on numbers 1 and 3. Rejected as the default.** The filled
picture through EGL stays an option; `vaPutSurface` stays the default.

| Number | A | B |
| --- | --- | --- |
| 1 `latency_p50_ms` | 11.945 to 12.055 | 13.669 to 13.776 (+1.7 ms) |
| 1 `latency_p95_ms` | 19.062 to 19.217 | 20.814 to 20.991 (+1.8 ms) |
| 3 `cpu_seconds_per_second` | 0.011 | 0.014 to 0.015 |

`stream-cadence view` on aka, 2026-11-17 13:00 to 13:16 UTC: five runs of
each side in the order A B A B, at the defaults, Korri's patched Sunshine
(`--sender-sunshine
/nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri`),
`--build-store auto`, no `--net`. The set started only after three checks a
minute apart found aka quiet (1-minute load below 1.5, k10temp Tctl below
78 °C, no Nix build, no game and no other session's proof), and held
`/var/tmp/korri-turns.lock` on aka while it ran.

- A: `/nix/store/wi0qpfa4fw85sgx0g0zvb0mja7hbf3i2-korri-viewer-rust-egl-presenter-0.1.0`
  (`vaPutSurface` for a filled picture).
- B: `/nix/store/y22h6fsr6pajjb8siw97xa9i1235pz2b-korri-viewer-rust-egl-filled-picture-0.1.0`
  (the Rust EGL presenter for a filled picture too).
- Both built by the step F worker (roots in
  `/nix/var/nix/gcroots/korri-391-step-f/` on aka). Both `x11_vaapi`,
  `decode=vaapi`.
- Every B run's log says "picture: the Rust EGL presenter shows VA-API
  surfaces", and none says "EGL cannot present here", so each B run showed
  its picture through EGL.

All 10 runs valid; other work on aka 0.8 to 2.2 % of all CPUs.
Record: `turns-2026-11-17/` (`baseline.json`, `candidate.json`,
`verdict.txt` unedited, each run's `viewer.log`).

| ADR 0062 number | Field | A | B | Verdict |
| --- | --- | --- | --- | --- |
| 1 Packet arrival to display | `packet_to_display_ms` | not measured | not measured | not measured |
| 1 (the gate) Sender presentation to display | `latency_p50_ms` | 11.945 to 12.055 | 13.669 to 13.776 | **worse** |
| | `latency_p95_ms` | 19.062 to 19.217 | 20.814 to 20.991 | **worse** |
| 2 Late, repeated, dropped | `dropped_frames` | 0 | 0 to 2 | equal |
| | `repeated_frames`, `late_frames` | 233 to 236 | 233 to 236 | equal |
| | `torn_frames`, `unreadable_frames`, `output_repeats` | 0 | 0 | equal |
| 3 CPU | `cpu_seconds_per_second` | 0.011 | 0.014 to 0.015 | **worse** |
| 3 Battery | `battery_mw` | not measured | not measured | not measured |
| 4 Startup | `startup_ms` | 272.8 to 289.3 | 272.9 to 289.4 | equal |
| 5 Input | `input_ms` | 0.048 to 0.057 | 0.051 to 0.056 | equal |
| | `input_missed` | 0 | 0 | equal |
| 6 Stability | `stream_stalls`, `viewer_exits` | 0 | 0 | equal |
