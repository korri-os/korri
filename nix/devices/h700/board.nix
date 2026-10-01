# Hardware facts shared by the Anbernic H700 handhelds (RG35XX SP and Pro).
# Each device module adds its device tree, identity and SD labels.
#
# Nothing here has been observed on a booted board yet. Each value names the
# source it is read from. Physical acceptance must confirm or replace it.
{
  config,
  korri,
  lib,
  pkgs,
  h700Kernel,
  h700Uboot,
  ...
}:
let
  system = pkgs.stdenv.hostPlatform.system;
  firmwarePartitionOffsetMiB = 16;
  # Allwinner BROM searches for SPL starting at sector 16 (8 KiB offset).
  ubootStartSector = 16;
in
{
  imports = [
    (import ../../formats/sd-card.nix { gpt = false; })
  ];

  nixpkgs.hostPlatform = "aarch64-linux";
  # No suspend or light-sleep path has been verified on these boards.
  services.korriProduct.sleep.states = [ ];

  # Both panel init files (kernel/firmware/panels) describe a 640x480 panel at
  # 60 Hz (24 MHz over 800x500) or 59.52 Hz (v2 panel, 24 MHz over 720x560).
  services.korri.bootSplash.refreshRate = 60;

  boot = {
    # Cross-built off-device (default.nix); never compiled on the handheld.
    kernelPackages = pkgs.linuxPackagesFor h700Kernel;
    kernelParams = [
      "console=ttyS0,115200n8"
      "console=tty0"
    ];
    # The ROCKNIX configuration builds MMC, ext4, vfat, the display and the
    # GPU into the kernel, so the initrd loads no modules.
    initrd = {
      includeDefaultModules = false;
      availableKernelModules = lib.mkForce [ ];
      kernelModules = lib.mkForce [ ];
    };
    loader = {
      grub.enable = false;
      # The board has no keyboard at the boot menu. A timeout of 0 makes the
      # extlinux builder boot the default entry at once, as on the RG353M.
      timeout = 0;
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
    # The installer-wide list includes PC drivers absent from this kernel.
    enableAllHardware = lib.mkForce false;
    deviceTree.enable = true;
    graphics.enable = true;
    # The kernel embeds the only firmware these boards load: RTL8821CS Wi-Fi
    # and Bluetooth and the panel init files (CONFIG_EXTRA_FIRMWARE). The Mali
    # G31 needs none. Ship the regulatory database for cfg80211.
    enableRedistributableFirmware = lib.mkForce false;
    wirelessRegulatoryDatabase = true;
    # RTL8821CS UART Bluetooth binds through serdev from the board tree.
    bluetooth.enable = true;
  };

  # The controller profile for both boards. The product module owns
  # InputPlumber and inputd themselves.
  services.korriLinuxInput.provider.extraDataPackages = [
    korri.packages.${system}.h700-inputplumber-data
  ];

  services.korriLinuxHost.compositor = {
    backend = "drm";
    localInput.enable = true;
    # sun4i-drm binds the top-level display-engine node (sun50i-h616.dtsi);
    # Panfrost binds gpu@1800000. Name both by platform path, because card
    # and render numbers follow probe order. Derived from the tree, not an
    # observed link.
    drmDevice = "/dev/dri/by-path/platform-display-engine-card";
    renderDevice = "/dev/dri/by-path/platform-1800000.gpu-render";
    # Kernel patch 0218 registers the RGB panel connector as DSI.
    outputName = "DSI-1";
    mode = "640x480@60Hz";
    renderer = "gles2";
  };

  # Recorded limit: Chromium's GPU path has not been accepted on these boards.
  services.korri.compositor.kiosk.extraChromiumArgs = [ "--disable-gpu" ];

  # The same RTL8821CS as the RG353M, where deep power saving caused pairing
  # failures and dropped links.
  networking.networkmanager.wifi.powersave = false;

  # The codec starts with "DAC Playback Switch" off. With it off, DAPM finds
  # no route from the DAC, the playback DMA never advances, and a game that
  # syncs to audio stops after its first frame (seen on the RG35XX Pro,
  # 2026-10-01). ROCKNIX turns the switch on in a FixedBootSequence that only
  # its alsa-ucm-conf patch adds (H700/0002_Add-Allwinner-H616-configuration);
  # the upstream alsa-ucm-conf H616 profile has no boot sequence. ROCKNIX's
  # H700 sleep hooks toggle the same switch. The card is "H616 Audio Codec"
  # in sun4i-codec.c, so ALSA gives it the id "Codec" (the last word of the
  # short name).
  services.udev.extraRules = ''
    SUBSYSTEM=="sound", KERNEL=="controlC*", ATTRS{id}=="Codec", TAG+="systemd", ENV{SYSTEMD_WANTS}+="h700-audio-boot.service"
  '';
  systemd.services.h700-audio-boot = {
    description = "Anbernic H700 codec DAC route";
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "${pkgs.alsa-utils}/bin/amixer -c Codec -q cset name='DAC Playback Switch' on,on";
      TimeoutStartSec = "20s";
    };
  };

  # Read-only tools for recording the display and input facts on first boot.
  environment.systemPackages = [
    pkgs.evtest
    pkgs.libdrm
  ];

  sdImage = {
    compressImage = true;
    firmwarePartitionOffset = firmwarePartitionOffsetMiB;
    firmwarePartitionName = "NIXOS_BOOT";
    # U-Boot lives in the raw space before the first partition. The otherwise
    # unused FAT partition remains because the NixOS SD image builder needs it.
    populateFirmwareCommands = ":";
    populateRootCommands = ''
      mkdir -p ./files/boot
      ${config.boot.loader.generic-extlinux-compatible.populateCmd} \
        -c ${config.system.build.toplevel} \
        -d ./files/boot
    '';
    postBuildCommands = ''
      uboot_size="$(${pkgs.coreutils}/bin/stat -c %s ${h700Uboot}/u-boot-sunxi-with-spl.bin)"
      boot_area_size="$((
        ${toString firmwarePartitionOffsetMiB} * 1024 * 1024
        - ${toString ubootStartSector} * 512
      ))"
      if [ "$uboot_size" -gt "$boot_area_size" ]; then
        echo "U-Boot exceeds the raw area before the first partition" >&2
        exit 1
      fi
      dd if=${h700Uboot}/u-boot-sunxi-with-spl.bin of="$img" bs=512 \
        seek=${toString ubootStartSector} conv=notrunc
    '';
  };

  assertions = [
    {
      assertion =
        builtins.match "/dev/dri/card[0-9]+" config.services.korriLinuxHost.compositor.drmDevice == null;
      message = "The H700 compositor must name its KMS card by hardware path, because card numbers follow probe order.";
    }
  ];
}
