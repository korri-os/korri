# RG DS board and Jadard dual-panel support are upstream in Linux 7.x.
# Keep this source override local: the other devices retain their kernel pins.
{
  lib,
  linux_latest,
  fetchurl,
  structuredExtraConfig ? { },
  ...
}@args:
linux_latest.override (
  (builtins.removeAttrs args [
    "lib"
    "linux_latest"
    "fetchurl"
  ])
  // {
    argsOverride = {
      version = "7.2";
      modDirVersion = "7.2.0";
      isLTS = false;
      src = fetchurl {
        url = "https://cdn.kernel.org/pub/linux/kernel/v7.x/linux-7.2.tar.xz";
        hash = "sha256-+f7z0UwN9TgZAm9L50RZg1wqCw3L9bW72eoZ8IKUArM=";
      };
      extraMeta.branch = "7.2";
    };
    structuredExtraConfig =
      structuredExtraConfig
      // (with lib.kernel; {
        DRM_ROCKCHIP = module;
        ROCKCHIP_DW_MIPI_DSI = yes;
        DRM_PANEL_JADARD_JD9365DA_H3 = module;
        DRM_PANFROST = module;
        TOUCHSCREEN_GOODIX = module;
        USB_G_SERIAL = module;
        USB_DWC3 = module;
        USB_DWC3_DUAL_ROLE = yes;
        # The pinned nixpkgs common config still requests these removed 7.2
        # symbols. Keep strict config checking; remove only audited stale names.
        AX25 = lib.mkForce unset;
        CRYPTO_DRBG_CTR = lib.mkForce unset;
        CRYPTO_DRBG_HASH = lib.mkForce unset;
        DMABUF_MOVE_NOTIFY = lib.mkForce unset;
        FB_HYPERV = lib.mkForce unset;
        HAMRADIO = lib.mkForce unset;
        HIPPI = lib.mkForce unset;
        NFS_V4_1 = lib.mkForce unset;
        # mm/Kconfig makes RANDOM_KMALLOC_CACHES a transitional symbol.
        RANDOM_KMALLOC_CACHES = lib.mkForce unset;
        KMALLOC_PARTITION_CACHES = yes;
        # ARM64 now has ARCH_HAS_PREEMPT_LAZY, which hides VOLUNTARY.
        PREEMPT_VOLUNTARY = lib.mkForce unset;
        PREEMPT = lib.mkForce yes;
      });
  }
)
