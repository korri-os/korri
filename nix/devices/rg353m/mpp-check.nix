# Check the modules and merged DTB selected by the actual RG353M image.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  config = configuration.config;
  kernel = config.boot.kernelPackages.kernel;
  selected = lib.findFirst (
    p: lib.getName p == "rk-mpp-service-rkvenc"
  ) null config.boot.extraModulePackages;
  modules = "${selected}/lib/modules/${kernel.modDirVersion}/updates";
  dtb = "${config.hardware.deviceTree.package}/${config.hardware.deviceTree.name}";
in
assert lib.assertMsg (selected != null) "RG353M image must contain its RKMPP encoder kernel module";
assert builtins.elem "rk_vcodec" config.boot.kernelModules;
assert lib.assertMsg (builtins.elem "system_heap" config.boot.kernelModules)
  "RG353M RKMPP needs a loaded system DMA heap for buffer allocation";
assert lib.any (overlay: overlay.name == "rg353m-rkvenc-mpp") config.hardware.deviceTree.overlays;
assert lib.hasInfix ''KERNEL=="mpp_service", GROUP="video", MODE="0660"''
  config.services.udev.extraRules;
assert lib.hasInfix ''SUBSYSTEM=="dma_heap", KERNEL=="system", GROUP="video", MODE="0660"''
  config.services.udev.extraRules;
pkgs.runCommand "rg353m-mpp-check"
  {
    nativeBuildInputs = [
      pkgs.kmod
      pkgs.binutils
      pkgs.dtc
    ];
  }
  ''
    mkdir -p "$out"
    modinfo ${modules}/rk_vcodec.ko > "$out/modinfo.txt"
    modinfo -F vermagic ${modules}/rk_vcodec.ko | grep '^${kernel.modDirVersion} '
    test "$(modinfo -F import_ns ${modules}/rk_vcodec.ko)" = DMA_BUF
    readelf --wide --symbols ${modules}/rk_vcodec.ko > "$out/symbols.txt"
    grep ' rockchip_rkvenc_driver$' "$out/symbols.txt"
    modinfo -F vermagic ${modules}/rk_mpp_overlay.ko | grep '^${kernel.modDirVersion} '
    modinfo -F vermagic ${kernel.modules}/lib/modules/${kernel.modDirVersion}/kernel/drivers/dma-buf/heaps/system_heap.ko.xz | grep '^${kernel.modDirVersion} '

    test "$(fdtget ${dtb} /mpp-srv compatible)" = rockchip,mpp-service
    test "$(fdtget ${dtb} /mpp-srv status)" = okay
    test "$(fdtget ${dtb} /rkvenc@fdf40000 compatible)" = rockchip,rkv-encoder-v1
    test "$(fdtget ${dtb} /rkvenc@fdf40000 status)" = okay
    test "$(fdtget -t x ${dtb} /rkvenc@fdf40000 reg)" = '0 fdf40000 0 400'
    test "$(fdtget -t x ${dtb} /rkvenc@fdf40000 iommus)" = "$(fdtget -t x ${dtb} /iommu@fdf40f00 phandle)"
    test "$(fdtget ${dtb} /iommu@fdf40f00 status)" = okay
    cp ${dtb} "$out/rk3566-anbernic-rg353p.dtb"
  ''
