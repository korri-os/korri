# Checks for #484

- Both ARM Nix shell package builds: 207 tests passed across 36 test targets,
  including the retired renderer-flag test and the headless pixel tests.
- `nix run .#shell-check`: formatter, desktop clippy, webOS clippy and input
  trace checks passed. The first desktop test run stopped at Pico's CPU-time
  budget test. The exact same test executable later passed unchanged in
  0.88 s. A pinned-toolchain software baseline at `5ff83d91f` also passed.
  No test threshold was changed. Host load/frequency is a possible cause,
  not a proved cause.
- The remaining desktop test executables completed separately. Functional
  and pixel tests passed. Velvet's CPU-time budget test failed while its
  focus/render test ran, then the unchanged executable passed in isolation
  in 25.54 s. Zone's CPU-time budget test still failed in isolation. A
  CPU-0-pinned comparison made both the software baseline and this cut fail
  that same Zone test (exit 101). The baseline had passed without affinity.
  These desktop timing checks are not a clean pass; their core/frequency
  sensitivity remains a follow-up. No limits or timing tests were weakened.
- Nix shell module evaluation passed, including rejecting `/dev/input/event0`
  and `/dev/dri/card0` as the selected render node. Odin module evaluation
  passed. A product-wide evaluation was attempted but timed out; it is not
  counted as a pass.
- Nix formatting and `git diff --check` passed.
- Dependency graph: main enables winit `renderer-software`; this cut does
  not. `shell-check` now guards that boundary.
- Real CLI: obsolete `--renderer cpu` exits 2 with usage. Headless Zone
  `bench --frames 5 --games 12` succeeds, with zero frames over 16.7 ms.
- Real GPU path: the unprivileged shell ran all three measured Pro scenarios
  inside the render-node-only private device namespace. Panfrost fdinfo and
  Mesa mappings prove hardware use. PNGs came from compositor capture.

The first `shell-check` invocation also found unformatted import edits. They
were formatted before the substantive check above. Vendored winit reports
two unused software-pacing helper warnings now that its software adapter is
not enabled. Nix's sandbox also reports unwritable fontconfig cache warnings;
these did not fail the ARM pixel or font tests.

The final system built successfully on fuji. Rebuilding from a clean
archive of landed `aa5cef17c` reused that exact system output. It is deployed
as Pro generation 55. The shipped binary and its private device namespace
were checked again, with zero failed units and no leftover drop-ins.
