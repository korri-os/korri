{
  config,
  pkgs,
  ...
}:

let
  firmwarePartitionOffsetMiB = 16;
  uboot = pkgs.callPackage ./uboot.nix { };
  ubootStartSector = 64;
in
{
  imports = [
    ../../base
    (import ../../formats/sd-card.nix { gpt = false; })
    ../../../brand/plymouth/nixos-module.nix
    ./browser-bench.nix
    ./gpu.nix
    ./rk-mpp-service.nix
    ./sunshine-host.nix
    ./usb-gadget.nix
    ./wifi.nix
  ];

  nixpkgs.hostPlatform = "aarch64-linux";

  # The panel is 640x480 at 60 Hz (rg353v2_mode: 24150 kHz / (762 x 528)), so
  # the splash renders at 60 fps. `quiet` stays off until this device is seen
  # to boot with the splash: the panel is the only debugging channel it has.
  services.korri.bootSplash = {
    enable = true;
    refreshRate = 60;
    quiet = false;
  };

  boot = {
    consoleLogLevel = 7;
    # The stock kernel builds the RG353 panel path as modules. Load the DSI
    # PHY, DSI bridge, panel, and backlight in the initrd so the framebuffer
    # console appears before the root filesystem mounts.
    #
    # U-Boot reads the panel ID over DSI and rewrites the panel compatible
    # before Linux starts. This RG353M reports ID 0x3821, which U-Boot maps to
    # "anbernic,rg353v-panel-v2", a Sitronix ST7703 panel. Load both panel
    # drivers so either revision binds.
    #
    # Load panfrost here too. Left to udev coldplug it binds the Mali G52 at
    # about 19.5 s, but the compositor starts at about 16.9 s and exits when
    # /dev/dri/renderD128 is missing. systemd then waits out the compositor
    # start timeout and restarts the unit, which costs 18.5 s on every boot.
    # Binding the GPU in the initrd puts the render node in place long before
    # anything asks for it. Measured: with the node already present the
    # compositor starts in 2.65 s and never fails.
    initrd.kernelModules = [
      "mmc_block"
      "phy-rockchip-inno-dsidphy"
      "dw-mipi-dsi"
      "rockchipdrm"
      "panel-sitronix-st7703"
      "panel-newvision-nv3051d"
      "pwm_bl"
      "panfrost"
    ];
    # Start with the stock mainline kernel. It contains the RG353P device tree
    # and the RK3566 storage, display, RK817 audio, and RTL8821CS WiFi drivers.
    kernelPackages = pkgs.linuxPackages_latest;
    # The stock ST7703 driver draws nothing on the rg353v-panel-v2 glass. Ship
    # the corrected driver as an out-of-tree module in updates/, which depmod
    # prefers over the in-tree copy, instead of patching and rebuilding the
    # whole kernel.
    extraModulePackages = [
      (config.boot.kernelPackages.callPackage ./st7703-panel-module.nix { })
    ];
    kernelParams = [
      "console=ttyS2,1500000n8"
      "console=tty0"
    ];
    loader = {
      grub.enable = false;
      # The device boots one entry and has no keyboard at the menu. A timeout
      # of 0 makes the extlinux builder drop the menu and boot at once, which
      # measured 1.89 s off the pre-kernel phase.
      timeout = 0;
      generic-extlinux-compatible = {
        enable = true;
        configurationLimit = 3;
      };
    };
  };

  hardware = {
    # Upstream RGXX3 U-Boot detects the RG353M and selects the RG353P DTB.
    deviceTree = {
      enable = true;
      filter = "rk3566-anbernic-rg353p.dtb";
      name = "rockchip/rk3566-anbernic-rg353p.dtb";
      # The panel node names its supply "vdd" but the ST7703 driver asks for
      # "vcc" and "iovcc". Nothing claims vcc3v3_lcd0_n, so the regulator
      # core switches it off 30 s after boot and the panel goes dark. Keep it
      # on until a driver owns it.
      overlays = [
        {
          name = "rg353m-lcd-regulator-always-on";
          dtsText = ''
            /dts-v1/;
            /plugin/;
            / {
              compatible = "anbernic,rg353p";
            };
            &{/regulator-vcc3v3-lcd0} {
              regulator-always-on;
            };
          '';
        }
      ];
    };
    enableRedistributableFirmware = true;
  };

  image.baseName = "nixos-rg353m";

  sdImage = {
    compressImage = true;
    firmwarePartitionOffset = firmwarePartitionOffsetMiB;
    firmwarePartitionName = "NIXOS_BOOT";
    rootVolumeLabel = "NIXOS_RG353M";

    # U-Boot lives in the raw space before the first partition. The otherwise
    # unused FAT partition remains because the shared NixOS SD image builder
    # requires it.
    populateFirmwareCommands = ":";
    populateRootCommands = ''
      mkdir -p ./files/boot
      ${config.boot.loader.generic-extlinux-compatible.populateCmd} \
        -c ${config.system.build.toplevel} \
        -d ./files/boot
    '';
    postBuildCommands = ''
      uboot_size="$(${pkgs.coreutils}/bin/stat -c %s ${uboot}/u-boot-rockchip.bin)"
      boot_area_size="$((
        ${toString firmwarePartitionOffsetMiB} * 1024 * 1024
        - ${toString ubootStartSector} * 512
      ))"
      if [ "$uboot_size" -gt "$boot_area_size" ]; then
        echo "U-Boot exceeds the raw area before the first partition" >&2
        exit 1
      fi
      dd \
        if=${uboot}/u-boot-rockchip.bin \
        of="$img" \
        bs=512 \
        seek=${toString ubootStartSector} \
        conv=notrunc
    '';
  };

  networking.hostName = "haku";
  environment.systemPackages = [ pkgs.networkmanager ];
}
