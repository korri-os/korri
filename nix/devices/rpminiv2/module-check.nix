# Assertions read the exported NixOS configuration. The CLI tests exercise
# actual GPT/FAT/ext4 files, not another model of the NixOS image.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  c = configuration.config;
  packageNames = map lib.getName c.environment.systemPackages;
in
assert c.nixpkgs.hostPlatform.system == "aarch64-linux";
assert c.networking.hostName == "rpminiv2";
assert c.hardware.deviceTree.name == "qcom/sm8250-retroidpocket-rpminiv2.dtb";
assert c.hardware.deviceTree.overlays == [ ];
assert !c.hardware.enableAllHardware;
assert c.hardware.graphics.enable;
assert c.hardware.bluetooth.enable;
assert !c.hardware.bluetooth.powerOnBoot;
assert lib.elem "bluez" packageNames;
assert lib.elem "bluetooth.target" c.systemd.services.bluetooth.wantedBy;
assert c.hardware.firmwareCompression == "none";
assert c.boot.consoleLogLevel == 8;
assert lib.last (lib.filter (lib.hasPrefix "console=") c.boot.kernelParams) == "console=tty0";
assert c.boot.initrd.compressor == "gzip";
assert !c.boot.initrd.allowMissingModules;
assert !c.boot.initrd.includeDefaultModules;
assert lib.elem "qcom/sm8250/slpi.mbn" c.boot.initrd.extraFirmwarePaths;
assert lib.elem "regulatory.db.p7s" c.boot.initrd.extraFirmwarePaths;
assert c.boot.loader.systemd-boot.enable;
assert !c.boot.loader.grub.enable;
assert !c.boot.loader.generic-extlinux-compatible.enable;
assert !c.boot.loader.efi.canTouchEfiVariables;
assert c.boot.loader.timeout == 0;
assert c.fileSystems."/".device == "/dev/disk/by-label/NIXOS_RPMINIV2";
assert c.fileSystems."/boot".device == "/dev/disk/by-label/RPMINIV2";
assert lib.all (
  fs: !(lib.hasPrefix "/dev/mmcblk" fs.device) && !(lib.hasPrefix "/dev/sd" fs.device)
) (lib.attrValues c.fileSystems);
assert c.swapDevices == [ ];
assert lib.elem "systemd.gpt_auto=0" c.boot.kernelParams;
assert lib.elem "rd.systemd.gpt_auto=0" c.boot.kernelParams;
assert lib.elem "g_serial" c.boot.kernelModules;
assert !(c.systemd.units ? "serial-getty@ttyGS0.service");
assert lib.hasInfix "serial-getty@ttyGS0.service" c.services.udev.extraRules;
assert c.services.getty.autologinUser == "root";
assert !c.services.openssh.enable && !c.services.openssh.openFirewall;
assert c.users.users.root.openssh.authorizedKeys.keys == [ ];
assert c.networking.firewall.allowedTCPPorts == [ ];
assert c.nix.settings.max-jobs == 0;
assert c.nix.settings.builders == "";
assert !c.nix.distributedBuilds;
assert !c.nix.settings.fallback;
assert c.nix.settings.require-sigs;
assert !(c.systemd.services ? korrid);
assert !c.system.tools.nixos-install.enable;
assert !c.services.xserver.enable;
assert !c.services.greetd.enable;
assert lib.elem "korrid" packageNames;
assert lib.all (name: !(lib.elem name packageNames)) [
  "android-tools"
  "flashrom"
  "rocknix-abl"
  "nixos-install"
];
assert c.sdImage.firmwarePartitionOffset == 8;
assert c.sdImage.firmwarePartitionName == "RPMINIV2";
assert c.sdImage.rootVolumeLabel == "NIXOS_RPMINIV2";
pkgs.runCommand "rpminiv2-module-check"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.util-linux
      pkgs.e2fsprogs
      pkgs.dosfstools
      pkgs.mtools
      pkgs.dtc
      pkgs.gptfdisk
    ];
  }
  ''
    cp ${./verify-image.py} verify-image.py
    cp ${./verify-image.test.py} verify-image.test.py
    python3 verify-image.test.py
    touch "$out"
  ''
