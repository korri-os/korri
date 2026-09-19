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

  image.baseName = "nixos-rpminiv2";
  sdImage = {
    compressImage = true;
    # The upstream board identity fits the FAT label's 11-character limit.
    firmwarePartitionName = "RPMINIV2";
    rootVolumeLabel = "NIXOS_RPMINIV2";
    firmwareSize = 512;
    populateFirmwareCommands =
      let
        activeKernel = config.boot.kernelPackages.kernel;
        toplevel = config.system.build.toplevel;
        kernel = "${activeKernel}/${config.system.boot.loader.kernelFile}";
        initrd = "${config.system.build.initialRamdisk}/${config.system.boot.loader.initrdFile}";
        dtb = "${config.hardware.deviceTree.package}/${config.hardware.deviceTree.name}";
        kernelName = "${baseNameOf activeKernel}-${config.system.boot.loader.kernelFile}";
        initrdName = "${baseNameOf config.system.build.initialRamdisk}-${config.system.boot.loader.initrdFile}";
        # deviceTree.package ends in /dtbs, not at the owning store path.
        dtbName = "${baseNameOf activeKernel}-${baseNameOf config.hardware.deviceTree.name}";
        params = lib.concatStringsSep " " ([ "init=${toplevel}/init" ] ++ config.boot.kernelParams);
        loaderEntryTitle =
          if config.image.baseName == "nixos-rpminiv2-korri" then
            "NixOS Retroid Pocket Mini V2 Korri"
          else
            "NixOS Retroid Pocket Mini V2 candidate";
        grubEntryTitle =
          if config.image.baseName == "nixos-rpminiv2-korri" then
            "NixOS Retroid Pocket Mini V2 Korri"
          else
            "NixOS Retroid Pocket Mini V2";
        loaderConf = pkgs.writeText "loader.conf" ''
          timeout 0
          default nixos-generation-1.conf
          console-mode keep
        '';
        entry = pkgs.writeText "nixos-generation-1.conf" ''
          title ${loaderEntryTitle}
          version Generation 1 ${config.system.nixos.label}
          linux /EFI/nixos/${kernelName}
          initrd /EFI/nixos/${initrdName}
          options ${params}
          devicetree /EFI/nixos/${dtbName}
        '';
        grubCfg = pkgs.writeText "grub.cfg" ''
          insmod part_gpt
          insmod part_msdos
          set timeout=2
          set default=0
          set timeout_style=menu
          set lang=en_US
          loadfont /boot/grub/dejavu-mono.pf2
          set rotation=270
          set gfxmode=auto
          insmod efi_gop
          insmod gfxterm
          terminal_output gfxterm
          set menu_color_normal=cyan/blue
          set menu_color_highlight=white/blue

          menuentry '${grubEntryTitle}' {
                  search --set -f /EFI/nixos/${kernelName}
                  linux /EFI/nixos/${kernelName} ${params}
                  initrd /EFI/nixos/${initrdName}
                  devicetree /EFI/nixos/${dtbName}
          }
        '';
      in
      ''
        mkdir -p \
          ./firmware/EFI/BOOT \
          ./firmware/EFI/systemd \
          ./firmware/EFI/nixos \
          ./firmware/boot/grub \
          ./firmware/loader/entries
        # This is the exact GRUB/GOP handoff that remained visible and booted
        # the accepted NixOS TTY. Keep systemd-boot only as an inactive backup.
        cp ${rpminiRocknixBaseline}/bootaa64.efi ./firmware/EFI/BOOT/BOOTAA64.EFI
        cp ${pkgs.systemd}/lib/systemd/boot/efi/systemd-bootaa64.efi ./firmware/EFI/systemd/systemd-bootaa64.efi
        cp ${rpminiRocknixBaseline}/dejavu-mono.pf2 ./firmware/boot/grub/dejavu-mono.pf2
        cp ${grubCfg} ./firmware/boot/grub/grub.cfg
        cp ${kernel} ./firmware/EFI/nixos/${kernelName}
        cp ${initrd} ./firmware/EFI/nixos/${initrdName}
        cp ${dtb} ./firmware/EFI/nixos/${dtbName}
        # Retain BLS metadata for offline inspection and rollback tooling. GRUB
        # is the active removable-media loader.
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
