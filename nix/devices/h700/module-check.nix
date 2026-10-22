# Check the exported Anbernic H700 configurations. This proves declaration
# only; it does not grant hardware support.
{
  pkgs,
  korri,
  configurations,
}:
let
  inherit (pkgs) lib;
  expected = {
    rg35xxsp = {
      dtb = "sun50i-h700-anbernic-rg35xx-sp";
      root = "NIXOS_RG35XXSP";
      image = "nixos-rg35xxsp";
    };
    rg35xxsp-v2-panel = {
      dtb = "sun50i-h700-anbernic-rg35xx-sp-v2-panel";
      root = "NIXOS_RG35XXSP";
      image = "nixos-rg35xxsp-v2-panel";
    };
    rg35xxpro = {
      dtb = "sun50i-h700-anbernic-rg35xx-pro";
      root = "NIXOS_RG35XXPRO";
      image = "nixos-rg35xxpro";
    };
  };
  crossKernel = korri.packages.x86_64-linux.h700-kernel;
  inputData = korri.packages.aarch64-linux.h700-inputplumber-data;
  check =
    name: facts:
    let
      c = configurations.${name}.configuration.config;
      kernel = c.boot.kernelPackages.kernel;
      rawWrite = builtins.unsafeDiscardStringContext c.sdImage.postBuildCommands;
    in
    assert c.nixpkgs.hostPlatform.system == "aarch64-linux";
    assert c.services.korriProduct.installed;
    assert c.hardware.deviceTree.name == "allwinner/${facts.dtb}.dtb";
    assert c.hardware.deviceTree.filter == "${facts.dtb}.dtb";
    assert c.hardware.deviceTree.overlays == [ ];
    assert !c.hardware.enableAllHardware;
    assert !c.hardware.enableRedistributableFirmware;
    assert c.boot.loader.generic-extlinux-compatible.enable;
    assert !c.boot.loader.grub.enable;
    assert c.fileSystems."/".device == "/dev/disk/by-label/${facts.root}";
    assert c.image.baseName == facts.image;
    assert c.sdImage.firmwarePartitionOffset == 16;
    assert lib.hasInfix ''of="$img" bs=512'' rawWrite;
    assert lib.hasInfix "seek=16 conv=notrunc" rawWrite;
    assert lib.hasInfix "U-Boot exceeds the raw area" rawWrite;
    assert !c.boot.initrd.includeDefaultModules;
    assert c.boot.initrd.availableKernelModules == [ ];
    assert c.boot.initrd.kernelModules == [ ];
    # The image carries the off-device cross kernel and its derived config.
    assert kernel.drvPath == crossKernel.drvPath;
    assert lib.versions.major kernel.stdenv.cc.cc.version == "15";
    assert kernel.config.isModule "JOYSTICK_ROCKNIX_SINGLEADC";
    assert kernel.config.isYes "VIDEO_SUNXI";
    assert kernel.config.isModule "VIDEO_SUNXI_CEDRUS";
    assert kernel.config.isYes "USB_CONFIGFS_ACM";
    assert kernel.config.isYes "NF_TABLES";
    assert kernel.config.isYes "NFT_COMPAT";
    assert lib.elem (toString inputData) (
      map toString c.services.korriLinuxInput.provider.extraDataPackages
    );
    assert c.services.korriLinuxHost.compositor.outputName == "DSI-1";
    assert c.services.korriLinuxHost.compositor.mode == "640x480@60Hz";
    true;
  gadgets = map (name: configurations.${name}.configuration.config.services.korriProduct.usbGadget) [
    "rg35xxsp"
    "rg35xxpro"
  ];
in
assert lib.all (name: check name expected.${name}) (builtins.attrNames expected);
assert lib.allUnique (map (gadget: gadget.name) gadgets);
assert lib.allUnique (map (gadget: gadget.address) gadgets);
assert lib.allUnique (map (gadget: gadget.hostMac) gadgets);
# The video engine decodes from its own reusable CMA pool with no IOMMU link.
# Any `iommus` property makes Linux turn on translation for every master,
# and the display engine then faults on wake (patch 0236; see
# docs/research/h700-video-engine-worksheet.md).
pkgs.runCommand "h700-module-check"
  {
    nativeBuildInputs = [
      pkgs.dtc
      pkgs.binutils
    ];
  }
  ''
    # Both nodes must ship in the same module, with one engine owner.
    module="${crossKernel.modules}/lib/modules/7.2.0/kernel/drivers/staging/media/sunxi/cedrus/sunxi-cedrus.ko"
    nm "$module" > symbols
    grep -q ' T cedrus_enc_register$' symbols
    grep -q ' T cedrus_enc_irq$' symbols
    grep -q ' T cedrus_device_run$' symbols
    grep -aq 'cedrus-encoder' "$module"

    for name in ${lib.concatMapStringsSep " " (facts: facts.dtb) (builtins.attrValues expected)}; do
      dtc -q -I dtb -O dts "${crossKernel}/dtbs/allwinner/$name.dtb" > tree.dts
      if grep -q 'iommus' tree.dts; then
        echo "$name: a node links the IOMMU" >&2; exit 1
      fi
      sed -n '/video-codec@1c0e000 {/,/};/p' tree.dts > ve.dts
      grep -q 'sun50i-h616-video-engine' ve.dts || { echo "$name: no H616 video engine" >&2; exit 1; }
      grep -q 'memory-region' ve.dts || { echo "$name: video engine has no memory pool" >&2; exit 1; }
      sed -n '/ve-pool {/,/};/p' tree.dts > pool.dts
      grep -q 'shared-dma-pool' pool.dts && grep -q 'reusable' pool.dts \
        || { echo "$name: video engine pool is not a reusable CMA pool" >&2; exit 1; }
    done
    touch $out
  ''
