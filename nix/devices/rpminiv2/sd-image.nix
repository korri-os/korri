# SD-only candidate, based on the Odin 2 Portal's EFI image producer.
# Stock Retroid U-Boot must reach the card's removable-media EFI loader.
# No loader, ABL, Android boot image, or internal installer is included.
{
  config,
  lib,
  pkgs,
  rpminiKernel,
  rpminiFirmware,
  rpminiRocknixBaseline,
  ...
}:
{
  imports = [
    ../../base
    ../../device-cache/nixos-module.nix
    (import ../../formats/sd-card.nix { gpt = true; })
  ];
  nixpkgs.hostPlatform = "aarch64-linux";
  # OLED idle blanking is not sleep; no sleep state is verified.
  networking.hostName = "rpminiv2";
  system.tools.nixos-install.enable = false;

  boot = {
    kernelPackages = pkgs.linuxPackagesFor rpminiKernel;
    # Match the working ROCKNIX console level and avoid changing display probe
    # timing with the unused Qualcomm UART console. USB g_serial remains the
    # recovery shell after root mounts.
    consoleLogLevel = 4;
    kernelParams = [
      "quiet"
      "rootwait"
      "console=tty0"
      # Blank the framebuffer console after one idle minute to protect the OLED.
      "consoleblank=60"
      # Preserve ROCKNIX's efifb setting. This does not repair a faulty
      # loader GOP or disable the separate DT simple-framebuffer node.
      "video=efifb:off"
      "gpt"
      # Root is selected by NixOS's explicit SD label, not GPT discovery of
      # another disk. Do not let systemd mount internal Android partitions.
      "systemd.gpt_auto=0"
      "rd.systemd.gpt_auto=0"
    ];
    initrd = {
      compressor = "gzip";
      includeDefaultModules = false;
      availableKernelModules = [ ];
      # Expose the complete board subset here, including aliases and service
      # manifests, so stage 1 never depends on the root filesystem.
      extraFirmwarePaths = rpminiFirmware.firmwarePaths;
    };
    # A physical USB serial console is independent of panel visibility.
    # DWC3's role switch remains the upstream board setting. Cable operation
    # is not established until the actual device passes the arrival checks.
    kernelModules = [ "g_serial" ];
    loader = {
      grub.enable = false;
      timeout = 0;
      efi.canTouchEfiVariables = false;
      # The image populates the hardware-proven GRUB removable-media path
      # directly. Do not let bootctl replace it during later activations.
      systemd-boot.enable = false;
    };
  };
  services.udev.extraRules = ''
    SUBSYSTEM=="tty", KERNEL=="ttyGS0", TAG+="systemd", ENV{SYSTEMD_WANTS}+="serial-getty@ttyGS0.service"
  '';
  hardware = {
    enableAllHardware = lib.mkForce false;
    deviceTree = {
      enable = true;
      # Extended product configurations may select a featureful sibling of the
      # recovery kernel. Always follow the configuration's active kernel.
      name = "qcom/${config.boot.kernelPackages.kernel.dtbName}.dtb";
    };
    firmware = lib.mkBefore [ rpminiFirmware ];
    # The package contains the producer's full board/accessory firmware
    # subset. Do not add a second, older, machine-wide firmware collection.
    enableRedistributableFirmware = false;
    firmwareCompression = "none";
    # The first milestone is fbcon, not accelerated rendering. The kernel's
    # DRM client owns the console without a Mesa userspace stack.
    graphics.enable = lib.mkForce false;
  };

  # Keep one read-only DRM probe in both profiles. The recovery image remains
  # minimal; portal.nix adds the bounded product services without adding broad
  # hardware diagnostics.
  environment.systemPackages = [ pkgs.libdrm ];

  system.build.rpminiv2Firmware = import ./esp.nix {
    inherit
      config
      lib
      pkgs
      rpminiRocknixBaseline
      ;
  };

  image.baseName = "nixos-rpminiv2";
  sdImage = {
    compressImage = true;
    # The upstream board identity fits the FAT label's 11-character limit.
    firmwarePartitionName = "RPMINIV2";
    rootVolumeLabel = "NIXOS_RPMINIV2";
    firmwareSize = 512;
    populateFirmwareCommands = ''
      mkdir -p ./firmware
      cp -r ${config.system.build.rpminiv2Firmware}/. ./firmware/
    '';
    # These operations target only the image file inside the Nix sandbox.
    # GPT conversion and first-boot expansion follow the Odin implementation.
    postBuildCommands = ''
      truncate -s +1M "$img"
      ${pkgs.gptfdisk}/bin/sgdisk --mbrtogpt "$img"
      ${pkgs.gptfdisk}/bin/sgdisk \
        --typecode=1:ef00 --change-name=1:${config.sdImage.firmwarePartitionName} \
        --typecode=2:8305 --change-name=2:${config.sdImage.rootVolumeLabel} "$img"
      ${pkgs.gptfdisk}/bin/sgdisk --verify "$img"
      export PATH=${
        lib.makeBinPath [
          pkgs.util-linux
          pkgs.e2fsprogs
          pkgs.mtools
          pkgs.dtc
          pkgs.gptfdisk
        ]
      }:$PATH
      ${pkgs.python3}/bin/python3 ${./verify-image.py} "$img" --profile ${
        if config.image.baseName == "nixos-rpminiv2-korri" then "korri" else "recovery"
      }
    '';
  };
  fileSystems."/boot" = {
    device = "/dev/disk/by-label/${config.sdImage.firmwarePartitionName}";
    fsType = "vfat";
    options = [ "nofail" ];
  };
}
