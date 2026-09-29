# Sunshine KMS rotation cost probe (off-device only)

`rotation-probe.patch` is deliberately **not** in `approved-patches.nix` or
`package.nix`. It is an experiment, not a fix ready to release. The first signed
batch, `build-932cd7194921`, **must not be selected on the current Mini**. Its
inspected plugins replace the active root-owned receiver with a different root
setup and require the absent `korri-input-seat-receiver.service`. After testing
the compatible `build-211e26ed0ca1` batch, the Mini again selects the original
signed package `/nix/store/fq4ayc50jgf0b3bd85hpw41cg6kx83ig-korri-plugin`
with its `korri-sunshine-input-seat-receiver.service`. The signed rotation-on
trial package remains the previous rollback selection. Any replacement must
retain the original producer and permission boundary.

## Mini trial result and diagnostic change

On 2026-09-28, the compatible signed force-off package was selected with the
original as rollback. Moonlight returned `failed to start desktop error 503`.
Sunshine logged a KMS capture initialization failure, then no working encoder;
its startup saw a `0x0` expected mode against `1080x1240`. The original package
had shown that same transient startup mismatch before recovering, so this is
not proof that the probe caused the 503. The original signed selection was
restored and Moonlight opened, but the picture remained rotated.

The compatible signed rotation-on package then streamed. The owner still saw a
rotated picture. Its 300-frame capture logs repeatedly reported
`rotated=false`, so the rotation pass never ran. The logged unrotated capture
p50 was about 5.1 ms in the observed windows. This is **not** rotation cost:
there is no rotated timing sample, matched force-off stream, controlled moving
scene, delivered-FPS measure or cursor result. The original signed package was
restored again; its receiver, SSH, audio environment and H.264 startup were
verified active. The trial did not write an SD card or firmware.

Sway's live output query reported `DSI-1` transform `90`; the active DRM primary
plane reported `rotation=1` (`rotate-0`). These are observations of the
compositor and DRM plane, **not** the values Sunshine saw in its rotation guard.
This revision logs the Wayland-to-DRM output correlation and the guard's actual
Wayland transform, plane rotation, CRTC, plane, framebuffer dimensions and
offsets once per KMS RAM capture initialization. It does not relax the guard
or change the readback, rotation shader, cursor blending or other capture
routes. A newly signed exact package and separate Mini approval are required
before another device test.

The Mini also has an externally managed `90-audio-runtime.conf` in Sunshine's
host-owned systemd drop-in directory. The plugin host's stop path cannot remove
that directory while the extra file remains; the first update stopped Sunshine
and needed a scoped `/run` recovery from the saved original unit and policy.
For the subsequent trials, the audio file was parked only during the host
update/restore, then restored and applied by a service restart. Do not repeat
a plugin update without accounting for this conflict and verifying the audio
setting afterward.

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
  `WL_OUTPUT_TRANSFORM_90`. A visible hardware cursor follows the same
  source-to-destination mapping after readback. It visits only cursor pixels,
  not the whole frame; the existing blend already does not scale the cursor.
  A mismatched Wayland output mode cannot authorize rotation. The probe logs
  the matched Wayland output name, DRM CRTC/plane, raw size, output size and
  transform at initialization. The Mini's actual Wayland event, exact match,
  cursor appearance and observed orientation still need a physical check.
  Do not infer them from `swaymsg` output or panel dimensions alone.

At 1240 × 1080 × 4 bytes, the extra destination texture holds **5,356,800
bytes** (about 5.11 MiB). An uncached full-frame GPU read plus write at 60
FPS would move **642,816,000 bytes/s** (about 643 MB/s). This is calculated
traffic, **not measured bandwidth, latency, GPU occupancy, or power**. There is
no second CPU readback or CPU full-frame rotation in this candidate.

## Off-device gates

Run from the worktree on a build machine with the approved source available:

```sh
services/sunshine/rotation-probe-check.py \
  /nix/store/gybylg65i7xxapkabpwy1jgscbb44b0l-source \
  /nix/store/3gjgvw98yp52scnbwdxibdfw9abzk3k3-source
```

The check applies the probe after both the approved base patch set and the
approved RKMPP patch set. It compares the KMS GPU capture class before/after,
checks that the original unrotated readback stays, and checks the asymmetric
3 × 2 shader coordinate mapping. These are **static** checks, not a GPU or
zero-copy runtime trace. The `--fuzz=0` probe application prevents silent
context relocation in the two reviewed profiles.

Compile each instrumented profile off-device, without changing the approved
package or patch list:

