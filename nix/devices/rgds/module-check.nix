# Check the exported NixOS configuration, not a second model of the image.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  c = configuration.config;
  usbConsoleCheck = import ./usb-console-check.nix {
    inherit pkgs;
    config = c;
  };
  # Build the gadget script here so its text can be read. The device build is
  # aarch64; this copy only has to prove the script's shape.
  gadgetScript = pkgs.callPackage ../../product/usb-gadget-package.nix { };
  gadgetText = builtins.readFile (lib.getExe gadgetScript);
  rawWrite = builtins.unsafeDiscardStringContext c.sdImage.postBuildCommands;
in
assert c.nixpkgs.hostPlatform.system == "aarch64-linux";
assert c.boot.kernelPackages.kernel.version == "7.2";
assert c.boot.kernelPackages.kernel.modDirVersion == "7.2.0";
assert c.hardware.deviceTree.name == "rockchip/rk3568-anbernic-rg-ds.dtb";
assert map (overlay: overlay.name) c.hardware.deviceTree.overlays == [ "rgds-rkvenc-mpp" ];
assert c.networking.networkmanager.wifi.powersave == false;
assert c.services.korri.clockGovernor.gpuDevfreqNodes == [ "fde60000.gpu" ];
assert c.hardware.bluetooth.enable;
assert lib.elem "bluetooth.target" c.systemd.services.bluetooth.wantedBy;
assert !c.hardware.enableAllHardware;
assert !c.boot.initrd.allowMissingModules;
assert !(lib.elem "pata_qdi" c.boot.initrd.availableKernelModules);
assert map lib.getName c.boot.extraModulePackages == [ "rk-mpp-service-rkvenc" ];
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
assert lib.elem "libcomposite" c.boot.kernelModules;
assert !(lib.elem "g_serial" c.boot.kernelModules);
assert c.systemd.network.networks."10-usb-gadget".matchConfig.Name == "usb0";
assert lib.elem "usb0" c.networking.networkmanager.unmanaged;
# The cable carries a link, not an open door. The lease the device hands out is
# the one thing that must get through, so port 67 is open and nothing else.
assert c.networking.firewall.interfaces.usb0.allowedUDPPorts == [ 67 ];
assert (c.networking.firewall.interfaces.usb0.allowedTCPPorts or [ ]) == [ ];
# The controller can register after the service starts. A silent exit here once
# took the console away with nothing in the journal, so the script must wait and
# must report the failure.
assert lib.hasInfix "no USB device controller appeared" gadgetText;
assert !(lib.hasInfix ''test -n "$udc"'' gadgetText);
assert !(c.systemd.units ? "serial-getty@ttyGS0.service");
# The removed device-owned runtime account must not return. The shared product
# check owns the current product account policy.
assert !(c.users.users ? gameplay);
assert !(c.users.groups ? games);
assert c.services.korriProduct.sleep.states == [ ];
assert c.services.logind.settings.Login.HandlePowerKey == "poweroff";
assert c.services.logind.settings.Login.HandleLidSwitch == "poweroff";
assert c.services.korriLinuxHost.label == "rgds";
assert c.services.korriLinuxHost.compositor.backend == "drm";
assert c.services.korriLinuxHost.compositor.localInput.enable;
assert
  c.services.korriLinuxHost.compositor.drmDevice
  == "/dev/dri/by-path/platform-display-subsystem-card";
assert c.services.korriLinuxHost.compositor.renderDevice == "/dev/dri/renderD128";
assert c.services.korriLinuxHost.compositor.mainDisplay == "DSI-2";
# Catalog startup powers only its main display; game/overlay placement owns
# waking other displays later. This is not an output-disable/removal rule.
assert lib.hasInfix "output DSI-1 power off"
  c.systemd.services.korri-compositor.environment.KORRI_SWAY_CONFIG.text;
assert
  !(lib.hasInfix "output DSI-2 power off" c.systemd.services.korri-compositor.environment.KORRI_SWAY_CONFIG.text);
assert (builtins.head c.services.korriLinuxHost.compositor.displays).mode == "640x480@60Hz";
assert
  map (d: {
    inherit (d)
      name
      x
      y
      touch
      ;
  }) c.services.korriLinuxHost.compositor.displays == [
    {
      name = "DSI-2";
      x = 0;
      y = 0;
      touch = "/i2c@fe5c0000/touchscreen@14";
    }
    {
      name = "DSI-1";
      x = 0;
      y = 480;
      touch = "/i2c@fe5e0000/touchscreen@14";
    }
  ];
assert
  lib.filter (line: lib.hasInfix "ENV{WL_OUTPUT}=" line) (
    lib.splitString "\n" c.services.udev.extraRules
  ) == [
    ''SUBSYSTEM=="input", KERNEL=="event*", ENV{ID_INPUT_TOUCHSCREEN}=="1", DEVPATH=="*/fe5c0000.i2c/i2c-*/*-0014/input/input*/event*", ENV{WL_OUTPUT}="DSI-2"''
    ''SUBSYSTEM=="input", KERNEL=="event*", ENV{ID_INPUT_TOUCHSCREEN}=="1", DEVPATH=="*/fe5e0000.i2c/i2c-*/*-0014/input/input*/event*", ENV{WL_OUTPUT}="DSI-1"''
  ];
assert c.services.korriLinuxHost.compositor.renderer == "gles2";
assert builtins.match "/dev/dri/card[0-9]+" c.services.korriLinuxHost.compositor.drmDevice == null;
assert c.services.korri.compositor.kiosk.extraChromiumArgs == [ "--disable-gpu" ];
assert
  map lib.getName c.services.korriLinuxInput.provider.extraDataPackages == [
    "rgds-inputplumber-data"
  ];
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
