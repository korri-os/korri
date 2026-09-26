# Assertions read both exported systems. The recovery configuration must remain
# the hardware-proven TTY baseline while the default inherits the shared product.
{
  pkgs,
  nixpkgs,
  korri,
  configuration,
  consoleConfiguration,
  dtDriverCheck,
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
    "CONFIG_HWMON=y"
    "CONFIG_SENSORS_PWM_FAN=m"
    "CONFIG_NEW_LEDS=y"
    "CONFIG_LEDS_CLASS=y"
    "CONFIG_LEDS_CLASS_MULTICOLOR=y"
    "CONFIG_LEDS_QCOM_LPG=y"
    "CONFIG_IIO=y"
    "CONFIG_QCOM_SPMI_ADC5=y"
    "CONFIG_QCOM_SPMI_ADC_TM5=y"
    "CONFIG_QCOM_SPMI_TEMP_ALARM=y"
    "CONFIG_TOUCHSCREEN_EDT_FT5X06=y"
    "CONFIG_LEDS_GROUP_MULTICOLOR=y"
    "CONFIG_NVMEM_SPMI_SDAM=m"
    "CONFIG_RTC_DRV_PM8XXX=y"
    "CONFIG_INPUT_PM8941_PWRKEY=y"
    "CONFIG_CGROUPS=y"
    "CONFIG_INPUT_EVDEV=y"
    "CONFIG_INPUT_JOYSTICK=y"
    "CONFIG_INPUT_MISC=y"
    "CONFIG_INPUT_UINPUT=y"
    "CONFIG_INPUT_QCOM_SPMI_HAPTICS=y"
    "CONFIG_IPV6=y"
    "CONFIG_NETFILTER=y"
    "CONFIG_NETFILTER_NETLINK=y"
    "CONFIG_NF_CONNTRACK=y"
    "CONFIG_NF_LOG_SYSLOG=y"
    "CONFIG_NF_TABLES=y"
    "CONFIG_NF_TABLES_IPV4=y"
    "CONFIG_NF_TABLES_IPV6=y"
    "CONFIG_NFT_CT=y"
    "CONFIG_NFT_LOG=y"
    "CONFIG_NFT_COMPAT=y"
    "CONFIG_NETFILTER_XTABLES=y"
    "CONFIG_NETFILTER_XT_TARGET_LOG=y"
    "CONFIG_NETFILTER_XT_MATCH_PKTTYPE=y"
    "CONFIG_IP_NF_IPTABLES=y"
    "CONFIG_IP6_NF_IPTABLES=y"
    "CONFIG_JOYSTICK_RETROID=m"
    "CONFIG_SOUND=y"
    "CONFIG_SND_SOC=y"
    "CONFIG_SND_SOC_SM8250=m"
    "CONFIG_SND_SOC_WCD938X_SDW=m"
    "CONFIG_SND_SOC_WSA881X=m"
    "CONFIG_SOUNDWIRE_QCOM=m"
    "CONFIG_REMOTEPROC=y"
    "CONFIG_QCOM_Q6V5_PAS=y"
    "CONFIG_PID_NS=y"
    "CONFIG_SECCOMP=y"
    "CONFIG_SECCOMP_FILTER=y"
    "CONFIG_USER_NS=y"
    # The composed USB gadget. The ACM function comes from usb_f_acm.ko,
    # which g_serial already requires (requiredRecoveryModules).
    # CONFIG_USB_CONFIGFS_ACM only selects that module, so it may stay unset.
    "CONFIG_CONFIGFS_FS=y"
    "CONFIG_USB_LIBCOMPOSITE=y"
    "CONFIG_USB_CONFIGFS=y"
    "CONFIG_USB_CONFIGFS_NCM=y"
    "CONFIG_USB_F_NCM=y"
    "CONFIG_USB_U_ETHER=y"
  ];
  common =
    config:
    config.nixpkgs.hostPlatform.system == "aarch64-linux"
    && config.networking.hostName == "rpminiv2"
    && config.hardware.deviceTree.name == "qcom/sm8250-retroidpocket-rpminiv2.dtb"
    && config.hardware.deviceTree.overlays == [ ]
    && !config.hardware.enableAllHardware
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
  thermalSnapshot = c.systemd.services.rpminiv2-thermal-snapshot;
  gadget = c.systemd.services.usb-gadget;
  gadgetNetwork = c.systemd.network.networks."10-usb-gadget";
  gadgetPackage = configuration.pkgs.callPackage ./usb-gadget-package.nix { };
  # Shell control-flow checks only: ordinary directories are NOT configfs.
  # usb-gadget-vm-test.nix checks kernel links, binding and service restarts.
  # One UDC wait second keeps these failure cases short.
  gadgetTestPackage = pkgs.callPackage ./usb-gadget-package.nix {
    configfsRoot = "$TMPDIR/configfs";
    udcRoot = "$TMPDIR/udc";
    udcWaitSeconds = 1;
  };
