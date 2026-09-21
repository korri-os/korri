# Check the exported RG35XXSP NixOS configuration.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  c = configuration.config;
  rawWrite = builtins.unsafeDiscardStringContext c.sdImage.postBuildCommands;
in
assert c.nixpkgs.hostPlatform.system == "aarch64-linux";
assert c.hardware.deviceTree.name == "allwinner/sun50i-h700-anbernic-rg35xx-sp.dtb";
assert c.hardware.deviceTree.overlays == [ ];
assert !c.hardware.enableAllHardware;
assert c.boot.loader.generic-extlinux-compatible.enable;
assert !c.boot.loader.grub.enable;
assert c.fileSystems."/".device == "/dev/disk/by-label/NIXOS_RG35XXSP";
assert c.sdImage.firmwarePartitionOffset == 16;
assert lib.hasInfix ''of="$img" bs=512'' rawWrite;
assert lib.hasInfix "seek=16 conv=notrunc" rawWrite;
assert lib.hasInfix "U-Boot exceeds the raw area" rawWrite;
assert !c.boot.initrd.includeDefaultModules;
assert c.boot.initrd.availableKernelModules == [ ];
assert c.boot.initrd.kernelModules == [ ];
pkgs.runCommand "rg35xxsp-module-check" { } "touch $out"
