# Rockchip MPP kernel support

`rk-mpp-service/` is the existing narrow Rockchip MPP service/RKVENC-v1 port.
It moved here from `rg353m/` without source changes. See its
[`PROVENANCE.md`](rk-mpp-service/src/PROVENANCE.md). R36T Max also consumes
these service sources, with its own VEPU2 driver and binding.

`rk-mpp-service.nix` supplies the kernel package, boot loading, DT overlay, and
video-group access to MPP and the system DMA heap. Sunshine supplies userspace.
The default overlay is the unchanged RG353M binding. The RG DS selects the same
binding with only its board-compatible string replaced. Both boot and diagnostic
module builds consume the selected overlay; neither board auto-loads the
runtime overlay helper. `mpp-check.nix` checks each image's actual modules and DTB.

## Runtime-test boundary

Do not treat the diagnostic helper as a reversible test:

- `rk-mpp-service/src/rk_mpp_overlay.c` deliberately has no exit function.
  Removing dynamically added Rockchip IOMMU nodes can corrupt device links.
  The helper requires a reboot to remove it.
- Linux 7.2 `drivers/dma-buf/heaps/system_heap.c` also has no module exit
  function. Loading `system_heap` requires a reboot to remove it.
- `CONFIG_OF_OVERLAY=y` does not provide a configfs overlay interface by itself.
  Check the running kernel and `/sys/kernel/config/device-tree/overlays` first.

A test that requires all modules and overlays to be unloaded without reboot
cannot use these components. Stop before insertion. Do not force-unload them
or add a removable overlay helper to bypass the device-link hazard.
