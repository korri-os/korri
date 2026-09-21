{
  config,
  lib,
  pkgs,
  ...
}:
let
  uboot = pkgs.callPackage ./uboot.nix { };
  firmwarePartitionOffsetMiB = 16;
  # Allwinner BROM searches for SPL starting at sector 16 (8 KiB offset).
  ubootStartSector = 16;
  kernel = pkgs.callPackage ./kernel { };
in
{
  imports = [
    ../../base
    ../../device-cache/nixos-module.nix
    (import ../../formats/sd-card.nix { gpt = false; })
  ];

  nixpkgs.hostPlatform = "aarch64-linux";
  networking.hostName = "rg35xxsp";

  boot = {
    kernelPackages = pkgs.linuxPackagesFor kernel;
    consoleLogLevel = 7;
    kernelParams = [
      "console=ttyS0,115200n8"
      "console=tty0"
    ];
    initrd = {
      includeDefaultModules = false;
      availableKernelModules = lib.mkForce [ ];
      kernelModules = lib.mkForce [ ];
    };
    loader = {
      grub.enable = false;
      timeout = 3;
      generic-extlinux-compatible = {
        enable = true;
        configurationLimit = 3;
      };
    };
  };

  # systemd requires DMIID unconditionally, which arm64 without EFI/ACPI lacks.
  # Restate the required kernel config options without DMIID.
  system.requiredKernelConfig = lib.mkForce (
    map config.lib.kernelConfig.isEnabled [
      "DEVTMPFS"
      "CGROUPS"
      "INOTIFY_USER"
      "SIGNALFD"
      "TIMERFD"
      "EPOLL"
      "NET"
      "SYSFS"
      "PROC_FS"
      "FHANDLE"
      "CRYPTO_USER_API_HASH"
      "CRYPTO_HMAC"
      "CRYPTO_SHA256"
      "AUTOFS_FS"
      "TMPFS_POSIX_ACL"
      "TMPFS_XATTR"
      "SECCOMP"
    ]
  );

  hardware = {
    enableAllHardware = lib.mkForce false;
    deviceTree = {
      enable = true;
      filter = "sun50i-h700-anbernic-rg35xx-sp.dtb";
      name = "allwinner/sun50i-h700-anbernic-rg35xx-sp.dtb";
    };
    graphics.enable = true;
    enableRedistributableFirmware = true;
  };

  # Preserve the shared DRM-seat policy for the display bring-up image.
  # No compositor or controller mapping is selected before hardware probing.
  services.seatd.enable = true;
  security.polkit.enable = true;

  # Autologin root on serial and console for first-boot diagnostic verification.
  services.getty.autologinUser = "root";

  image.baseName = "nixos-rg35xxsp";
  sdImage = {
    compressImage = true;
    firmwarePartitionOffset = firmwarePartitionOffsetMiB;
    firmwarePartitionName = "NIXOS_BOOT";
    rootVolumeLabel = "NIXOS_RG35XXSP";
    populateFirmwareCommands = ":";
    populateRootCommands = ''
      mkdir -p ./files/boot
      ${config.boot.loader.generic-extlinux-compatible.populateCmd} \
        -c ${config.system.build.toplevel} \
        -d ./files/boot
    '';
    postBuildCommands = ''
      uboot_size="$(${pkgs.coreutils}/bin/stat -c %s ${uboot}/u-boot-sunxi-with-spl.bin)"
      boot_area_size="$((
        ${toString firmwarePartitionOffsetMiB} * 1024 * 1024
        - ${toString ubootStartSector} * 512
      ))"
      if [ "$uboot_size" -gt "$boot_area_size" ]; then
        echo "U-Boot exceeds the raw area before the first partition" >&2
        exit 1
      fi
      dd if=${uboot}/u-boot-sunxi-with-spl.bin of="$img" bs=512 \
        seek=${toString ubootStartSector} conv=notrunc
    '';
  };
}