in
assert lib.all hasRecoveryKernelConfig requiredKernelConfig;
assert lib.all hasRecoveryKernelConfig requiredRecoveryModules;
assert lib.all hasProductKernelConfig (requiredKernelConfig ++ requiredRecoveryModules);
assert lib.all hasProductKernelConfig requiredProductKernelConfig;
# Sound is a product feature; the recovery kernel stays without it.
assert hasRecoveryKernelConfig "# CONFIG_SOUND is not set";
assert hasRecoveryKernelConfig "# CONFIG_REMOTEPROC is not set";
assert common c;
assert lib.hasInfix (builtins.unsafeDiscardStringContext (
  toString dtDriverCheck
)) c.sdImage.populateRootCommands;
assert
  !(lib.hasInfix (builtins.unsafeDiscardStringContext (toString dtDriverCheck)) recovery.sdImage.populateRootCommands);
assert common recovery;
# C3 is a Mini V2 product edge, never a recovery or unpatched-kiosk feature.
assert !((recovery.systemd.services.korrid.environment or { }) ? KORRID_PORTAL_UNIT);
assert !(recovery.systemd.services ? korri-chromium-kiosk-thaw);
assert !(lib.hasInfix "korri-chromium-kiosk" recovery.security.polkit.extraConfig);
assert
  (c.systemd.services.korrid.environment ? KORRID_PORTAL_UNIT)
  == builtins.any (patch: baseNameOf (toString patch) == "stop-thaws-unit.patch") (
    c.systemd.package.patches or [ ]
  );
assert !(c.systemd.services ? korri-chromium-kiosk-thaw);
assert c.services.korri.clockGovernor.enable;
assert c.services.korri.clockGovernor.cpuGovernor == "schedutil";
assert c.services.korri.clockGovernor.gpuDevfreqNodes == [ ];
assert c.services.korri.clockGovernor.cpuIdleDisable == [ ];
assert !(recovery.systemd.services ? korri-clock-governor);
assert c.hardware.bluetooth.enable;
assert c.systemd.services ? bluetooth;
assert c.systemd.timers ? rpminiv2-va-macro-retry;
assert c.systemd.timers.rpminiv2-va-macro-retry.timerConfig.OnBootSec == "20s";
assert
  c.systemd.services.rpminiv2-va-macro-retry.unitConfig.ConditionPathExists
  == "!/proc/asound/RetroidPocket";
assert !recovery.hardware.bluetooth.enable;
assert !(recovery.systemd.services ? bluetooth);
assert c.networking.networkmanager.enable;
assert c.hardware.wirelessRegulatoryDatabase;
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
assert
  korri.packages.aarch64-linux.rpminiv2-kernel.drvPath
  == korri.packages.x86_64-linux.rpminiv2-kernel.drvPath;
