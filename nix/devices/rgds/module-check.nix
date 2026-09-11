# Check the exported NixOS configuration, not a second model of the image.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  c = configuration.config;
  usbConsoleCheck = import ./usb-console-check.nix {
    inherit pkgs;
    config = c;
  };
  rawWrite = builtins.unsafeDiscardStringContext c.sdImage.postBuildCommands;
in
assert c.nixpkgs.hostPlatform.system == "aarch64-linux";
assert c.boot.kernelPackages.kernel.version == "7.2";
assert c.boot.kernelPackages.kernel.modDirVersion == "7.2.0";
assert c.hardware.deviceTree.name == "rockchip/rk3568-anbernic-rg-ds.dtb";
assert c.hardware.deviceTree.overlays == [ ];
assert !c.hardware.enableAllHardware;
assert !c.boot.initrd.allowMissingModules;
assert !(lib.elem "pata_qdi" c.boot.initrd.availableKernelModules);
assert c.boot.extraModulePackages == [ ];
assert c.boot.loader.generic-extlinux-compatible.enable;
assert !c.boot.loader.grub.enable;
assert c.fileSystems."/".device == "/dev/disk/by-label/NIXOS_RGDS";
assert lib.all (fs: !(lib.hasPrefix "/dev/mmcblk" fs.device)) (lib.attrValues c.fileSystems);
assert c.swapDevices == [ ];
assert c.sdImage.firmwarePartitionOffset == 16;
assert lib.hasInfix ''of="$img" bs=512'' rawWrite;
assert lib.hasInfix "seek=64 conv=notrunc" rawWrite;
assert lib.hasInfix "U-Boot exceeds the raw area" rawWrite;
assert lib.elem "panel-jadard-jd9365da-h3" c.boot.initrd.kernelModules;
assert lib.elem "panfrost" c.boot.initrd.kernelModules;
assert lib.elem "g_serial" c.boot.kernelModules;
assert !(c.systemd.units ? "serial-getty@ttyGS0.service");
assert c.services.getty.autologinUser == "root";
assert !c.services.openssh.enable && !c.services.openssh.openFirewall;
assert c.users.users.root.openssh.authorizedKeys.keys == [ ];
assert c.nix.settings.max-jobs == 0;
assert c.nix.settings.require-sigs;
assert !c.services.sunshine.enable;
assert lib.elem "korrid" (map lib.getName c.environment.systemPackages);
pkgs.runCommand "rgds-module-check"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.util-linux
      pkgs.e2fsprogs
      pkgs.dosfstools
    ];
  }
  ''
    test -f ${usbConsoleCheck}
    cp ${./verify-image.py} verify-image.py
    cp ${./verify-image.test.py} verify-image.test.py
    python3 verify-image.test.py
    touch "$out"
  ''
