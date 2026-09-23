{
  config,
  lib,
  pkgs,
  ...
}:
let
  uboot = pkgs.callPackage ./uboot.nix { };
  firmwarePartitionOffsetMiB = 16;
  ubootStartSector = 64;
in
{
  imports = [
    (import ../../formats/sd-card.nix { gpt = false; })
    ./usb-gadget.nix
  ];
  nixpkgs.hostPlatform = "aarch64-linux";
  # No suspend or light-sleep path has been verified on this board.
  services.korriProduct.sleep.states = [ ];
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
    # usb-gadget.nix composes the console and the network link through
    # configfs, so no legacy single-function gadget module is loaded here.
    loader = {
      grub.enable = false;
      timeout = 3;
      generic-extlinux-compatible = {
        enable = true;
        configurationLimit = 3;
      };
    };
  };
  # Start the USB login when the gadget tty appears, using systemd's own
  # device-driven mechanism. Declaring the instance in NixOS instead produced
  # a unit that ran before /dev/ttyGS0 existed and never restarted, and any
  # instance drop-in would discard the packaged agetty and root autologin.
  services.udev.extraRules = ''
    SUBSYSTEM=="tty", KERNEL=="ttyGS0", TAG+="systemd", ENV{SYSTEMD_WANTS}+="serial-getty@ttyGS0.service"
  '';

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
  environment.systemPackages = [
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
