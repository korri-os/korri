---
id: 01M2MAXVBEKYZT9RJB28FR264A
slug: reach-60-fps-ps1-emulation-with-a-concurrent-720x720-stream-
title: Reach 60 fps PS1 emulation with a concurrent 720x720 stream on R36T Max
origin: parked
status: To Do
priority: medium
labels:
  - r36tmax
  - sunshine
  - rkmpp
  - performance
  - emulation
created: 2026-09-16
source: se-debug
context:
  cwd: korri/.worktree/r36tmax-sunshine-mpp
  branch: feat/r36tmax-sunshine-mpp
  repo: korri
---

# Reach 60 fps PS1 emulation with a concurrent 720x720 stream on R36T Max

## Why it matters

PCSX-ReARMed sustains 58 fps standalone on the R36T Max but drops to 35 fps once Sunshine captures and encodes concurrently, making streamed PS1 play unacceptable to the user. Measurement shows this is not CPU starvation, clock throttling, or scheduler preemption: the emulation thread holds 97% of a dedicated core at 1200 MHz (vs 98% at 1296 MHz idle, an 8% deficit) yet loses 40% of its frame rate. The same cycles do less work, which points at the shared L2 cache and single-channel memory on RK3326 being saturated by the capture path's ~450 MB/s of traffic. The only remaining software lever is cutting that traffic: Sway composites into its output buffer and screencopy then copies it again into the encoder's linear buffer, roughly 8 MB per frame. Capturing the scanout framebuffer directly would remove ~4 MB/frame. This blocks PS1-class content as a streamed use case and caps what the device can advertise.

## Acceptance Criteria

- [ ] PCSX-ReARMed sustains >=58 fps in actual gameplay (not menu or FMV) with an active 720x720 Sunshine session
- [ ] Emulator frame rate measured from RetroArch's on-screen counter via grim, not from journal lines, on an identical scene with streaming on and off
- [ ] Encoding remains entirely on VEPU2 hardware with zero IOMMU faults
- [ ] Per-frame capture memory traffic is quantified before and after any change

## Related

- `services/sunshine/patches/0027-capture-wayland-frames-for-rkmpp.patch`
- `services/sunshine/patches/0031-pipeline-wayland-capture-requests.patch`
- `nix/devices/r36tmax/mpp/README.md`

## Notes

Measured on branch feat/r36tmax-sunshine-mpp with Tomba (PS1, BIN/CUE, Mode2/2352).

Baselines, identical static scene:
- streaming OFF: 57.5-57.8 fps
- streaming ON: 32.5-35.1 fps
- encoder concurrently: 55-56 fps, 0 IOMMU faults

Scheduler data (10s samples, emulation thread):
- streaming OFF: run 98% of wall, 841 nonvoluntary preemptions
- streaming ON:  run 93% of wall, 5013 nonvoluntary preemptions
- streaming ON + CPU affinity (RetroArch 2,3 / Sunshine+Sway 0,1): run 97%, 376/s preemptions, still 35.11 fps

Ruled out with evidence, do not retry blindly:
- Wi-Fi: 5.33 Mbps actual, 0 tx failures, 53 retries/s, -31 dBm. The 183k retry figure is cumulative since boot.
- Sway software rendering: WLR_RENDERER=gles2, EGL 1.5, Panfrost on renderD128.
- PCSX-ReARMed speed hacks (nosmccheck, gteregsunneeded, nogteflags, nostalls): no measurable gain, caused visible graphical glitches, reverted.
- vsync halving (video_vsync=false, max_swapchain_images=3): no change.
- IRQ preemption on isolated cores: all device IRQs (VEPU, VOP/IOMMU, rk915) are already pinned to CPU0.

Kept because it genuinely helped: pcsx_rearmed_icache_emulation=disabled (54.5 -> 60.6 on an identical scene, ~11C cooler, no artefacts).

Promising direction: Sunshine has a KMS zero-copy path (patch 0023) but wlroots exposes no scanout framebuffer, which is why patch 0027 uses wlr-screencopy instead. Eliminating the redundant screencopy blit is the main lever.
