# Source provenance

These files are the narrow Rockchip MPP service/RKVENC-v1 slice copied from
`rockchip-linux/kernel` commit `470f9dccbdc42e7b8a824d0a5c5640a10e9457d2`
(`develop-6.12`):

- `drivers/video/rockchip/mpp/{mpp_service,mpp_common,mpp_iommu,mpp_rkvenc}.c`
- their local headers
- `include/uapi/linux/rk-mpp.h`

The port intentionally excludes Rockchip decoders, Hantro/JPEG drivers,
RKVENC2, RGA, vendor devfreq/system-monitor policy, and vendor DMA-BUF cache.
It uses mainline DMA/IOMMU APIs, constrains RKVENC-v1 to 32-bit DMA addresses,
and leaves IOMMU fault handling with the mainline DMA domain. The local
`rk_mpp_overlay.c` is a bring-up helper rather than vendor source; it embeds the
same overlay used by NixOS and is deliberately non-unloadable after insertion.
The original source is licensed GPL-2.0+ OR MIT as declared per file.