assert c.boot.kernelPackages.kernel.drvPath == korri.packages.x86_64-linux.rpminiv2-kernel.drvPath;
assert c.image.baseName == "nixos-rpminiv2-korri";
assert recovery.image.baseName == "nixos-rpminiv2";
assert
  lib.sort lib.lessThan c.boot.kernelModules == [
    "libcomposite"
    "retroid"
    "usb_f_acm"
    "usb_f_ncm"
  ];
assert !(lib.elem "retroid" recovery.boot.kernelModules);
# USB: recovery keeps the hardware-proven g_serial console and no network.
# The product replaces g_serial with the composed NCM plus ACM gadget; its
# ACM tty is still ttyGS0, so the shared udev getty rule serves both.
assert lib.elem "g_serial" recovery.boot.kernelModules;
assert !(lib.any (lib.hasPrefix "usb_f_") recovery.boot.kernelModules);
assert !(lib.elem "libcomposite" recovery.boot.kernelModules);
assert !(recovery.systemd.services ? usb-gadget);
assert !recovery.systemd.network.enable;
assert recovery.networking.firewall.interfaces == { };
assert !(lib.elem "usb0" recovery.networking.networkmanager.unmanaged);
assert !(lib.elem "g_serial" c.boot.kernelModules);
assert gadget.wantedBy == [ "multi-user.target" ];
assert lib.elem "sys-kernel-config.mount" gadget.requires;
assert lib.elem "sys-kernel-config.mount" gadget.after;
assert gadget.serviceConfig.ExecStart == lib.getExe gadgetPackage;
assert lib.hasInfix ''
  if [ -e /sys/kernel/config/usb_gadget/rpminiv2/UDC ] && [ -n "$(cat /sys/kernel/config/usb_gadget/rpminiv2/UDC)" ]; then
'' gadget.preStop;
assert gadgetNetwork.matchConfig.Name == "usb0";
assert gadgetNetwork.address == [ "10.42.3.1/24" ];
assert gadgetNetwork.networkConfig.DHCPServer;
assert gadgetNetwork.dhcpServerConfig.PoolOffset == 10;
assert gadgetNetwork.dhcpServerConfig.PoolSize == 20;
assert !gadgetNetwork.dhcpServerConfig.EmitDNS;
assert !gadgetNetwork.dhcpServerConfig.EmitRouter;
assert !gadgetNetwork.linkConfig.RequiredForOnline;
assert !c.systemd.network.wait-online.enable;
assert lib.elem "usb0" c.networking.networkmanager.unmanaged;
# The cable carries a link, not an open door: only the DHCP lease port.
assert c.networking.firewall.interfaces.usb0.allowedUDPPorts == [ 67 ];
assert (c.networking.firewall.interfaces.usb0.allowedTCPPorts or [ ]) == [ ];
assert (c.networking.firewall.interfaces.usb0.allowedTCPPortRanges or [ ]) == [ ];
assert (c.networking.firewall.interfaces.usb0.allowedUDPPortRanges or [ ]) == [ ];
assert c.hardware.graphics.enable;
assert c.services.seatd.enable;
assert !recovery.hardware.graphics.enable;
assert !recovery.services.seatd.enable;
assert !(recovery.systemd.services ? korrid);
assert !(recovery.systemd.services ? rpminiv2-thermal-snapshot);
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
assert lib.hasInfix ''input "0:0:generic_ft5x06_(8d)" map_to_output DSI-1''
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
assert c.services.korriLinuxInput.inputd.actions.controller-activity.command == [
  "${configuration.pkgs.sway}/bin/swaymsg"
  "-s"
  "/run/korri-compositor/sway-ipc.sock"
  "seat * idle_notify"
];
assert !(recovery.systemd.services ? rpminiv2-display-idle);
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
assert thermalSnapshot.wantedBy == [ "multi-user.target" ];
assert lib.elem "korri-compositor.service" thermalSnapshot.before;
assert lib.elem "korri-chromium-kiosk.service" thermalSnapshot.before;
assert thermalSnapshot.serviceConfig.ProtectKernelTunables;
assert thermalSnapshot.serviceConfig.TimeoutStartSec == "10s";
assert lib.elem "libdrm" (packageNames recovery);
# Sound: PipeWire and WirePlumber read ROCKNIX's RetroidPocket UCM, and the
# card's arrival runs its boot volumes once.
assert
  c.systemd.user.services.pipewire.environment.ALSA_CONFIG_UCM2
  == c.systemd.user.services.wireplumber.environment.ALSA_CONFIG_UCM2;
