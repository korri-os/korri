# RG DS RKMPP kernel support: build verified, runtime blocked

## Result

The RG DS image now selects the existing RK356x MPP service, RKVENC binding,
system DMA heap, and video-group permissions. All three requested checks pass.
**Hardware encoding on the RG DS is unproven.** The runtime-only test stopped
before module insertion because the required components cannot be removed
without a reboot. No device deployment or Sunshine restart occurred.

## Change and grounding

The restored RG353M implementation from `cdaf55d60` moved to
`nix/devices/rockchip/`. All driver sources and the original overlay are
unchanged. The shared NixOS module accepts the device name and overlay. The
module package embeds that same selected overlay in its diagnostic helper.

The RG DS imports this module and replaces only the original overlay's board
match with `anbernic,rg-ds`. Linux 7.2's
`rk3568-anbernic-rg-ds.dts` and the running device both use that exact string.
The board includes `rk3568.dtsi`, which includes `rk356x-base.dtsi`; RK3566
includes the same base. The mainline base supplies the RKVENC power domain and
QoS nodes, but does **not** define the RKVENC encoder or its IOMMU. Mainline
therefore does not independently verify the `0xfdf40000` register address or
encoder interrupts. Those remain the existing vendor-derived RG353M binding,
not a newly measured RG DS hardware fact.

Verified Linux 7.2 binding constants are `ACLK_RKVENC=259`, `HCLK_RKVENC=260`,
`CLK_RKVENC_CORE=261`, `SRST_A_RKVENC=307`, `SRST_H_RKVENC=308`,
`SRST_RKVENC_CORE=309`, and `RK3568_PD_RKVENC=14`. Applying the same RKVENC-v1
binding on the RG DS is **inferred** from the shared RK356x IP and the existing
RG353M implementation. A successful hardware test is still required.

R36T Max's existing reuse of the MPP service sources now points to the shared
location. Its module output remains exactly
`/nix/store/m8m9jh0a25hqc2hxmd13b98pzidkd55f-rk-mpp-service-r36tmax-7.2.6`.
No R36T Max source or binding changed.

## Build verification

The new `checks.x86_64-linux.rgds-mpp` first failed before hardware enablement:

```text
error: rgds image must contain its RKMPP encoder kernel module
```

The shared check examines each image's selected module and merged DTB. It
checks the kernel ABI, DMA-BUF namespace, RKVENC symbol, heap ABI, boot loading,
permissions, enabled service/encoder/IOMMU nodes, register range, and IOMMU link.
The existing RG DS composition check now expects the selected MPP module and
overlay instead of empty lists.

Run the focused checks with the explicitly supplied cache and key:

```sh
nix build --no-link --print-out-paths -L \
  .#checks.x86_64-linux.rgds-mpp \
  .#checks.x86_64-linux.rg353m-mpp \
  .#checks.x86_64-linux.rgds \
  --option extra-substituters https://korri-os.github.io/nix-cache/ \
  --option extra-trusted-public-keys \
  korri-cache-ci-1:iH8gsPMtGrreeuXt2kt6M2ca+y4u/dO+T2km6AUuI8c=
```

| Check | Verified result |
| --- | --- |
| `rgds-mpp` | Pass: `/nix/store/avaqw1877pilmf6rly1dr6j5mk364ls0-rgds-mpp-check` |
| `rg353m-mpp` | Pass: `/nix/store/nn0amxaglfj6p5gl8v2picaplqm0hp0d-rg353m-mpp-check` |
| `rgds` | Pass: `/nix/store/lsa7sz7vkd7x5kfqljl9c2lbdbl5mags-rgds-module-check`; 22 image-verifier tests pass |
| `nixfmt --check` | Pass for all changed Nix files |
| `deadnix --fail` | Pass for all changed Nix files |
| `statix check` | Pass for the ten device Nix files; `flake.nix` retains its pre-existing repeated `devShells` keys warning |
| `git diff --check` | Pass |

The RG353M module output is unchanged:
`/nix/store/ijgzs39iybskd7a69k62y6a06z747swj-rk-mpp-service-rkvenc-7.2`.
Its merged device-tree output is also unchanged:
`/nix/store/571amxp9n8c4zkdd2a1iqx8hhdxpmlwk-device-tree-overlays`.
A recursive diff of the old and new RG353M check outputs found no differences.
These are byte-identical artifacts, not only equivalent assertions.

### Exact running kernel

The device's current and booted generation is unchanged:

`/nix/store/45bq492lq12f6pswcw58bckb1hmnczvx-nixos-system-rgds-sd-card-26.11.20260923.4975466`

Evaluating `nixosConfigurations.rgds.config.boot.kernelPackages.kernel` at full
source revision `ac3c4faa09752242f9f29bc7ded5fdb7b13a8677` selects:

- Image: `/nix/store/bgz5vwwcvz3wdn4bamha2ir9bpg0xmkr-linux-7.2`
- Development files: `/nix/store/ihlis03idqid9cnp8xv5r3j6j096ckmf-linux-7.2-dev`
- Modules: `/nix/store/qkqvg7k1jv22zpk1l8nr55qs3q2y8kc6-linux-7.2-modules`

