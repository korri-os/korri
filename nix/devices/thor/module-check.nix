# Focused evaluation of the exported AYN Thor configuration.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  c = configuration.config;
  host = c.services.korriLinuxHost.compositor;
  shell = c.systemd.services.korri-shell;
  compositor = c.systemd.services.korri-compositor;
  credential = "KORRID_RPC_CAPABILITY:/run/korri-shell-credentials/KORRID_RPC_CAPABILITY";
  inputDataNames = map lib.getName c.services.korriLinuxInput.provider.extraDataPackages;
  # The board package is the first definition (mkBefore), ahead of linux-firmware.
  boardFirmware = builtins.head configuration.options.hardware.firmware.definitions;
  firmwarePaths = lib.concatMap (p: p.passthru.firmwarePaths or [ ]) boardFirmware;
  sway = compositor.environment.KORRI_SWAY_CONFIG.text;
in
# Boot: the Portal kernel, the Thor DTB, the Portal's systemd-boot ESP.
assert c.nixpkgs.hostPlatform.system == "aarch64-linux";
assert c.networking.hostName == "thor";
assert c.boot.kernelPackages.kernel.version == "7.2";
assert c.boot.kernelPackages.kernel.features.efiBootStub;
# The Thor kernel carries ROCKNIX's fixed DPU resource-cleanup series.
assert
  c.boot.kernelPackages.kernel.dpuResourceCleanupPatch == "0000-version-0010-msm-resource-cleanup.patch";
assert lib.any (
  p: baseNameOf p == "0000-version-0010-msm-resource-cleanup.patch"
) c.boot.kernelPackages.kernel.patches;
assert c.hardware.deviceTree.enable;
assert c.hardware.deviceTree.name == "qcom/qcs8550-ayn-thor.dtb";
assert lib.hasInfix "thor-dtb" (toString c.hardware.deviceTree.package);
assert c.specialisation == { };
assert c.boot.loader.systemd-boot.enable;
assert c.boot.loader.systemd-boot.graceful;
assert !c.boot.loader.grub.enable;
assert !c.boot.loader.efi.canTouchEfiVariables;
assert c.boot.initrd.compressor == "gzip";
assert !c.boot.initrd.includeDefaultModules;
assert c.hardware.firmwareCompression == "none";
assert builtins.elem "qcom/sm8550/a740_zap.mbn" c.boot.initrd.extraFirmwarePaths;
assert builtins.elem "console=tty0" c.boot.kernelParams;
assert builtins.elem "allow_mismatched_32bit_el0" c.boot.kernelParams;

# Removable media only, with labels that cannot match the Portal's internal
# install or ROCKNIX's.
assert c.sdImage.firmwarePartitionName == "THOR_ESP";
assert c.sdImage.rootVolumeLabel == "THOR_ROOT";
assert c.fileSystems."/".device == "/dev/disk/by-label/THOR_ROOT";
assert c.fileSystems."/boot".device == "/dev/disk/by-label/THOR_ESP";
assert lib.hasInfix "--change-name=1:THOR_ESP" c.sdImage.postBuildCommands;
assert lib.hasInfix "--change-name=2:THOR_ROOT" c.sdImage.postBuildCommands;
assert lib.hasInfix "qcs8550-ayn-thor.dtb" c.sdImage.populateFirmwareCommands;
assert !lib.hasInfix "odin2portal.dtb" c.sdImage.populateFirmwareCommands;
assert
  lib.all
    (
      label:
      !lib.elem label [
        "ODIN2P_ESP"
        "ODIN2P_ROOT"
        "STORAGE"
        "ROCKNIX"
      ]
    )
    [
      c.sdImage.firmwarePartitionName
      c.sdImage.rootVolumeLabel
    ];

# Firmware: the Thor's own ADSP and amp blobs, none of the Portal's.
assert map (p: p.name) boardFirmware == [ "thor-firmware" ];
assert builtins.elem "qcom/sm8550/ayn/thor/adsp.mbn" firmwarePaths;
assert builtins.elem "qcom/sm8550/ayn/thor/aw883xx_acf.bin" firmwarePaths;
assert !lib.any (lib.hasInfix "odin2portal") firmwarePaths;

# Top screen only; the bottom screen is dark and never a Korri display.
assert host.backend == "drm";
assert host.mainDisplay == "DSI-2";
assert map (d: d.name) host.displays == [ "DSI-2" ];
assert (builtins.head host.displays).mode == "1080x1920@120Hz";
assert host.localInput.enable;
assert lib.hasInfix "output DSI-2 transform 90" sway;
assert lib.hasInfix "output DSI-1 power off" sway;
assert !lib.hasInfix "output DSI-2 power off" sway;
assert !lib.hasInfix "DSI-2 disable" sway;
# Display idle stays the product's (README: first-boot test for the DSI-2 wedge).
assert c.systemd.services.korri-display-idle.enable;
assert lib.hasInfix ''ATTRS{name}=="bottom_touchscreen", ENV{LIBINPUT_IGNORE_DEVICE}="1"'' c.services.udev.extraRules;
assert builtins.elem "inputplumber-ayn-thor-data" inputDataNames;
assert !builtins.elem "inputplumber-ayn-sm8550-data" inputDataNames;

# SM8550 policy shared with the Portal.
assert c.services.korri.clockGovernor.cpuIdleDisable == [ "cpu0/cpuidle/state1" ];
assert c.services.korri.fanControl.hwmonName == "pwmfan";
assert c.services.korriProduct.sleep.states == [ ];
assert !c.systemd.targets.suspend.enable;

# The shared product: korrid, the shell as its own user, SSH by plugin.
assert c.services.korriProduct.installed;
assert c.services.korri.shell.enable;
assert c.services.korri.pluginHost.enable;
assert shell.serviceConfig.User == "korri-shell";
assert shell.serviceConfig.LoadCredential == [ credential ];
assert !(shell.environment ? KORRID_RPC_CAPABILITY);
assert lib.elem "korrid.service" shell.requires;
assert c.services.korriProduct.usbGadget.address == "10.42.8.1";

pkgs.runCommand "thor-module-check" { } ''
  touch "$out"
''
