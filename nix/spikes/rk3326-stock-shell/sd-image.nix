# A first-boot SD image for the R36T Max.
#
# The goal is one thing only: reach a shell on the serial console. No Korri
# services, no display, no splash. This deliberately does not import
# `nix/base`, because everything in there is another way for a first boot to
# fail before it says anything.
#
# Expect a black screen. The panel needs an ST7703 variant carrying this
# board's init sequence and that work has not started, so the only output is
# UART5.
#
# The card is the whole boot path. The handheld's boot ROM prefers SD over
# its internal eMMC, and the device tree leaves eMMC disabled, so the stock
# EmuELEC system and the SSH access installed on it stay untouched and remain
# the way back if this image does nothing at all.
{
  config,
  pkgs,
  lib,
  ...
}:

let
  # Rockchip's boot ROM looks for a loader at sector 64, and everything up to
  # the first partition is reserved for it. Identical to the RG353M: this is a
  # property of the boot ROM, not of the SoC generation.
  firmwarePartitionOffsetMiB = 16;
  ubootStartSector = 64;

  armTrustedFirmwarePX30 = pkgs.callPackage ../rk3326-boot-chain/atf-px30.nix { };
  uboot = pkgs.callPackage ../rk3326-boot-chain/uboot.nix {
    inherit armTrustedFirmwarePX30;
  };

  kernel = pkgs.callPackage ./dts/kernel-trimmed.nix { };
in
{
  imports = [
    (import ../../formats/sd-card.nix { gpt = false; })
    ./usb-gadget.nix
  ];

  nixpkgs.hostPlatform = "aarch64-linux";

  boot = {
    consoleLogLevel = 7;

    # The board device tree ships inside this kernel, so hardware.deviceTree
    # finds it in the kernel's own dtbs directory with no extra wiring.
    kernelPackages = pkgs.linuxPackagesFor kernel;

    # No initrd storage modules at all. ROCKNIX's configuration builds
    # MMC_BLOCK, MMC_DW, MMC_DW_ROCKCHIP, and EXT4_FS into the kernel, so the
    # root filesystem mounts with nothing loaded.
    #
    # The default list has to be forced empty, not just left alone. NixOS
    # fills availableKernelModules with PC storage controllers -- 3w-9xxx,
    # megaraid, and friends -- and modules-shrunk fails hard on the first one
    # a handheld kernel does not build:
    #   modprobe: FATAL: Module 3w-9xxx not found
    initrd.availableKernelModules = lib.mkForce [ ];
    initrd.kernelModules = lib.mkForce [ ];
    initrd.includeDefaultModules = false;

    kernelParams = [
      # UART5 at 1500000, taken from the stock system's own earlycon
      # (0xff178000). Most RK3326 boards use UART2; this family does not.
      "console=ttyS5,1500000n8"
      "earlycon=uart8250,mmio32,0xff178000"
      # Keep the framebuffer console too, in case the panel ever lights.
      "console=tty0"
    ];

    loader = {
      grub.enable = false;
      # No keyboard at a boot menu on a handheld, and nothing to choose.
      timeout = 0;
      generic-extlinux-compatible = {
        enable = true;
        configurationLimit = 3;
      };
    };
  };

  # The installer-wide hardware list pulls in PC drivers this kernel does not
  # build, which is what made modules-shrunk fail on 3w-9xxx. The RG DS turns
  # it off for the same reason.
  hardware.enableAllHardware = lib.mkForce false;

  hardware.deviceTree = {
    enable = true;
    filter = "rk3326-aislpc-r36t-max.dtb";
    name = "rockchip/rk3326-aislpc-r36t-max.dtb";
  };

  image.baseName = "nixos-r36t-max";

  sdImage = {
    compressImage = true;
    firmwarePartitionOffset = firmwarePartitionOffsetMiB;
    firmwarePartitionName = "NIXOS_BOOT";
    rootVolumeLabel = "NIXOS_R36TMAX";

    # U-Boot lives in the raw space before the first partition. The FAT
    # partition is unused but the shared NixOS image builder requires it.
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

  # systemd requires DMIID unconditionally, and this board cannot provide it:
  # DMI on arm64 comes from EFI or ACPI, and ROCKNIX's configuration has
  # neither (`# CONFIG_EFI is not set`). Writing CONFIG_DMIID=y anyway would
  # satisfy the assertion, which reads the file, while Kconfig dropped the
  # symbol for an unmet dependency -- a check that passes and a kernel that
  # does not have it.
  #
  # So the requirement is dropped rather than faked, and the other seventeen
  # are restated rather than disabled wholesale. Each one was checked present
  # in dts/config before this was written. The list mirrors
  # nixos/modules/system/boot/systemd.nix; if that grows an entry, this needs
  # the same entry.
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

  networking.hostName = "r36tmax";

  # Serial console only. Root login without a password is acceptable here and
  # nowhere else: this image has no network, exists to be watched over a wire,
  # and is replaced the moment it boots.
  users.users.root.initialHashedPassword = "";
  services.getty.autologinUser = "root";

  system.stateVersion = "25.05";
}