The running generation's kernel symlink selects that same image. The new RG DS
module compiled on the ARM builder `yari`, against those same development files,
as `/nix/store/ssqdvipc4hkj8xxr2s0kjfnvjzj62hcl-rk-mpp-service-rkvenc-7.2`.
No kernel was rebuilt and no compilation ran on the device. The compiled boot
overlay is `/nix/store/zg7fbdps4drq2ih6kpszygypy8c3ds47-rgds-rkvenc-mpp-dtbo`.
Neither artifact was transferred to the device.

## Device preflight, 2026-10-25 UTC

All commands used the USB root console and were read-only system queries.
Raw local evidence is in `/tmp/rgds-mpp-test/`; it is not shipped with the image.

| Time | Verified observation |
| --- | --- |
| 07:54:35 | Linux `7.2.0`; original current/booted generation; no `rk_vcodec`, `rk_mpp_overlay`, `system_heap`, `/dev/mpp_service`, or `/dev/dma_heap/system` |
| 07:54:35 | Sunshine active, PID `1754`, `NRestarts=0`; journal reproduces the 05:24:17 missing MPP-service failure and 05:24:18 software fallback failure |
| 07:54:35 | Owner's Tetris DS/RetroArch process present, PID `63122`; no stop or kill command was sent |
| 07:54:35 | `CONFIG_OF_OVERLAY=y`, `CONFIG_DMABUF_HEAPS_SYSTEM=m`, `CONFIG_CONFIGFS_FS=y`; no `CONFIG_OF_CONFIGFS` |
| 08:00:07 | configfs contains only `pci_ep` and `usb_gadget`; no `/sys/kernel/config/device-tree/overlays` interface |
| 08:00:07 | `/dev/video1` is `rockchip,rk3568-vpu-dec`; `/dev/video2` is `rockchip,rk3568-vepu-enc`; no RKVENC interrupts present |
| 08:07:43 | Sunshine still active with PID `1754`, `NRestarts=0`; original generations and absent test modules/device nodes confirmed |
| 08:08:09 | RetroArch PID `63122` no longer exists; journal records shutdown/flush messages at 08:05:53. Cause unknown; this test made no game-control changes |

### Why insertion stopped

1. The running kernel has no configfs overlay interface. `CONFIG_OF_OVERLAY=y`
   alone does not expose one.
2. The existing `rk_mpp_overlay.c` deliberately omits a module exit function.
   Its comment records device-link corruption when removing dynamically added
   Rockchip IOMMU nodes. Its supported cleanup is reboot, not `rmmod`.
3. Linux 7.2's `drivers/dma-buf/heaps/system_heap.c` also has only a
   `module_init`, with no exit function. The exact kernel heap module and the
   built overlay helper both have `init_module` but no `cleanup_module` symbol.

Loading these components would violate the requested no-reboot, fully reverted
runtime test. No force unload or alternate removable helper was attempted.
The earlier RG353M acceptance record also removed its runtime overlay by reboot.

| Requested runtime result | Status |
| --- | --- |
| `Found H.264 encoder: h264_rkmpp` | Unproven; no new probe was run |
| 640x480/30 fps Moonlight stream | Not attempted; module/overlay safety gate failed first |
| Sunshine stream CPU | Unknown; not measured |
| Pairing without owner | Not attempted; no credentials or pairing state changed |

### Runtime changes and reverts

| Area | Change | Revert |
| --- | --- | --- |
| Modules, overlays, udev rules | None | Not needed |
| Sunshine service, overrides, configuration, pairing | None; zero restarts | Not needed |
| System/profile selection, `/boot`, eMMC/SD contents | None | Not needed |
| HTTP transfer server and host firewall | None | Not needed |
| Owner's `melonDS.opt` and Wi-Fi profile | Not read or edited | Not needed |
| Serial helper shell | Helper disables echo during queries, sets terminal variables/width, and restores echo on exit | Echo restored by helper; no system configuration change |

A later hardware test needs separate permission for reboot-assisted cleanup, or
separately verified reversible kernel support. Neither is implied by this test's
explicit stop-on-reboot-risk boundary. A permanent installation is also outside
this work: there was no switch, boot selection, land, or push.

## Separate software-fallback finding

**Verified:** the installed Sunshine executable is
`/nix/store/889z38dsimz0fx2vdf8p2lyx1f2nf1x3-sunshine-korri-2025.924.154138-korri/bin/sunshine-2025.924.154138-korri`.
Its actual derivation selects the static bundle
`/nix/store/r3ylkk8shgql48jklgq20jzyl3nkbbjy-sunshine-ffmpeg-rkmpp-8.0-61c5040`.
That bundle's derivation sets `-DBUILD_FFMPEG_X264:BOOL=FALSE`.

The installed executable's symbol table contains `ff_h264_rkmpp_encoder` and
`ff_h264_v4l2m2m_encoder`, but no `ff_libx264_encoder`. Its exact Sunshine source
at `/nix/store/gybylg65i7xxapkabpwy1jgscbb44b0l-source/src/video.cpp:1509-1513`
logs `Couldn't open [libx264]` when `avcodec_find_encoder_by_name` returns null.
This happens before codec initialization, not on an MPP allocation or a display
open. The fallback is missing from the ARM build, independently of the kernel
MPP defect. No userspace fix is included. Suggested follow-up: restore and test
the intended ARM software fallback in the Sunshine publisher, with its size and
CPU cost measured. No issue was filed.
