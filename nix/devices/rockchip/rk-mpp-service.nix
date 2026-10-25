# Device kernel support for the RK356x RKVENC H.264 encoder. The Sunshine
# plugin supplies MPP userspace; the device owns the driver and its binding.
{
  device,
  overlay ? ./rk-mpp-service-overlay.dts,
}:
{
  config,
  lib,
  ...
}:
{
  boot.extraModulePackages = [
    (config.boot.kernelPackages.callPackage ./rk-mpp-service-module.nix { inherit overlay; })
  ];
  # MPP 1.1.0 prefers a DMA heap over its vendor DRM allocator. Mainline's
  # Panfrost render node cannot serve that allocator's Rockchip GEM ioctl.
  boot.kernelModules = [
    "system_heap"
    "rk_vcodec"
  ];

  hardware.deviceTree.overlays = lib.mkAfter [
    {
      name = "${device}-rkvenc-mpp";
      dtsFile = overlay;
    }
  ];

  services.udev.extraRules = ''
    KERNEL=="mpp_service", GROUP="video", MODE="0660"
    SUBSYSTEM=="dma_heap", KERNEL=="system", GROUP="video", MODE="0660"
  '';
}