assert
  c.systemd.services.rpminiv2-audio-boot.environment.ALSA_CONFIG_UCM2
  == c.systemd.user.services.pipewire.environment.ALSA_CONFIG_UCM2;
assert lib.hasInfix ''ATTRS{id}=="RetroidPocket"'' c.services.udev.extraRules;
assert lib.hasInfix "rpminiv2-audio-boot.service" c.services.udev.extraRules;
assert lib.hasInfix "PULSE_SERVER" (builtins.readFile c.services.korridLinuxDevice.deviceConfig);
assert lib.hasSuffix "/bin/wpctl" (
  builtins.head c.services.korriLinuxInput.inputd.actions.volume-up.command
);
assert
  builtins.tail c.services.korriLinuxInput.inputd.actions.volume-up.command == [
    "set-volume"
    "@DEFAULT_AUDIO_SINK@"
    "5%+"
  ];
assert lib.hasSuffix "/bin/wpctl" (
  builtins.head c.services.korriLinuxInput.inputd.actions.volume-down.command
);
assert
  builtins.tail c.services.korriLinuxInput.inputd.actions.volume-down.command == [
    "set-volume"
    "@DEFAULT_AUDIO_SINK@"
    "5%-"
  ];
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
    ucm=${c.systemd.user.services.pipewire.environment.ALSA_CONFIG_UCM2}
    test "$(readlink -f "$ucm/conf.d/sm8250/retroidpocket-RetroidPocketMiniV2.conf")" = "$(readlink -f "$ucm/Qualcomm/sm8250/RetroidPocket.conf")"
    grep -F "SpkrLeft PA Volume' 12" "$ucm/Qualcomm/sm8250/RetroidPocket.conf"
    test -f ${c.services.inputplumber.package}/share/inputplumber/devices/01-retroid-pocket-mini-v2.yaml
    test -f ${c.services.inputplumber.package}/share/inputplumber/capability_maps/retroid_pocket_mini_v2.yaml
    test -f ${c.services.inputplumber.package}/share/inputplumber/capability_maps/retroid_pocket_mini_v2_volume.yaml
    bundle=${c.services.korriBundle.initialPackage}
    test -L "$bundle/share/inputplumber"
    test "$(readlink -f "$bundle/share/inputplumber")" = ${c.services.inputplumber.package}/share/inputplumber
    test -f "$bundle/share/inputplumber/devices/01-retroid-pocket-mini-v2.yaml"
    test -f "$bundle/share/inputplumber/capability_maps/retroid_pocket_mini_v2.yaml"
    test -f "$bundle/share/inputplumber/capability_maps/retroid_pocket_mini_v2_volume.yaml"
    test "$(readlink -f "$bundle/share/korri-input-profile")" = ${c.services.inputplumber.package}/share/inputplumber/profiles/korri-60-xbox_one_gamepad.yaml
    test "$(readlink -f "$bundle/bin/inputplumber")" = ${c.services.inputplumber.package}/bin/inputplumber
    cp ${./verify-image.py} verify-image.py
    cp ${./verify-image.test.py} verify-image.test.py
    export RP_MINIV2_KORRI_KERNEL=${c.system.build.kernel}/${c.system.boot.loader.kernelFile}
    python3 verify-image.test.py
    cp ${./kernel/fan-map.test.py} fan-map.test.py
    export RP_MINIV2_PRODUCT_DTB=${c.system.build.kernel}/dtbs/qcom/sm8250-retroidpocket-rpminiv2.dtb
    export RP_MINIV2_RECOVERY_DTB=${recovery.system.build.kernel}/dtbs/qcom/sm8250-retroidpocket-rpminiv2.dtb
    python3 fan-map.test.py
    cat ${dtDriverCheck}
    python3 ${./kernel}/next-image-config.test.py \
      --source-directory ${./kernel} \
      ${c.boot.kernelPackages.kernel.dev}/lib/modules/${c.boot.kernelPackages.kernel.modDirVersion}/build/.config
    cp ${./thermal-readonly.sh} thermal-readonly.sh
    cp ${./thermal-readonly.test.py} thermal-readonly.test.py
    python3 thermal-readonly.test.py

    # USB gadget shell checks with ordinary directories. These cannot prove
    # configfs semantics, function registration, UDC binding or USB traffic.
    export PATH=${gadgetTestPackage}/bin:$PATH
    gadget_dir="$TMPDIR/configfs/rpminiv2"
    mkdir -p "$gadget_dir"
    : > "$gadget_dir/UDC" # configfs supplies this attribute; plain dirs do not.
    # Missing class directory: wait, fail, and leave the gadget unbound.
    if rpminiv2-usb-gadget-configure > none.stdout 2> none.stderr; then
      echo "gadget bound without a USB device controller" >&2
      exit 1
    fi
    grep -F 'no USB device controller appeared' none.stderr
    test ! -s "$gadget_dir/UDC"
    # Existing but empty class directory follows the same timeout path.
    mkdir "$TMPDIR/udc"
    if rpminiv2-usb-gadget-configure > empty.stdout 2> empty.stderr; then
      echo "gadget bound with an empty controller class" >&2
      exit 1
    fi
    grep -F 'no USB device controller appeared' empty.stderr
    # The one controller the device tree enables.
    mkdir "$TMPDIR/udc/a600000.usb"
    rpminiv2-usb-gadget-configure
    test "$(cat "$gadget_dir/UDC")" = a600000.usb
    test "$(cat "$gadget_dir/functions/ncm.usb0/host_addr")" = 02:52:50:4d:32:01
    test "$(cat "$gadget_dir/functions/ncm.usb0/dev_addr")" = 02:52:50:4d:32:02
    test "$gadget_dir/configs/c.1/ncm.usb0" -ef "$gadget_dir/functions/ncm.usb0"
    test "$gadget_dir/configs/c.1/acm.usb0" -ef "$gadget_dir/functions/acm.usb0"
    # A second invocation must not nest links inside either function.
    rpminiv2-usb-gadget-configure
    test ! -e "$gadget_dir/functions/ncm.usb0/ncm.usb0"
    test ! -e "$gadget_dir/functions/acm.usb0/acm.usb0"
    # A restart after preStop's unbind binds the same controller again.
    : > "$gadget_dir/UDC"
    rpminiv2-usb-gadget-configure
    test "$(cat "$gadget_dir/UDC")" = a600000.usb
    # A second controller is ambiguous: refuse it and name both.
    : > "$gadget_dir/UDC"
    mkdir "$TMPDIR/udc/a800000.usb"
    if rpminiv2-usb-gadget-configure > two.stdout 2> two.stderr; then
      echo "gadget guessed between two USB device controllers" >&2
      exit 1
    fi
    grep -F 'found: a600000.usb a800000.usb' two.stderr
    test ! -s "$gadget_dir/UDC"
    rmdir "$TMPDIR/udc/a800000.usb"
    # A changed existing NCM address is refused before the bind.
    printf '%s\n' 02:00:00:00:00:01 > "$gadget_dir/functions/ncm.usb0/host_addr"
    if rpminiv2-usb-gadget-configure > mismatch.stdout 2> mismatch.stderr; then
      echo "mismatched existing NCM address unexpectedly succeeded" >&2
      exit 1
    fi
    grep -F 'expected 02:52:50:4d:32:01' mismatch.stderr
    test ! -s "$gadget_dir/UDC"
    touch "$out"
  ''
