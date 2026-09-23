# Assertions read both exported systems. The recovery configuration must remain
# the hardware-proven TTY baseline while the default inherits the shared product.
{
  pkgs,
  nixpkgs,
  korri,
  configuration,
  consoleConfiguration,
}:
let
  inherit (pkgs) lib;
  c = configuration.config;
  recovery = consoleConfiguration.config;
  packageNames = config: map lib.getName config.environment.systemPackages;
  productKernelConfigLines = lib.splitString "\n" (builtins.readFile ./kernel/config-korri);
  recoveryKernelConfigLines = lib.splitString "\n" (builtins.readFile ./kernel/config-tty-trim);
  hasProductKernelConfig = setting: builtins.elem setting productKernelConfigLines;
  hasRecoveryKernelConfig = setting: builtins.elem setting recoveryKernelConfigLines;
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
    "CONFIG_FRAMEBUFFER_CONSOLE=y"
    "CONFIG_MMC_SDHCI_MSM=y"
    "CONFIG_MODULES=y"
    "CONFIG_RD_GZIP=y"
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
  requiredProductKernelConfig = [
    "CONFIG_CGROUPS=y"
    "CONFIG_INPUT_EVDEV=y"
    "CONFIG_INPUT_JOYSTICK=y"
    "CONFIG_INPUT_MISC=y"
    "CONFIG_INPUT_UINPUT=y"
    "CONFIG_INPUT_QCOM_SPMI_HAPTICS=y"
    "CONFIG_JOYSTICK_RETROID=m"
    "CONFIG_PID_NS=y"
    "CONFIG_SECCOMP=y"
    "CONFIG_SECCOMP_FILTER=y"
    "CONFIG_USER_NS=y"
  ];
  common =
    config:
    config.nixpkgs.hostPlatform.system == "aarch64-linux"
    && config.networking.hostName == "rpminiv2"
    && config.hardware.deviceTree.name == "qcom/sm8250-retroidpocket-rpminiv2.dtb"
    && config.hardware.deviceTree.overlays == [ ]
    && !config.hardware.enableAllHardware
    && !config.hardware.bluetooth.enable
    && !(lib.elem "bluez" (packageNames config))
    && !(config.systemd.services ? bluetooth)
    && config.hardware.firmwareCompression == "none"
    && config.boot.consoleLogLevel == 4
    && lib.filter (lib.hasPrefix "console=") config.boot.kernelParams == [ "console=tty0" ]
    && lib.elem "quiet" config.boot.kernelParams
    && lib.elem "rootwait" config.boot.kernelParams
    && lib.elem "consoleblank=60" config.boot.kernelParams
    && lib.elem "gpt" config.boot.kernelParams
    && config.boot.initrd.compressor == "gzip"
    && !config.boot.initrd.allowMissingModules
    && !config.boot.initrd.includeDefaultModules
    && !(lib.elem "g_serial" config.boot.initrd.availableKernelModules)
    && lib.elem "qcom/a650_gmu.bin" config.boot.initrd.extraFirmwarePaths
    && lib.elem "qcom/a650_sqe.fw" config.boot.initrd.extraFirmwarePaths
    && lib.elem "qcom/sm8250/a650_zap.mbn" config.boot.initrd.extraFirmwarePaths
    && lib.elem "qcom/sm8250/slpi.mbn" config.boot.initrd.extraFirmwarePaths
    && lib.elem "regulatory.db.p7s" config.boot.initrd.extraFirmwarePaths
    && !config.boot.loader.systemd-boot.enable
    && !config.boot.loader.grub.enable
    && !config.boot.loader.generic-extlinux-compatible.enable
    && !config.boot.loader.efi.canTouchEfiVariables
    && config.boot.loader.timeout == 0
    && config.fileSystems."/".device == "/dev/disk/by-label/NIXOS_RPMINIV2"
    && config.fileSystems."/boot".device == "/dev/disk/by-label/RPMINIV2"
    && lib.all (fs: !(lib.hasPrefix "/dev/mmcblk" fs.device) && !(lib.hasPrefix "/dev/sd" fs.device)) (
      lib.attrValues config.fileSystems
    )
    && config.swapDevices == [ ]
    && lib.elem "systemd.gpt_auto=0" config.boot.kernelParams
    && lib.elem "rd.systemd.gpt_auto=0" config.boot.kernelParams
    && lib.elem "g_serial" config.boot.kernelModules
    && !(config.systemd.units ? "serial-getty@ttyGS0.service")
    && lib.hasInfix "serial-getty@ttyGS0.service" config.services.udev.extraRules
    && !config.system.tools.nixos-install.enable
    && config.sdImage.firmwarePartitionOffset == 8
    && config.sdImage.firmwarePartitionName == "RPMINIV2"
    && config.sdImage.rootVolumeLabel == "NIXOS_RPMINIV2";
  productModule = import ../../product/nixos-module.nix { inherit korri; };
  productCheck = import ../../product/check-lib.nix {
    inherit korri;
    inherit (pkgs) lib;
    defaultSystem = pkgs.stdenv.hostPlatform.system;
    referenceForSystem =
      system:
      import ../../product/reference.nix {
        inherit
          nixpkgs
          korri
          productModule
          system
          ;
      };
  };
  productFailures = productCheck.validate "rpminiv2" configuration;
  idle = c.systemd.services.rpminiv2-display-idle;
