# THROWAWAY shell proof — 2026-10-31

**Ready for a newly approved Zone/Pico device run.** The final aarch64 archive
was unpacked into a fresh workstation directory and its unchanged `run.sh`
rendered both real packages under QEMU/PRoot and Xvfb. No device launch has been
made since the two separately approved failed test-screen attempts below.

## Exact next device command (approval required)

```sh
clients/surface-shell-spike/device/drive.sh /tmp/surface-shell-sp.tar.gz --surfaces zone,pico --hold-seconds 120 --results /tmp/surface-shell-real-results
```

It measures Zone for 24 seconds, switches and measures Pico for 24 seconds,
returns to Zone and leaves input live for 120 seconds. Then it restores the
kiosk, verifies the kiosk and Chromium PIDs for 60 seconds, collects evidence,
and deletes the spike's `/tmp` files, including the two earlier failed runs.
The new measurement/hold/cleanup path passed ShellCheck, not a device run.

## Native x86_64 Xvfb proof

`proof-surfaces.sh` loads the unmodified Zone and Pico packages. Screenshots in
`surfaces/` show Zone's Hollow Knight, Right to Tetris, Left back to Hollow
Knight, F2 to Pico and F2 back to Zone. Nested cover-image structs rendered.
Twenty further switches completed. The shell ignores X11's synthetic held-key
presses on focus, which otherwise replayed F2 and caused a second switch.

| Initial load | Compile ms | Logic-load ms | First completed frame ms |
| --- | ---: | ---: | ---: |
| Zone | 64.783 | 6.989 | 153.971 |
| Pico | 19.798 | 6.914 | 46.267 |
| Zone after F2 back | 63.489 | 7.276 | 103.249 |

Zone alone reported **61.985 and 62.371 redraw frames/s**, in two five-second
intervals. These are Xvfb redraws, not a claim about a physical panel's refresh.
Full timing/FPS lines: `surfaces/stats.log`.

**Memory limit, not a flat-RSS claim:** `surfaces/rss.csv` records **57,520 KiB
before and 59,600 KiB after 20 switches**: +2,080 KiB, about **104 KiB/switch**
on x86_64. Slint's software font loader reads/registers imported fonts on each
component creation; Fontique appends these faces. This is consistent with the
growth, not a full allocation proof. If that observed rate continued, 100
switches/day would cost about **10.2 MiB/day**. That is an extrapolation, not a
long-run or SP measurement. Per the supervisor, this spike does not manage
Slint's font lifecycle or clear global font collections.

Zone's initial 0-to-1 band snap remains its own declared animation. It is not
suppressed by the shell. No Zone or Pico files were changed by the shell worker.

`proof-pico.sh` also checks absent art (4 of 12 fixture carts) and a temporary
empty-shelf fixture whose `stage.cart` is null. Both render without shell errors.
`xvfb/` holds the minimal test screens, arrow/F2/reset checks, retaining the
active surface after a compile failure, and the 1280x720 proof.

## Final aarch64 build and exact archive

Built on **fuji**, including **10 passing aarch64 unit tests**:
`/nix/store/vjn4k07p1mk5xyh2dg1irnpfn534lv25-surface-shell-spike-0.0.0`.

```sh
nix build --impure --no-link --print-out-paths --expr \
  '(import <nixpkgs> { system = "aarch64-linux"; }).callPackage ./clients/surface-shell-spike/package.nix {}'
SHELL_WTYPE_BUNDLE=/tmp/shell-wtype clients/surface-shell-spike/device/bundle.sh \
  /nix/store/vjn4k07p1mk5xyh2dg1irnpfn534lv25-surface-shell-spike-0.0.0 \
  surface-shell-spike /tmp/surface-shell-sp
```

`/tmp/surface-shell-sp.tar.gz` contains the executable, its own loader, libraries,
XKB, fixture, Zone/Pico/test packages and fonts, and a prebuilt wtype companion.
SHA-256: `7f0c80539d8a455ae853a7e846066f323ba883af37f56f8ab32311da1ae2eaa6`.

`proof-bundle.sh` extracted this exact archive freshly and ran its unchanged
`run.sh` from caller cwd `/`, with `SHELL_ROOT`, `SHELL_SIZE` and
`LD_LIBRARY_PATH` unset. It used fullscreen, software rendering, stats and
`zone,pico`. PRoot inserted QEMU only for foreign ELF execution. The display
edge was X11/Xvfb, not the SP Wayland socket; its private runtime directory was
owned by the workstation user. `bundle/` contains screenshots, hashes and logs.
Emulated timings are **not** SP timings. The generated launcher now sets
`SHELL_ROOT` to its own `bin/`, so explicit ld-linux execution cannot misdirect
resource lookup to the loader's directory. Both packages and F2 back rendered.

## Device attempts and recovery

| Separately approved test attempt | Result | Kiosk afterwards |
| --- | --- | --- |
| First | Aborted in preflight: inactive game slice absent from cgroupfs. No kiosk change or shell launch. Fixed locally. | MainPID 16682 / Chromium 16689; stable 60 s; NRestarts=0. |
| Retry | Shell exited before first frame: current_exe identified explicit loader, so fixture path was wrong. Launcher fixed locally and exact archive tested above. | Restored by trap. MainPID 17270 / Chromium 17277; stable 60 s; NRestarts=0. |

Evidence: `device-preflight/`, `device-loader-failure/`. The first attempt's
specified `/tmp` files were removed before the retry. Retry files remain until
the next approved driver's post-copy cleanup. No SP switch timings or shell
screenshots exist from these failed attempts. No real-package SP run is claimed.

## Checks and remaining limits

- `cargo fmt --check`, `cargo clippy --release --all-targets -- -D warnings`,
  `cargo test --release` (10), `cargo build --release`: pass.
- Nix aarch64 build and its 10 tests: pass, fuji.
- ShellCheck on all proof/device scripts; nixfmt package check: pass.
- Real native Xvfb test/Pico/Zone proofs and exact aarch64 archive proof: pass.
- Copied PSS/CPU sampler exercised against an owned workstation scope: pass
  (one sample, 1.01 s, 733 KiB PSS, 0% CPU for sleep).
- Pre-commit secret scan: pass for prior implementation/proof commits.

Compilation pauses input/animation on the UI thread while the old frame stays
visible. Root remains a spike-only input exception. Detailed numeric metadata
uses a pinned Slint-internal API. Gamepad actions and a successful Wayland
surface switch on SP still need the newly approved run. No signatures,
permissions, groups, persistent units or device generations were changed.
