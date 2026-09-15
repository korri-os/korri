# Source provenance

`mpp_vepu2.c` was adapted from `rockchip-linux/kernel` commit
`470f9dccbdc42e7b8a824d0a5c5640a10e9457d2`, path
`drivers/video/rockchip/mpp/mpp_vepu2.c`. The fetched original has SHA-256
`cc40cae9824fe8ff75d241fa8fa3272f9e1a67b5b7e819a5fb05b757b8034772`.

The module derivation composes this file with Korri's existing common files and
UAPI header in `nix/devices/rg353m/rk-mpp-service/src/`, rather than keeping a
second copy of them. Those files were already adapted for mainline DMA/IOMMU
APIs while porting RK3566 RKVENC; the same adaptations apply here.

## What this port keeps and what it drops

The PX30 device variant is present: `rockchip,vpu-encoder-px30` selects
`vepu_px30_hw_ops`/`vepu_px30_dev_ops`, which flush the IOMMU TLB before each
job and write the GRF codec selection before each run.

The vendor's `hack/mpp_hack_px30.c` combo workaround is **not** carried over.
That code exists because the vendor tree drives the decoder and the encoder
behind one GRF mux, and has to stall, reset and re-page the shared IOMMU every
time it switches between them. This port enables only the encoder node; the
decoder and the mainline Hantro driver never bind to the same block at the same
time, so no mux switch happens while the hardware is live and there is nothing
to stall. `mpp_set_grf` writes the encoder selection once per run, which is the
part of the vendor sequence that still applies.

For the same reason the vendor's private PM bus-idle handshake is not required:
no reset is issued while a second engine holds the bus. Runtime PM owns the
power domain, and `vepu_reset` drives only the encoder's own clocks and resets.

The port intentionally excludes Rockchip decoders, Hantro/JPEG drivers,
RKVENC/RKVENC2, RGA, vendor devfreq/system-monitor policy, and the vendor
DMA-BUF cache.

## Runtime overlay

`rk_vepu_overlay.ko` applies the compiled `rk-mpp-runtime-overlay.dts` at module
load. It is a bring-up aid: it adds the `mpp-srv` and `vepu@ff442000` nodes to
the live device tree without replacing the boot DTB. It deliberately has no exit
path, because removing dynamically added nodes corrupts mainline's device-link
list. Reboot to remove it.

The overlay does **not** disable `/video-codec@ff442000`. Overriding a live
node's status through `of_overlay_fdt_apply` destroys the platform device while
its IOMMU group still references it, which faults inside
`rk_iommu_release_device`. Unbind the Hantro driver first (`rmmod hantro_vpu`),
then load the overlay.

Original vendor source is GPL-2.0+ OR MIT, as declared per file. This port
retains those notices.