in
assert lib.all hasRecoveryKernelConfig requiredKernelConfig;
assert lib.all hasRecoveryKernelConfig requiredRecoveryModules;
assert lib.all hasProductKernelConfig (requiredKernelConfig ++ requiredRecoveryModules);
assert lib.all hasProductKernelConfig requiredProductKernelConfig;
assert common c;
assert common recovery;
# Use the shared product validator instead of duplicating its policy here.
assert productFailures == [ ];
assert lib.all (a: a.assertion) c.assertions;
assert lib.all (a: a.assertion) recovery.assertions;
assert lib.hasSuffix "/config-korri" (toString c.boot.kernelPackages.kernel.kernelConfig);
assert lib.hasSuffix "/config-tty-trim" (toString recovery.boot.kernelPackages.kernel.kernelConfig);
assert lib.versionAtLeast c.boot.kernelPackages.kernel.compilerVersion "15";
assert lib.versionOlder c.boot.kernelPackages.kernel.compilerVersion "16";
assert lib.versionAtLeast recovery.boot.kernelPackages.kernel.compilerVersion "15";
assert lib.versionOlder recovery.boot.kernelPackages.kernel.compilerVersion "16";
assert c.boot.kernelPackages.kernel.drvPath != recovery.boot.kernelPackages.kernel.drvPath;
assert c.image.baseName == "nixos-rpminiv2-korri";
assert recovery.image.baseName == "nixos-rpminiv2";
assert
  c.boot.kernelModules == [
    "g_serial"
    "retroid"
  ];
assert !(lib.elem "retroid" recovery.boot.kernelModules);
assert c.hardware.graphics.enable;
assert c.services.seatd.enable;
assert !recovery.hardware.graphics.enable;
assert !recovery.services.seatd.enable;
assert !(recovery.systemd.services ? korrid);
assert !(recovery.systemd.services ? korri-compositor);
assert !(recovery.systemd.services ? korri-chromium-kiosk);
assert !(c.users.users ? gameplay);
assert !(c.users.groups ? games);
assert c.services.korriProduct.sleep.states == [ ];
assert c.services.logind.settings.Login.HandlePowerKey == "poweroff";
assert c.services.logind.settings.Login.HandleLidSwitch == "poweroff";
assert c.services.korriLinuxHost.compositor.localInput.enable;
assert c.services.korriLinuxHost.compositor.backend == "drm";
assert c.services.korriLinuxHost.compositor.drmDevice == "/dev/dri/card0";
assert c.services.korriLinuxHost.compositor.renderDevice == "/dev/dri/renderD128";
assert c.services.korriLinuxHost.compositor.outputName == "DSI-1";
assert c.services.korriLinuxHost.compositor.mode == "1080x1240@60Hz";
assert c.services.korriLinuxHost.compositor.renderer == "gles2";
assert lib.hasInfix "output DSI-1 transform 90 scale 1"
  c.services.korriLinuxHost.compositor.extraConfig;
