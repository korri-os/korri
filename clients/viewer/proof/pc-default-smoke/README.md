# #391: one smoke stream with the PC default package

One stream with `korri-viewer` after the switch that makes nightjar-core,
the Rust EGL presenter and the Rust X11 window the PC defaults. It is a
smoke stream, not a measure: one run, no baseline.

**It streamed with all three Rust parts**, and the run was valid.

- Package: `/nix/store/04vlll7zbzrp1z6xd7jr3bnmidhgi1zl-korri-viewer-0.1.0`,
  built on aka from the tree of the commit "nightjar-core, the Rust EGL
  presenter and the Rust X11 window are the PC defaults". Its build ran 298
  lib tests with the three features, the Rust presenter's and window's
  among them.
- 2026-11-17 20:30 to 20:32 UTC, after three checks a minute apart found
  aka quiet, holding `/var/tmp/korri-turns.lock`. Other work took 0.8 % of
  all CPUs.

```sh
nix run .#stream-cadence -- view --build-store auto --sender local --sender-fixture \
  --viewer local --viewer-package /nix/store/04vlll7zbzrp1z6xd7jr3bnmidhgi1zl-korri-viewer-0.1.0 \
  --sender-sunshine /nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri \
  --viewer-output 1920x1200 --tone 1000 --runs 1 --out /var/tmp/step-e-core/sets/pc-default-smoke
```

`--viewer-output 1920x1200` gives the 1920x1080 picture bars, so the EGL
presenter draws it (#476). The viewer passed no `-networkcore`,
`-eglpresenter` or `-x11window`. Its log (`viewer.log`) names each part:

```text
picture: the Rust X11 window 0x800000, 1920x1080, on its own connection to :0
picture: the Rust EGL presenter shows VA-API surfaces
nightjar: at the stop: SessionStats { … }
nightjar: recovery requests sent: 0 IDR, 0 reference frame invalidations
```

| Field | Value |
| --- | --- |
| Picture | `x11_vaapi`, `decode=vaapi`; the EGL presenter did not give up |
| `latency_p50_ms`, `latency_p95_ms` | 13.67, 20.86 (the median of 6 windows; F3's set gave 13.69 to 13.82 and 20.87 to 21.03 for the Rust presenter at this output) |
| `cpu_seconds_per_second` | 0.017 |
| `startup_ms` | 160.7 |
| `input_ms`, `input_missed` | 0.056, 0 |
| `stream_stalls`, `viewer_exits` | 0, 0 |

Record: `results.json` and `viewer.log`, unedited.

**The first smoke stream showed the C core.** At 18:43 the same command
with the first build of the switch streamed with the C core: its log had
`Stopping input stream...` and `ENet wait interrupted`, and no Rust part's
line. That build had named its features `cargoBuildFeatures`, which
buildRustPackage overwrites with `buildFeatures`, so it built none of the
three. The switch now passes `buildFeatures` and `checkFeatures`, and an
evaluation assert in `default.nix` keeps them equal to the PC list.

## The landing stack's package, and `-networkcore c`

The package of the stack that lands the switch (with the review's licence
fix and `korri-viewer-c`):
`/nix/store/hfdna451li0vvski5ycfnmzma9rwda4b-korri-viewer-0.1.0`. Its build
ran 298 lib tests. Both streams ran under the lock but not after the quiet
gate: another session's game tests (`longshot`, about 520 % CPU) held aka's
load at 4.5 to 6 and Tctl at 86 to 96 °C for hours. Each stream started in
a pause below 82 °C, as stream-cadence's heat guard refuses a start at
85 °C (it refused the first try of each). `host-load.txt` has the load and
Tctl before and after. They prove function, not numbers.

| | `landing/` (2026-11-18 00:21) | `networkcore-c/` (01:09) |
| --- | --- | --- |
| Package | `korri-viewer` | the same binary, whose `bin/moonlight` appends `-networkcore c` (`/nix/store/4d1sv0qhs7qa9fyahqbj8valss6n3c1w-korri-viewer-networkcore-c`, built on aka for this stream only) |
| Valid | yes | yes |
| Network core in the log | nightjar-core (`nightjar: at the stop`) | the C core (`Stopping input stream...`, `ENet wait interrupted`; no `nightjar:` line) |
| Rust X11 window, Rust EGL presenter | both | both |
| Other work, share of all CPUs | 45.6 % | 71.5 % |
| Tctl during the run | 68.6 to 92.9 °C | 73.1 to 94.9 °C |
| `latency_p50_ms`, `latency_p95_ms` | 14.49, 21.69 | 16.67, 24.65 |
| `cpu_seconds_per_second` | 0.038 | 0.048 |
| `startup_ms` | 160.7 | 128.5 |
| `input_ms`, `input_missed` | 0.089, 0 | 0.082, 0 |
| `stream_stalls`, `viewer_exits` | 0, 0 | 0, 0 |
