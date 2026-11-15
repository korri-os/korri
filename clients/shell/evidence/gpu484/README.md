# Linux shell GPU cut (#484)

2026-11-15. FemtoVG remains the Linux renderer chosen in ADR 0068.

## Pro comparison

RG35XX Pro, Mali-G31 / Panfrost, 640×480 at 60 Hz. The same installed
181-game catalog and surface package closure were used for both runs:
`/nix/store/pwv1zkrji6iq17v574prgm6bzmyygr52-korri-shell-surfaces`.
Velvet uses full motion. Zone uses its existing retro look on GAMES, not
its attract screen. No governor settings changed (`schedutil` CPU,
`simple_ondemand` GPU).

Each run restarts the service with just the measured surface, warms it for
12 seconds, then takes 31 `/proc` samples. Navigation enters Velvet's row
with Down, then alternates Right and Left with 400 ms rests. Zone receives
Up to keep its GAMES page awake. `measure-pro.sh` records the exact command.
The temporary override and input helper are removed on exit.

CPU is percent of one core, from process user+system ticks (100 Hz) over
actual `/proc/uptime` deltas. CPU p95 is nearest rank over 30 intervals.
PSS/RSS are means over 31 samples. Frame statistics below count **redraw
requests**, not completed or displayed frames; the old Zone renderer can
request more than the display's 60 Hz. Interval percentiles are separate
five-second windows, not a combined percentile. The first logged window
overlaps warmup and is excluded from the pacing table.

| Surface / state | CPU mean / p95, software → GPU | PSS / RSS MiB, software → GPU |
|---|---|---|
| Velvet idle | 157.3 / 204.3% → 32.8 / 55.9% | 53.8 / 57.8 → 106.0 / 111.8 |
| Velvet navigation | 262.8 / 277.5% → 63.0 / 74.5% | 57.1 / 61.1 → 111.5 / 117.3 |
| Zone animated GAMES | 15.7 / 22.6% → 25.7 / 28.2% | 77.3 / 81.2 → 122.5 / 128.3 |

| Surface / state | Redraws/s range, software → GPU | Interval p50 range, ms, software → GPU | Interval p95 range, ms, software → GPU |
|---|---|---|---|
| Velvet idle | 16.5–20.0 → 35.2–35.4 | 46.4–48.8 → 33.1–33.3 | 81.8–94.9 → 34.1–34.6 |
| Velvet navigation | 8.6–8.9 → 30.0–30.5 | 111.1–115.5 → 33.4–33.5 | 133.3–147.7 → 41.1–44.1 |
| Zone animated GAMES | 75.2–76.8 → 60.0 | 16.1–16.2 → 16.7 | 16.9–17.0 → 17.5–17.8 |

The software executable is 27,465,896 bytes; GPU is 27,334,888 bytes.
Their package closures are 120,055,304 and 125,052,712 bytes respectively.
These closure sizes exclude the system graphics driver, which is already
part of the device generation for its compositor.

These are not the disturbed measurements in the previous handoff. An
initial attempt here with `wtype` was also discarded: it constructs a
logical keymap with arbitrary physical key codes, whereas the shell reads
physical keys. Those attempts did not navigate. The committed baseline
uses the temporary `ydotoold` helper and real physical key codes instead.

The software binary was
`/nix/store/cdvav54myikfdcz6sq309wxc2js8lx5n-korri-shell-0.0.0/bin/korri-shell`.
The pre-landing GPU binary was
`/nix/store/f5aqmkwx4wmlqs88f0sd3g590115j40b-korri-shell-0.0.0/bin/korri-shell`.
The latter has the clean-cut code before rustfmt and documentation changes.
Both are Nix `dist` builds. The new binary was delivered with a signature
from the existing trusted `korri-cache-zao-1` key; no signature checks were
disabled. Build commands ran through `simon@fuji`, with rooted outputs and
at most three Nix jobs; its configured remote builder also ran work on yari.

### GPU work and isolation

The GPU shell opens only the bound render node. Its private `/dev/dri`
contains `renderD128`, not primary nodes; `/dev/input` remains hidden.
`fdinfo` identifies Panfrost and one DRM client shared by three file
descriptors. Mesa `libgallium-26.2.3.so` is mapped. This is not llvmpipe.

A second run enabled Panfrost's existing `device/profiling` switch to read
per-engine busy nanoseconds from fdinfo. It restored the switch to `0`.
Duplicate descriptors of the same DRM client were counted once.