assert c.services.korri.compositor.kiosk.extraChromiumArgs == [ "--disable-gpu" ];
assert
  map lib.getName c.services.korriLinuxInput.provider.extraDataPackages == [
    "rpminiv2-inputplumber-data"
  ];
assert
  c.systemd.services.korri-bundle-selector.environment.KORRI_BUNDLE_INITIAL_PACKAGE
  == toString c.services.korriBundle.initialPackage;
assert lib.hasSuffix " initialize \${KORRI_BUNDLE_INITIAL_PACKAGE}"
  c.systemd.services.korri-bundle-selector.serviceConfig.ExecStart;
assert !(c.systemd.services ? sunshine);
assert !(c.systemd.sockets ? korri-certificate-control);
assert idle.serviceConfig.User == c.services.korriLinuxHost.runtimeUser;
assert idle.serviceConfig.Group == c.services.korriLinuxHost.runtimeGroup;
assert
  idle.environment.XDG_RUNTIME_DIR == "/run/user/${toString c.services.korriLinuxHost.runtimeUid}";
assert idle.serviceConfig.NoNewPrivileges;
assert idle.serviceConfig.ProtectSystem == "strict";
assert idle.environment.WAYLAND_DISPLAY == "korri-wayland";
assert idle.environment.SWAYSOCK == "/run/korri-compositor/sway-ipc.sock";
assert lib.elem "korri-compositor.service" idle.requires;
assert lib.elem "korri-compositor.service" idle.after;
assert lib.elem "libdrm" (packageNames recovery);
assert lib.all (name: !(lib.elem name (packageNames recovery))) [
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
assert lib.hasInfix (builtins.unsafeDiscardStringContext (
  toString c.boot.kernelPackages.kernel
)) c.sdImage.populateFirmwareCommands;
assert lib.hasInfix (builtins.unsafeDiscardStringContext (
  toString recovery.boot.kernelPackages.kernel
)) recovery.sdImage.populateFirmwareCommands;
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
    grep -F 'timeout 300' ${idle.serviceConfig.ExecStart}
    test -f ${c.services.inputplumber.package}/share/inputplumber/devices/01-retroid-pocket-mini-v2.yaml
    test -f ${c.services.inputplumber.package}/share/inputplumber/capability_maps/retroid_pocket_mini_v2.yaml
    bundle=${c.services.korriBundle.initialPackage}
    test -L "$bundle/share/inputplumber"
    test "$(readlink -f "$bundle/share/inputplumber")" = ${c.services.inputplumber.package}/share/inputplumber
    test -f "$bundle/share/inputplumber/devices/01-retroid-pocket-mini-v2.yaml"
    test -f "$bundle/share/inputplumber/capability_maps/retroid_pocket_mini_v2.yaml"
    test "$(readlink -f "$bundle/share/korri-input-profile")" = ${c.services.inputplumber.package}/share/inputplumber/profiles/korri-60-xbox_one_gamepad.yaml
    test "$(readlink -f "$bundle/bin/inputplumber")" = ${c.services.inputplumber.package}/bin/inputplumber
    cp ${./verify-image.py} verify-image.py
    cp ${./verify-image.test.py} verify-image.test.py
    export RP_MINIV2_KORRI_KERNEL=${c.system.build.kernel}/${c.system.boot.loader.kernelFile}
    python3 verify-image.test.py
    touch "$out"
  ''
