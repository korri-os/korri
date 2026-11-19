---
status: accepted
---

# The viewer moves to Rust in measured stages

**Owner decisions, 2026-11-11.** Issue #391 replaces Korri's viewers with one
viewer that Korri owns. The end state is a viewer with no C or Java that Korri
writes, patches or forks. Unpatched system libraries can stay. A stretch goal
goes further: no C library in the viewer, only the kernel and Android's own
API.

Korri reaches that state in stages. Measured performance decides each stage.
The language does not.

## Decision

- Stage 1 puts the session part in Rust, for Linux and Android. The session
  part starts the stream, reports its lifecycle, sends input and holds the
  stream controls. Stage 1 keeps today's C network library
  (`moonlight-common-c`) and today's picture code. It also makes the viewer
  report its frame timing.
- After stage 1, each stage replaces one part. A C or Java part stays until
  its Rust replacement measures equal or better.
- Where a part cannot be Rust yet, Korri prefers C to Java.
- Six numbers define performance: the time from packet arrival to the frame
  on the display; frames shown late, shown twice or dropped; CPU and battery
  use on the viewing device; the time from Play to the first frame; the time
  from a button press to the packet that leaves the device; stream drops and
  reconnects.
- Agents run all validation without the owner. The owner validates a stage
  only after that proof exists.

## Why

Rust does not make a stream faster by itself. A viewer's speed comes from its
picture path, and that code works today. A stage that changes one part gives
each change in the numbers one cause.

Korri cannot measure the viewer's picture path on this date.
`nix/stream-cadence` replaces the renderer, so it measures the sender and the
network. The frame timing from stage 1 is the measure for every later stage.

## Considered options

- **Everything in Rust at once.** Nothing would be comparable with today, and
  every device would need a hands-on check before any of it ships.
- **Rust wins when it conflicts with performance.** Streams could be worse
  than today for an unknown time.

## Amendment

**Owner decision, 2026-11-18.** The Rust audio output (#391 step F, slice
F1) became the PC viewer's default ahead of a quiet measurement. The owner
deferred that measurement to the end of #391, with the other final
measurements: `korri-viewer-c` against `korri-viewer-rust-audio-output`,
with `--tone 1000`. Until then F1 has not measured equal or better as the
decision above asks. Its four sets on aka were all disturbed by other work
(`clients/viewer/proof/rust-audio-output/`):

- the first set, 2026-11-17 (`turns-2026-11-17/`): other work above 5 % of
  all CPUs in four of the five candidate runs; equal on every number;
- the three attempts of the rerun that was to make the verdict firm,
  2026-11-17 (`rerun-2026-11-17/`): other work above 5 % in 6, 3 and 8 of
  the 10 runs; the first two equal on every number, the third one frame
  later at startup.

`-audiooutput c` keeps the carried C.
