# SD-only candidate, based on the Odin 2 Portal's EFI image producer.
# Stock Retroid U-Boot must reach the card's removable-media EFI loader.
# No loader, ABL, Android boot image, or internal installer is included.
{
  config,
  lib,
  pkgs,
  korri,
  rpminiKernel,
  rpminiFirmware,
  ...
}:
{
  imports = [
    ../../base
    ../../device-cache/nixos-module.nix
    (import ../../formats/sd-card.nix { gpt = true; })
  ];
  nixpkgs.hostPlatform = "aarch64-linux";
  networking.hostName = "rpminiv2";
  system.tools.nixos-install.enable = false;

  boot = {
    kernelPackages = pkgs.linuxPackagesFor rpminiKernel;
    # The preserved vendor patch reports initrd unpack failures at KERN_DEBUG.
    # Level 8 exposes them; this candidate trades quieter boot for diagnostics.
    consoleLogLevel = 8;
    kernelParams = [
      "console=ttyMSM0,115200n8"
      # Stage 1 uses the last console for its emergency shell. Keep the
      # documented panel/USB keyboard primary, not the board UART.
      "console=tty0"
      # Preserve ROCKNIX's efifb setting. This does not repair a faulty
      # loader GOP or disable the separate DT simple-framebuffer node.
      "video=efifb:off"
      # Root is selected by NixOS's explicit SD label, not GPT discovery of
      # another disk. Do not let systemd mount internal Android partitions.
      "systemd.gpt_auto=0"
      "rd.systemd.gpt_auto=0"
    ];
    initrd = {
      compressor = "gzip";
      includeDefaultModules = false;
      availableKernelModules = [ ];
      # ROCKNIX embeds early GPU/DSP firmware in its kernel. Include the
      # complete board subset here, with its aliases and service manifests,
      # so stage 1 does not depend on the root filesystem for firmware.
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
      systemd-boot = {
        enable = true;
        configurationLimit = 3;
        graceful = true;
      };
    };
  };
  services.udev.extraRules = ''
    SUBSYSTEM=="tty", KERNEL=="ttyGS0", TAG+="systemd", ENV{SYSTEMD_WANTS}+="serial-getty@ttyGS0.service"
  '';
  hardware = {
    enableAllHardware = lib.mkForce false;
    deviceTree = {
      enable = true;
      name = "qcom/${rpminiKernel.dtbName}.dtb";
    };
    firmware = lib.mkBefore [ rpminiFirmware ];
    # The package contains the producer's full board/accessory firmware
    # subset. Do not add a second, older, machine-wide firmware collection.
    enableRedistributableFirmware = false;
    firmwareCompression = "none";
    graphics.enable = true;
    # Supply the CLI and daemon for arrival tests without starting the radio.
    bluetooth = {
      enable = true;
      powerOnBoot = false;
    };
  };

  # Keep the candidate at a console. These programs permit hardware checks
  # without starting a compositor, korrid, SSH, or a plugin host.
  environment.systemPackages = [
    korri.packages.aarch64-linux.korrid
    pkgs.evtest
    pkgs.libdrm
    pkgs.alsa-utils
    pkgs.pciutils
    pkgs.usbutils
    pkgs.dtc
  ];

  image.baseName = "nixos-rpminiv2";
  sdImage = {
    compressImage = true;
    # The upstream board identity fits the FAT label's 11-character limit.
    firmwarePartitionName = "RPMINIV2";
    rootVolumeLabel = "NIXOS_RPMINIV2";
    firmwareSize = 512;
    populateFirmwareCommands =
      let
        toplevel = config.system.build.toplevel;
        kernel = "${rpminiKernel}/${config.system.boot.loader.kernelFile}";
        initrd = "${config.system.build.initialRamdisk}/${config.system.boot.loader.initrdFile}";
        dtb = "${config.hardware.deviceTree.package}/${config.hardware.deviceTree.name}";
        kernelName = "${baseNameOf rpminiKernel}-${config.system.boot.loader.kernelFile}";
        initrdName = "${baseNameOf config.system.build.initialRamdisk}-${config.system.boot.loader.initrdFile}";
        # deviceTree.package ends in /dtbs, not at the owning store path.
        dtbName = "${baseNameOf rpminiKernel}-${baseNameOf config.hardware.deviceTree.name}";
        params = lib.concatStringsSep " " ([ "init=${toplevel}/init" ] ++ config.boot.kernelParams);
        loaderConf = pkgs.writeText "loader.conf" ''
          timeout 0
          default nixos-generation-1.conf
          console-mode keep
        '';
        entry = pkgs.writeText "nixos-generation-1.conf" ''
          title NixOS Retroid Pocket Mini V2 candidate
          version Generation 1 ${config.system.nixos.label}
          linux /EFI/nixos/${kernelName}
          initrd /EFI/nixos/${initrdName}
          options ${params}
          devicetree /EFI/nixos/${dtbName}
        '';
      in
      ''
        mkdir -p ./firmware/EFI/BOOT ./firmware/EFI/systemd ./firmware/EFI/nixos ./firmware/loader/entries
        cp ${pkgs.systemd}/lib/systemd/boot/efi/systemd-bootaa64.efi ./firmware/EFI/BOOT/BOOTAA64.EFI
        cp ${pkgs.systemd}/lib/systemd/boot/efi/systemd-bootaa64.efi ./firmware/EFI/systemd/systemd-bootaa64.efi
        cp ${kernel} ./firmware/EFI/nixos/${kernelName}
        cp ${initrd} ./firmware/EFI/nixos/${initrdName}
        cp ${dtb} ./firmware/EFI/nixos/${dtbName}
        cp ${loaderConf} ./firmware/loader/loader.conf
        cp ${entry} ./firmware/loader/entries/nixos-generation-1.conf
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
      ${pkgs.python3}/bin/python3 ${./verify-image.py} "$img"
    '';
  };
  fileSystems."/boot" = {
    device = "/dev/disk/by-label/${config.sdImage.firmwarePartitionName}";
    fsType = "vfat";
    options = [ "nofail" ];
  };
}