```sh
nix build --impure --file services/sunshine/rotation-probe-build.nix \
  --out-link /tmp/sunshine-rotation-x86-experiment
nix build --impure --file services/sunshine/rotation-probe-build.nix \
  --argstr system aarch64-linux \
  --out-link /tmp/sunshine-rotation-arm-experiment
nix build --impure --file services/sunshine/rotation-probe-build.nix \
  --argstr system aarch64-linux --arg forceOff true \
  --out-link /tmp/sunshine-rotation-arm-force-off
nix build --impure --file services/sunshine/rotation-probe-plugin.nix \
  --out-link /tmp/sunshine-rotation-plugin-on
nix build --impure --file services/sunshine/rotation-probe-plugin.nix \
  --arg forceOff true --out-link /tmp/sunshine-rotation-plugin-off
services/sunshine/rotation-probe-artifact-check.py \
  /tmp/sunshine-rotation-plugin-on /tmp/sunshine-rotation-plugin-off \
  /nix/store/3gjgvw98yp52scnbwdxibdfw9abzk3k3-source
```

The ARM expression selects the `sunshine-korri-v4l2m2m` RKMPP profile from
Core `1a988da5`, the exact producer of the Mini's active signed plugin and
Sunshine binary. `rotation-probe-baseline.nix` pins that producer. It must
not silently follow the current Core plugin, which changed its units, receiver,
input patch and requested authority. The derivation has an experimental name
and version. Its provenance identifies
it as an experiment and records the probe hash, parent profile and force-off
mode. It is **not** an approved
Sunshine build or a plugin closure. The existing plugin's approval binds its
exact package closure. `rotation-probe-plugin.nix` builds the matching
experimental plugin output for both ARM variants off-device; it does not
publish or sign them. The official plugin publisher requires the same named
outputs on x86_64 and aarch64, so it also builds experimental x86_64 plugin
outputs. The Mini trial uses **only** the ARM outputs. Do not substitute a standalone executable under the
existing plugin unit, even after signing the binary. The plugin host needs a
publisher-bound signature for the **new full closure** and an inspected exact
approval. The on/off batch contains two outputs with the same `@korri:sunshine`
identity, so lookup by release and plugin ID rejects it as ambiguous. Inspect
each exact ARM output path from the batch separately; verify its signed
manifest and probe mode before approving an update. The artifact check reads both actual plugin
manifests, the retained receiver and setup paths, package provenance, patch
hash and AArch64 ELF headers. It does not verify a signature,
Mini GL context, or physical timing. The derivation defines
`SUNSHINE_CAPTURE_ROTATION_PROBE` for timing logs.
Each 300 successful KMS RAM frames log p50/p95/p99/max for capture (without FPS
pacing) and for readback plus the conditional draw. Each 300 encoded frames log
p50/p95/max for `session->convert` and the `encode()` call. `encode()` call time
can include packet handling; it does not prove hardware encode completion.
Readback wall time can include an implicit GPU wait and does not isolate GPU
execution. The counter labelled `budget-overruns` compares one stage with the
negotiated frame period; it is **not** a count of missed delivery deadlines.

## Trial gates before any device change

1. Review the patch and guessed monitor correlation. Confirm visible cursor
   rendering, GL output, 90-degree direction, and output dimensions on the
   actual Mini; static pixel mapping alone is not physical proof.
2. Obtain separate approval for a newly admitted exact signed plugin closure
   and a reversible device trial, not an override of the old approved unit. Recheck the
   exact device, active game/stream, pairing, signature checks, plugin owner,
   runtime overrides, old receiver unit and service policy, and rollback.
   Refuse a candidate with changed native authority or a missing host unit. Record the original signed selection and
   approval before each update. Build off-device; never compile on the Mini.
   Leave the current manual trial alone until its owner is ready. Test only
   one variant at a time: stop the stream, restore the original selection
   with `korri-plugin restore`, and verify its receipt and service before
   installing the other variant. The host retains only the current and one
   previous selection, so installing off and on back-to-back would discard
   the direct rollback to the original selection.
3. Compare `FORCE_OFF` with rotation on the **same** moving glmark2 scene,
   resolution, encoder, FPS, cursor state, and warm-up. Record 300-frame timing
   distributions, delivered FPS and actual late/dropped frames, Sunshine CPU
   time, GPU evidence, thermals, and power separately. The Mini has no verified
   GPU-busy counter, and plugged-in battery readings do not establish a power
   delta. The prior moving-scene baseline was 61 app FPS, 16.654 ms **app**
   frame time, and 148.32% of one CPU core for Sunshine, not capture stage
   timings. The IDR warning is a separate acceptance issue.

**Cost of this route:** an extra GPU pass and texture on rotated KMS RAM
captures, plus review and measured trial time. Cursor blending still visits
only visible cursor pixels. Physical cursor behavior, monitor-correlation
uncertainty, and IDR recovery remain open. Do not ship it on that basis.
