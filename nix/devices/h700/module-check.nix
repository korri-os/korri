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
pkgs.runCommand "h700-module-check" { } "touch $out"
