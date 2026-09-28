# Sunshine KMS rotation cost probe (off-device only)

`rotation-probe.patch` is deliberately **not** in `approved-patches.nix` or
`package.nix`. It is not installed on any device. The patch is an experiment,
not a fix ready to release.

## Scope and provenance

- Sunshine's `wl_output.geometry` callback supplies the Wayland output transform.
  The existing KMS-to-Wayland output match associates it with a DRM CRTC. That
  match is explicitly described as guesswork in `kmsgrab.cpp`.
- Only the KMS **RAM** capture object tries rotation. It selects the GPU pass
  when the matched Wayland output reports `WL_OUTPUT_TRANSFORM_90`, the DRM
  plane reports `DRM_MODE_ROTATE_0`, and the source rectangle is the entire,
  uncropped framebuffer. The output dimensions are swapped. The source texture
  is drawn into one persistent destination texture; the existing single
  GPU-to-CPU readback takes the destination instead of the source.
- A KMS RAM capture with no transform executes the existing readback call.
  The KMS VRAM, Wayland, CUDA, VAAPI, and RKMPP capture classes are not changed.
  The transform-0 path gains no draw, texture allocation, readback, wait, or
  color conversion. The uninstrumented build has no per-frame clocks or logs.
  `SUNSHINE_CAPTURE_ROTATION_FORCE_OFF` keeps the unrotated path available in
  an otherwise identical instrumented build for a controlled comparison.
- The shader applies the 90-degree counter-clockwise transform specified by
  `WL_OUTPUT_TRANSFORM_90`. The Mini's actual Wayland event, exact monitor match,
  and observed orientation still need a physical check. Do not infer them from
  `swaymsg` output or the panel's dimensions alone.

At 1240 × 1080 × 4 bytes, the extra destination texture holds **5,356,800
bytes** (about 5.11 MiB). An uncached full-frame GPU read plus write at 60
FPS would move **642,816,000 bytes/s** (about 643 MB/s). This is calculated
traffic, **not measured bandwidth, latency, GPU occupancy, or power**. There is
no second CPU readback or CPU full-frame rotation in this candidate.

## Off-device gates

Run from the worktree on a build machine with the approved source available:

```sh
services/sunshine/rotation-probe-check.py \
  /nix/store/gybylg65i7xxapkabpwy1jgscbb44b0l-source
```

The check applies the probe after both the approved base patch set and the
approved RKMPP patch set. It compares the KMS GPU capture class before/after,
checks that the original unrotated readback stays, and checks the asymmetric
3 × 2 shader coordinate mapping. These are **static** checks, not a GPU or
zero-copy runtime trace. The `--fuzz=0` probe application prevents silent
context relocation in the two reviewed profiles.

Compile the instrumented x86_64 probe off-device, without changing the
approved package or patch list:

```sh
nix build --impure --file services/sunshine/rotation-probe-build.nix \
  --out-link /tmp/sunshine-rotation-x86-build
```

This proves C++ compilation for the x86_64 profile. It does **not** compile
the aarch64/RKMPP profile or verify a Mini GL context. The resulting package
has unchanged release provenance and must **never** be deployed as an approved
Sunshine build. The experimental derivation defines
`SUNSHINE_CAPTURE_ROTATION_PROBE` for timing logs.
Each 300 successful KMS RAM frames log p50/p95/p99/max for capture (without FPS
pacing) and for readback plus the conditional draw. Each 300 encoded frames log
p50/p95/max for `session->convert` and the `encode()` call. `encode()` call time
can include packet handling; it does not prove hardware encode completion.
Readback wall time can include an implicit GPU wait and does not isolate GPU
execution. The counter labelled `budget-overruns` compares one stage with the
negotiated frame period; it is **not** a count of missed delivery deadlines.

## Trial gates before any device change

1. Review the patch and monitor correlation. Add a correct transformed cursor
   path: the prototype deliberately returns a capture error when a visible
   hardware cursor is requested on a rotated stream. Confirm GL output,
   90-degree direction, and output dimensions on the actual Mini.
2. Obtain separate approval for a signed, reversible deployment. Recheck the
   exact device, active game/stream, pairing, signature checks, plugin owner,
   runtime overrides, and rollback. Build off-device; never compile on the
   Mini. Leave the current manual trial alone until its owner is ready.
3. Compare `FORCE_OFF` with rotation on the **same** moving glmark2 scene,
   resolution, encoder, FPS, cursor state, and warm-up. Record 300-frame timing
   distributions, delivered FPS and actual late/dropped frames, Sunshine CPU
   time, GPU evidence, thermals, and power separately. The Mini has no verified
   GPU-busy counter, and plugged-in battery readings do not establish a power
   delta. The prior moving-scene baseline was 61 app FPS, 16.654 ms **app**
   frame time, and 148.32% of one CPU core for Sunshine, not capture stage
   timings. The IDR warning is a separate acceptance issue.

**Cost of this route:** an extra GPU pass and texture on rotated KMS RAM
captures, plus review and measured trial time. It does not fix cursor handling,
monitor-correlation uncertainty, or IDR recovery. Do not ship it on that basis.
