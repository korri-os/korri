# R36T Max hardware H.264 encoding

The R36T Max encodes H.264 on its PX30/RK3326 VEPU2 block through Rockchip's
MPP service. Mainline Linux 7.2.6 binds that block with the Hantro driver, which
offers JPEG encoding only, so hardware H.264 needs this out-of-tree service.

This directory holds the kernel side. The userspace side is the existing
`aarch64-linux-rkmpp` Sunshine profile plus `services/sunshine/patches/0027`,
which hands the compositor's dma-buf to the encoder without a CPU copy.

## What is here

| File | Purpose |
| --- | --- |
| `src/mpp_vepu2.c` | PX30 VEPU2 device variant for Rockchip's MPP service. |
| `src/Makefile` | Builds `rk_vcodec.ko` and `rk_vepu_overlay.ko`. |
| `src/PROVENANCE.md` | Vendor source, what was kept, and what was dropped. |
| `rk-mpp-runtime-overlay.dts` | Live overlay that adds `mpp-srv` and `vepu@ff442000`. |
| `rk-mpp-service-module.nix` | Builds both modules against the device kernel. |
| `compile-check.nix` | Reads real module metadata from that same derivation. |
| `binding-check.nix` | Compiles the offline binding candidate and rejects mutations. |

## Bring-up on a running device

The Hantro driver owns `ff442000.video-codec` at boot. Unbind it before adding
the MPP nodes; a device-tree overlay cannot take a live IOMMU-attached node away
from a bound driver without faulting inside `rk_iommu_release_device`.

```sh
rmmod hantro_vpu
insmod .../updates/rk_vepu_overlay.ko
insmod .../updates/rk_vcodec.ko
```

`/dev/mpp_service` and `/dev/dma_heap/system` both have to be reachable by the
user that runs Sunshine. MPP allocates its reference frames from the dma-heap,
and a `root:root 0600` heap fails the encoder probe with
`Failed to get MPP internal buffer group`.

## Measured on hardware, 2026-09-15

Linux 7.2.6, Sway compositor, 720x720, H.264, zero-copy Wayland dma-buf input.

| Measure | Result |
| --- | --- |
| Client-decoded frame rate | 10,514 frames in 178.7 s, **58.83 fps** |
| Per-window spread | 86 of 89 two-second windows at 117 frames or more |
| Sunshine CPU | about 40% of one core |
| Idle CPU during the stream | about 49% across four cores |
| Peak package temperature | 80.8 C, settling to 70 C |
| Kernel log | no IOMMU page faults, no resets logged as errors, no timeouts |

Standalone `mpi_enc_test` reached 90.84 fps at 720x720 over 200 frames with a
13 ms per-frame latency, so the encoder is not the pacing limit; Sunshine paces
its encode loop to the negotiated 60 fps.

The same session on the software `libx264` profile sustained 28.5 fps at about
210% CPU. The win comes from two places: the VEPU2 does the encode, and its
preprocessor does the RGB-to-YUV conversion that previously cost 8.3 ms of CPU
per frame.

### What this does not cover

Audio, persistent boot activation, and input have not been tested on this path.
The overlay is loaded by hand; nothing in the normal image loads it yet. The
recorded temperatures were taken with the portal browser stopped, which by
itself accounts for about 15 C on this board.

## Checks

From this worktree:

```sh
nix build .#checks.x86_64-linux.r36tmax-mpp-binding --no-link -L
nix build .#checks.aarch64-linux.r36tmax-mpp-compile --no-link -L
nix build .#checks.x86_64-linux.r36tmax --no-link -L
```

The binding check compiles the actual board and overlay, applies the overlay
with `fdtoverlay`, and checks the resulting DTB. It requires both MPP nodes to
remain disabled in that offline candidate. Twenty-five mutations cover the old
generic binding, reset ownership, power domain, clocks, GRF, IRQ and queue
mistakes. It also runs the existing button, speaker and radio/eMMC checks
against the candidate.

The compile check reads `modinfo` and the ELF symbol table of the same modules
the device loads. It establishes identity and ABI, not register correctness.

## Grounding

The kernel reference is
[rockchip-linux/kernel at 470f9dccbdc42e7b8a824d0a5c5640a10e9457d2](https://github.com/rockchip-linux/kernel/tree/470f9dccbdc42e7b8a824d0a5c5640a10e9457d2).
Relevant files:

- `arch/arm64/boot/dts/rockchip/px30.dtsi:1627-1694` defines the service,
  encoder, GRF selection, shared resets, clocks and IOMMU references.
- `drivers/video/rockchip/mpp/mpp_vepu2.c` defines distinct generic and PX30
  variants.
- `include/dt-bindings/power/px30-power.h` defines the power-domain IDs;
  `PX30_PD_VPU` is 11.

Pinned userspace is
[rockchip-linux/mpp at 0986d01294d5c2449c14cf13af9b740368c33967](https://github.com/rockchip-linux/mpp/tree/0986d01294d5c2449c14cf13af9b740368c33967).
`osal/mpp_soc.c:916-934` lists RK3326/PX30 with VEPU2.

The RG353M comparison is
[the RKMPP acceptance record](../../../../docs/acceptance/sunshine-korri-rg353m-rkmpp-2026-09-05.md).
That device uses RKVENC-v1 and KMS capture; this one uses VEPU2 and Wayland
capture, so the encoder driver and the Sunshine capture route both differ.

## Next steps

1. Wire the overlay, modules and device permissions into the normal image so
   the encoder survives a reboot, then re-measure from a cold boot.
2. Give Sunshine the `rkmpp` encoder and `SUNSHINE_STRICT_ENCODER=1` on this
   device, matching the RG353M policy, so a probe failure fails the unit rather
   than silently streaming at 28 fps on the CPU.
3. Fix audio capture. Sunshine cannot reach PulseAudio from its hardened unit
   (`/run/user/1001/pulse` is read-only), so every session is video-only.
4. Decide whether the handheld's own Moonlight client should use the Hantro
   decoder that
   [the codec path record](../CODEC-PATH.md) already proves.
