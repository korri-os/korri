# Assertions read the exported NixOS configuration. The CLI tests exercise
# actual GPT/FAT/ext4 files, not another model of the NixOS image.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  c = configuration.config;
  packageNames = map lib.getName c.environment.systemPackages;
  kernelConfigLines = lib.splitString "\n" (builtins.readFile ./kernel/config);
  hasKernelConfig = setting: builtins.elem setting kernelConfigLines;
  requiredKernelConfig = [
    "CONFIG_ARCH_QCOM=y"
    "CONFIG_AUTOFS_FS=y"
    "CONFIG_BLK_DEV_INITRD=y"
    "CONFIG_DEVTMPFS=y"
    "CONFIG_DRM_FBDEV_EMULATION=y"
    "CONFIG_DRM_MSM=y"
    "CONFIG_DRM_MSM_DPU=y"
    "CONFIG_DRM_MSM_DP=y"
    "CONFIG_DRM_MSM_DSI=y"
    "CONFIG_DRM_MSM_DSI_7NM_PHY=y"
    "CONFIG_DRM_MSM_KMS_FBDEV=y"
    "CONFIG_DRM_PANEL_DDIC_CH13726A=y"
    "CONFIG_EFI_STUB=y"
    "CONFIG_EXT4_FS=y"
    ''CONFIG_EXTRA_FIRMWARE="qcom/a650_gmu.bin qcom/a650_sqe.fw qcom/sm8250/a650_zap.mbn qcom/sm8250/adsp.mbn qcom/sm8250/cdsp.mbn qcom/sm8250/slpi.mbn regulatory.db regulatory.db.p7s"''
    ''CONFIG_EXTRA_FIRMWARE_DIR="external-firmware"''
    "CONFIG_FRAMEBUFFER_CONSOLE=y"
    "CONFIG_MMC_SDHCI_MSM=y"
    "CONFIG_MODULES=y"
    "CONFIG_QCOM_Q6V5_COMMON=y"
    "CONFIG_QCOM_Q6V5_PAS=y"
    "CONFIG_RD_GZIP=y"
    "CONFIG_REMOTEPROC=y"
    "CONFIG_SERIAL_MSM_CONSOLE=y"
    "CONFIG_USB_G_SERIAL=m"
    "CONFIG_USB_HID=y"
    "CONFIG_VFAT_FS=y"
  ];
  requiredRecoveryModules = [
    "CONFIG_USB_F_ACM=m"
    "CONFIG_USB_U_SERIAL=m"
    "CONFIG_USB_F_SERIAL=m"
    "CONFIG_USB_F_OBEX=m"
    "CONFIG_USB_G_SERIAL=m"
  ];
in
assert lib.all hasKernelConfig requiredKernelConfig;
assert lib.all hasKernelConfig requiredRecoveryModules;
assert c.nixpkgs.hostPlatform.system == "aarch64-linux";
assert c.networking.hostName == "rpminiv2";
assert c.hardware.deviceTree.name == "qcom/sm8250-retroidpocket-rpminiv2.dtb";
assert c.boot.kernelPackages.kernel.usesRocknixBootImage;
assert lib.hasInfix "rpminiv2-rocknix-20260901-baseline" (
  toString c.boot.kernelPackages.kernel.rocknixBaseline
);
assert c.hardware.deviceTree.overlays == [ ];
assert !c.hardware.enableAllHardware;
assert !c.hardware.graphics.enable;
assert !c.hardware.bluetooth.enable;
assert !(lib.elem "bluez" packageNames);
assert !(c.systemd.services ? bluetooth);
assert c.hardware.firmwareCompression == "none";
assert c.boot.consoleLogLevel == 4;
assert lib.filter (lib.hasPrefix "console=") c.boot.kernelParams == [ "console=tty0" ];
assert lib.elem "quiet" c.boot.kernelParams;
assert lib.elem "rootwait" c.boot.kernelParams;
assert lib.elem "consoleblank=60" c.boot.kernelParams;
assert lib.elem "gpt" c.boot.kernelParams;
assert c.boot.initrd.compressor == "gzip";
assert !c.boot.initrd.allowMissingModules;
assert !c.boot.initrd.includeDefaultModules;
assert lib.elem "qcom/sm8250/slpi.mbn" c.boot.initrd.extraFirmwarePaths;
assert lib.elem "regulatory.db.p7s" c.boot.initrd.extraFirmwarePaths;
assert !c.boot.loader.systemd-boot.enable;
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
assert lib.elem "libdrm" packageNames;
assert lib.all (name: !(lib.elem name packageNames)) [
  "alsa-utils"
  "android-tools"
  "dtc"
  "evtest"
  "flashrom"
  "korrid"
  "nixos-install"
  "pciutils"
  "rocknix-abl"
  "usbutils"
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
