{
  config,
  lib,
  pkgs,
  korri,
  ...
}:
let
  uboot = pkgs.callPackage ./uboot.nix { };
  firmwarePartitionOffsetMiB = 16;
  ubootStartSector = 64;
in
{
  imports = [
    ../../base
    ../../device-cache/nixos-module.nix
    (import ../../formats/sd-card.nix { gpt = false; })
  ];
  nixpkgs.hostPlatform = "aarch64-linux";
  networking.hostName = "rgds";

  boot = {
    kernelPackages = pkgs.linuxPackagesFor (pkgs.callPackage ./kernel.nix { });
    consoleLogLevel = 7;
    kernelParams = [
      "console=ttyS2,1500000n8"
      "console=tty0"
    ];
    # Linux v7.2 rk3568-anbernic-rg-ds.dts wires two Jadard DSI panels,
    # a Panfrost GPU, and two Goodix touch controllers. No RG353 panel fixes.
    initrd.kernelModules = [
      "mmc_block"
      "phy-rockchip-inno-dsidphy"
      "dw-mipi-dsi"
      "rockchipdrm"
      "panel-jadard-jd9365da-h3"
      "pwm_bl"
      "panfrost"
    ];
    # The board DTS selects peripheral mode on usb_host0_xhci. ACM serial is
    # independent of WiFi and needs no guessed Ethernet address or input map.
    kernelModules = [
      "g_serial"
      "goodix_ts"
    ];
    loader = {
      grub.enable = false;
      timeout = 3;
      generic-extlinux-compatible = {
        enable = true;
        configurationLimit = 3;
      };
    };
  };
  systemd.services."serial-getty@ttyGS0".wantedBy = [ "getty.target" ];

  hardware = {
    # The installer-wide list includes PC drivers removed from Linux 7.2
    # (pata_qdi first failed the image build). This board has explicit early
    # display modules above; retain normal NixOS defaults and strict checking.
    enableAllHardware = lib.mkForce false;
    deviceTree = {
      enable = true;
      filter = "rk3568-anbernic-rg-ds.dtb";
      name = "rockchip/rk3568-anbernic-rg-ds.dtb";
    };
    graphics.enable = true;
    enableRedistributableFirmware = true;
  };
  # Preserve the shared DRM-seat policy for the later device-backed kiosk.
  # No compositor or controller mapping is selected before connector probing.
  services.seatd.enable = true;
  security.polkit.enable = true;
  environment.systemPackages = [
    korri.packages.aarch64-linux.korrid
    pkgs.networkmanager
    pkgs.evtest
    pkgs.libdrm
    pkgs.usbutils
  ];

  # The existing RG353M format reserves 16 MiB before partitions and loads
  # extlinux from ext4. Upstream RK3568 boot targets scan mmc1 before mmc0.
  # These commands write only the builder's image file, never a block device.
  image.baseName = "nixos-rgds";
  sdImage = {
    compressImage = true;
    firmwarePartitionOffset = firmwarePartitionOffsetMiB;
    firmwarePartitionName = "NIXOS_BOOT";
    rootVolumeLabel = "NIXOS_RGDS";
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
      dd if=${uboot}/u-boot-rockchip.bin of="$img" bs=512 \
        seek=${toString ubootStartSector} conv=notrunc
      ${pkgs.python3}/bin/python3 ${./verify-image.py} \
        "$img" ${uboot}/u-boot-rockchip.bin
    '';
  };
}
