# R36T Max Sunshine encoding: host-only prototype

Hardware-encoded Sunshine hosting is not working yet. Do not load this driver
or apply this overlay to the handheld. The earlier successful module build
proved compilation only; it did not prove the PX30 port was correct.

The normal portal, governor, board DTS and boot images remain unchanged.
`compile-check.nix` installs ELF/module metadata only. It installs no `.ko`.
`binding-check.nix` applies a disabled candidate to the real board DTB on the
host. Neither check has a device deployment path.

## Corrections to the first draft

The first draft used a generic VEPU2 binding, omitted GRF configuration,
removed the PX30 workaround and copied the RG353M no-op PM idle operation.
Its runtime overlay also used power-domain ID 1, which is `PX30_PD_A35_1`,
not `PX30_PD_VPU`, whose ID is 11. Those are defects, not tested alternatives.
The board DTS used the correct symbolic constant, so the two artifacts were
not equivalent. The unsafe runtime loader and service module were removed.

The previous output at
`/nix/store/3big9xm0ckb6cmpkj90kidppx1b0xhbv-rk-mpp-service-vepu2-6.12.63`
is not approved for device use. Its `result` link was removed from the worktree.
No module, overlay, governor change or reboot was applied to the handheld.

Claims that software encoding must shut down the R36T Max, that changing the
CPU governor fixes its heat, and that 720x720 at 60 fps is proven were incorrect.
No such measurements were made. RG353M results do not prove R36T Max performance.
The earlier VPU fault belongs to the boot recorded in the
[hardware survey](../HARDWARE-SURVEY-2026-09-14.md#stage-4-first-codec-job-failed).
Read the current boot identity before making a recovery decision.

## Grounding

The kernel reference is
[rockchip-linux/kernel at 470f9dccbdc42e7b8a824d0a5c5640a10e9457d2](https://github.com/rockchip-linux/kernel/tree/470f9dccbdc42e7b8a824d0a5c5640a10e9457d2).
Relevant files:

- `arch/arm64/boot/dts/rockchip/px30.dtsi:1627-1694` defines the service,
  encoder, GRF selection, shared resets, clocks and IOMMU references.
- `drivers/video/rockchip/mpp/mpp_vepu2.c` defines distinct generic and PX30
  variants. PX30 uses `vepu_px30_init`, `vepu_px30_run` and
  `px30_workaround_combo_switch_grf`.
- `drivers/video/rockchip/mpp/hack/mpp_hack_px30.c` owns the vendor combo
  switching sequence. It reads paging state, controls clocks/power, changes
  GRF selection and restores the page-directory address and paging state.
- `include/dt-bindings/power/px30-power.h` defines the power-domain IDs.

The selected mainline kernel is Linux 6.12.63. The host source archive is
`/nix/store/adddx8796jmn91lvgnsd05ycl180as99-linux-6.12.63.tar.xz`.
Its `include/linux/iommu.h:856-859` calls `flush_iotlb_all` only if the driver
supplies that callback. Its `drivers/iommu/rockchip-iommu.c:1164-1178` supplies
no such callback. This concerns the explicit full flush in this port;
mainline's own mapping/unmapping paths still perform their own invalidation.

Pinned userspace is
[rockchip-linux/mpp at 0986d01294d5c2449c14cf13af9b740368c33967](https://github.com/rockchip-linux/mpp/tree/0986d01294d5c2449c14cf13af9b740368c33967).
`osal/mpp_soc.c:916-934` lists RK3326/PX30 with VEPU2.
`mpp/hal/vpu/h264e/hal_h264e_vepu2_v2.c` implements the H.264 backend.
`mpp/hal/vpu/common/vepu_common.c` has RGB format configurations. This is source
support, not proof of dma-buf import or KMS zero-copy on this handheld.

## Dependency ledger

| Operation | Current draft | Required before a device load |
|---|---|---|
| PX30 selection and GRF switching | The generic variant replaced the PX30 path. | Preserve the vendor semantics through a reviewed mainline implementation, or prove a narrower encoder-only route. Do not remove switching merely because one encoder is requested. |
| Bus idle around reset | Copied `mpp_pmu_idle_request` returns success without an operation. | Provide the required handshake at the PM-domain owner. Runtime PM being active does not establish bus idle. |
| Explicit full TLB flush | Calls `iommu_flush_iotlb_all`, which does nothing with the selected Rockchip driver's callbacks. | Implement and check a real full flush at the IOMMU owner. |
| Post-reset IOMMU restoration | Refresh only invokes that full-flush helper. | Restore valid directory, paging and interrupt state after reset, with ordering and failures checked. A flush alone is not reset recovery. |
| Fault containment | Vendor interrupt masking was removed. | Bound repeated faults and prove cleanup after a failed job without bypassing IOMMU protection. |
| Initialization and power failures | The common driver continues after IOMMU probe failure and ignores power/clock errors. | Stop before register writes on failed dependencies. Prove resource cleanup and error propagation. |

The vendor IOMMU also consumes `rockchip,shootdown-entire`; mainline 6.12.63
does not. The offline overlay does not copy that ineffective property. Its
presence alone would not implement the missing operation.

These gaps are in kernel boundaries, not Sunshine codec configuration.
Resolving them can require a narrow kernel patch as well as an external module.
The cost is a larger reviewed kernel change and another image build before
hardware tests. Copying vendor MMIO manipulation alongside the mainline IOMMU
owner is not an accepted shortcut.

## Checks

From this worktree:

```sh
nix build .#checks.x86_64-linux.r36tmax-mpp-binding --no-link -L
nix build .#checks.aarch64-linux.r36tmax-mpp-compile --no-link -L
nix build .#checks.x86_64-linux.r36tmax --no-link -L
```

The binding check compiles the actual board and overlay, applies the overlay
with `fdtoverlay`, and checks the resulting DTB. It requires MPP and VEPU to
remain disabled. It also runs the existing button, speaker and radio/eMMC
checks against the candidate. Twenty-five mutations cover the old generic
binding, reset ownership, power domain, clocks, GRF, IRQ and queue mistakes.
The stricter checker rejects the old previously passing DTB.

The ARM compile check uses the selected kernel and a build host, never the
handheld. It reads `modinfo` and the ELF symbol table. It cannot establish
register correctness, fault recovery, encoding, streaming or thermal limits.

## Host verification on 2026-09-15

The three check commands above passed. ARM compilation ran on fuji against
Linux 6.12.63. The final compile output is
`/nix/store/77pv8j718965sw8gbg6274h4lr2ys2l0-r36tmax-mpp-compile-check-6.12.63`.
The binding output is
`/nix/store/prmdj217aalb60qfa164k4729sf5drbd-r36tmax-mpp-binding-check`.
Twenty-five MPP, fourteen button and twenty speaker mutations were rejected.
Python lint, Nix formatting checks and `git diff --check HEAD` also passed.
The build retains existing board DTC warnings; this is not a warning-free
or physical-device acceptance claim.

A read-only comparison against main at `f3aa66d9` found identical derivations
for the normal system, recovery system, diagnostic SD image and mainline-loader
SD image. A separate source review found no remaining automatic deployment
wiring. The review did not approve the unfinished driver for hardware use.
No change from this work is deployed or merged.

## Next steps

1. Resolve the dependency ledger against the owning mainline PM-domain and
   IOMMU drivers. Keep all normal image wiring unchanged during that work.
2. Add tests for initialization failures, real flush/reset ordering and bounded
   fault recovery before exposing a loadable package again.
3. Review the exact driver and compiled DTB together. One reviewed artifact
   must own the binding; do not keep a second, divergent runtime overlay.
4. Build and sign a separate SD test image. Verify the actual boot/recovery
   state and agree the physical test window before changing the handheld.
   Never write internal storage or compile on the target.
5. Prove a bounded standalone H.264 encode and decode the output on a host.
   Record completions, errors, memory, temperature and cleanup. Then test KMS
   dma-buf import before a Sunshine session.
6. Reuse the existing strict RKMPP Sunshine profile only after those gates.
   Its framebuffer formats, stride, synchronization and fallback costs need
   R36T Max measurements. Start with a bounded target; do not promise 60 fps.

The RG353M comparison remains
[the RKMPP acceptance record](../../../../docs/acceptance/sunshine-korri-rg353m-rkmpp-2026-09-05.md).
The existing userspace pieces are reusable candidates, not proof that no
FFmpeg or Sunshine changes will be needed.