| Surface / state | Fragment busy mean / p95 | Vertex-tiler busy mean / p95 |
|---|---|---|
| Velvet idle | 94.0 / 95.1% | 1.2 / 1.2% |
| Velvet navigation | 90.8 / 93.9% | 3.0 / 3.1% |
| Zone GAMES | 31.4 / 32.4% | 1.0 / 1.1% |

Velvet remains GPU-bound below 60 Hz. This cut improves its throughput and
CPU use; it does not establish that FemtoVG is the fastest renderer on Mali.
Zone's software renderer was already cheap because of its partial redraw
patches. Its CPU cost increases on the GPU. Memory increases by 45–54 MiB
PSS. No software fallback or per-device renderer switch was kept to mask
these costs.

### Pixels

The before and after PNGs use the real compositor capture, not headless
copies. The hero and GAMES screenshots show the same games. Navigation
screenshots are different animation/focus instants and are not pixel diffs.
The clock, relative time and battery values also changed between runs.

Verified by reading both sets of screenshots:

- Velvet's rounded poster and button edges are smoother on the GPU.
- The purple no-art posters gain visible centre holes and other small
  drawing details that were absent in the software capture. Small shelf
  icons and caption edges also rasterise differently.
- The GPU stage has visible fine dither and slightly different gradient
  blending. It is not a layout change or a new blur design.
- Zone's diagonal cartridge artwork is clean, broad colour bands on the
  GPU. The software capture has thin striped/moire-like regions instead.
  Small pixel-font edges differ too.
- The same controls remain visible. The eye still lands on Velvet's poster
  and large title, and on Zone's yellow GAMES tab. No surface styling or
  layout was changed in this cut.

## Linux device / GLES inventory

This table separates observed driver bindings from configuration. It does
not claim runtime acceptance on devices that were not available.

| Device | GLES driver / kernel | Evidence and limit |
|---|---|---|
| RG35XX Pro | Mesa Panfrost / `panfrost`, Mali-G31 | Live render fd, engine counters and screenshots in this proof. |
| RG35XX SP | Mesa Panfrost / `panfrost`, Mali-G31 | Same H700 producer as Pro: `nix/devices/h700/board.nix` and its README. No live SP check. |
| RG353M (haku) | Mesa Panfrost / `panfrost`, Mali-G52 | Existing live binding recorded in #484; `rg353m/gpu.nix` and `sd-image.nix`. SSH timed out in this run. No changes or prototype. |
| RG DS | Mesa Panfrost / `panfrost`, Mali-G52 | Running-kernel facts in `rgds/hardware.nix` and README; GLES2 configured there. No live check in this run. |
| R36T Max | Mesa Panfrost / `panfrost` (candidate) | `r36tmax/hardware.nix` explicitly loads Panfrost. Its saved survey had no driver bound; the render link is not runtime-proven. No new driver added. |
| Retroid Pocket Mini V2 | Mesa Freedreno / `msm`, Adreno 650 | `rpminiv2/hardware.nix`, product README and firmware inventory. No live check in this run. |
| Odin 2 Portal | Mesa Freedreno / `msm`, Adreno 740 | `odin2portal/sd-image.nix` supplies Mesa and GPU firmware; hardware module selects GLES2. No live check in this run. |
| ibuki | Mesa Iris / `i915`, Intel Iris Plus 655 | Live render-node binding and installed `iris_dri.so`. Its existing `50-gpu-test.conf` belongs to earlier work; not modified. #482 owns its earlier numbers. |
| zao | Mesa Iris / `i915`; NVIDIA EGL / `nvidia` | Live driver bindings and both EGL vendor files. No installed `korri-shell.service`. #482's 4K proof is earlier evidence, not re-measured here. The selected NVIDIA compositor path and proprietary device-node needs are not verified by this render-node-only proof. |
| aka | Mesa RadeonSI / `amdgpu` | Live bindings and Mesa EGL vendor file; no installed `korri-shell.service`. Not a shell deployment in this run. |

Fuji is a build machine, not a shell target. No GLES driver was added for
any device. A missing configured render node fails service startup rather
than falling back to software. R36T Max still needs device acceptance.

The LG B7 webOS SDL target is outside this Linux winit cut, as confirmed by
the supervisor. It still rasterises with Slint's software renderer into the
SDL window. A later GLES path needs a usable webOS EGL/GLES context, Slint
renderer integration with that SDL window, packaged dispatch dependencies,
and measured pixel/performance acceptance on the TV. This cut keeps its
existing software build compiling.
