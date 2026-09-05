# SD image for the AYN Odin 2 Portal.
#
# The device already carries everything needed to boot Linux, so this image
# writes nothing to internal storage and the Android install is never
# touched. The chain is:
#
#   PBL -> XBL -> ABL (BOOT MODE = Loader) -> loader_a (U-Boot 2025.01) ->
#   bootefi bootmgr -> EFI/BOOT/BOOTAA64.EFI on this card -> systemd-boot ->
#   kernel
#
# AYN ships U-Boot 2025.01-rc2 in the 10 MiB `loader_a` partition, reached by
# switching BOOT MODE from Android to Loader in the bootloader menu. Its
# default environment is the upstream Qualcomm one: `preboot` runs
# `scsi scan; usb start`, and `bootcmd` runs `bootefi bootmgr`. It does not
# run extlinux, and an extlinux card was tried and ignored. What it does run
# is the UEFI boot manager, whose removable-media fallback loads
# EFI/BOOT/BOOTAA64.EFI from the first EFI System Partition it can read.
#
# So the boot partition is an ESP carrying systemd-boot. The kernel is
# already an EFI application (CONFIG_EFI_STUB=y in the ROCKNIX config), and
# systemd-boot entries carry a `devicetree` line, so the Portal DTB is passed
# explicitly rather than trusting whatever U-Boot hands over for "AYN Odin 2".
#
# The ESP is populated at image-build time in the same layout the NixOS
# systemd-boot builder writes, so the first `nixos-rebuild` on the device
# takes ownership of it without reformatting.
{
  config,
  lib,
  modulesPath,
  pkgs,
  odinKernel,
  odinFirmware,
  ...
}:

{
  imports = [
    "${modulesPath}/installer/sd-card/sd-image.nix"
    ./expand-root.nix
    ./platform-policy.nix
    ./runtime-user.nix
    ./wifi.nix
  ];

  nixpkgs.hostPlatform = "aarch64-linux";

  boot = {
    consoleLogLevel = 7;

    # Linux 7.0.2 with the ROCKNIX SM8550 patch queue. Built for aarch64 but
    # compiled on an x86_64 host, because the aarch64 builder cannot spare the
    # ~30 GB a kernel compile needs.
    kernelPackages = pkgs.linuxPackagesFor odinKernel;

    # The AYN device tree puts stdout on serial0, which is not reachable
    # without opening the case. Add the panel so boot messages are visible on
    # the screen -- that is the only debugging channel this device has.
    kernelParams = [
      "console=tty0"
      "console=ttyMSM0,115200n8"
    ];

    # The ROCKNIX config enables RD_GZIP only; CONFIG_RD_ZSTD is off, so the
    # NixOS default zstd initrd would be unreadable by this kernel.
    initrd = {
      compressor = "gzip";

      # Firmware for drivers that are compiled into this kernel and probe
      # before the root filesystem exists. The msm GPU driver is built in
      # (CONFIG_DRM_MSM=y) and asks for the Adreno 740 microcode at ~6.6 s;
      # root mounts at ~8 s. Without these in the initrd the request fails
      # with ENOENT, the GPU stays uninitialised until something re-probes
      # it a minute later, and 3D is dead at boot. Every path resolves
      # against hardware.firmware, so the initrd copy and the store copy are
      # the same bytes by construction.
      #
      # Keep this list to what genuinely probes pre-root. The ADSP/CDSP
      # remoteprocs and ath12k come up after root today and are deliberately
      # not here; add them only if a kernel bump moves their probe earlier.
      extraFirmwarePaths = [
        "qcom/a740_sqe.fw"
        "qcom/gmu_gen70200.bin"
        "qcom/sm8550/a740_zap.mbn"
      ];
      # NixOS's default initrd module list targets PC and server storage
      # (ahci, sata, scsi). The ROCKNIX config builds none of those, and the
      # module-shrinking step fails on the first name it cannot resolve.
      # Nothing is lost by dropping the list: this board needs ext4, vfat,
      # and the Qualcomm SD host controller, and the config compiles all
      # three into the kernel image.
      includeDefaultModules = false;
      availableKernelModules = [ ];
    };

    loader = {
      grub.enable = false;
      timeout = 3;
      efi.canTouchEfiVariables = false;
      systemd-boot = {
        enable = true;
        configurationLimit = 3;
        # Keep going if bootctl cannot verify the firmware; U-Boot's EFI
        # implementation is not full UEFI and this must not block a rebuild.
        graceful = true;
        # The first `nixos-rebuild switch` on a freshly written card needs
        # --install-bootloader. U-Boot's UEFI keeps no EFI variables, so
        # `bootctl status` exits non-zero on "No boot loaders listed in EFI
        # Variables" and the builder reads that as no bootloader present.
        # Once bootctl has written its own loader.conf the check passes and
        # every later switch is unattended.
      };
    };
  };

  hardware = {
    # sd-image.nix turns this on, which pulls a generic initrd module list
    # aimed at PC and server hardware (3ware RAID controllers and the like).
    # The ROCKNIX config is a device config and builds none of them, so the
    # module-shrinking step fails outright. This board's hardware is named
    # explicitly instead.
    enableAllHardware = lib.mkForce false;

    deviceTree = {
      enable = true;
      name = "qcom/${odinKernel.dtbName}.dtb";
    };

    # Order matters. The option documents that the first package wins on a
    # filename collision, and two of ours are deliberate corrections to
    # linux-firmware rather than additions: the ath12k WCN7850 board-2.bin
    # carries this board's WiFi calibration, and vpu30_p4.mbn is the Venus
    # image AYN ships. mkBefore keeps them ahead of linux-firmware.
    firmware = lib.mkBefore [ odinFirmware ];

    # linux-firmware still supplies qcom/sm8550/a740_zap.mbn for the Adreno
    # 740 and the ath12k amss.bin/m3.bin, which are byte-identical upstream.
    enableRedistributableFirmware = true;

    # CONFIG_FW_LOADER_COMPRESS is not set in the ROCKNIX config, so the
    # kernel cannot decompress firmware. Leaving the NixOS default would make
    # every blob silently unloadable: no WiFi, no ADSP, no GPU zap shader.
    firmwareCompression = "none";

    # Adreno 740 userspace. Mesa's freedreno (GL) and turnip (Vulkan) drive
    # the msm kernel driver; without this there is a /dev/dri/renderD128 and
    # nothing that can open it.
    graphics.enable = true;
  };

  environment.systemPackages = with pkgs; [
    iw
    pciutils
    usbutils
    # Measure before tuning: glmark2 has a DRM/KMS backend and vulkaninfo
    # reports whether turnip found the GPU.
    glmark2
    mesa-demos
    vulkan-tools
  ];

  image.baseName = "nixos-odin2portal";

  sdImage = {
    compressImage = false;
    firmwarePartitionName = "ODIN2P_ESP";
    rootVolumeLabel = "ODIN2P_ROOT";
    # Three generations of kernel (30 MiB) plus initrd and DTB.
    firmwareSize = 1024;

    # Lay the ESP out exactly as systemd-boot-builder.py would: the loader at
    # the removable-media fallback path, one entry per generation under
    # loader/entries, and the kernel, initrd, and DTB under EFI/nixos. The
    # kernel is copied under its store name so the builder's later
    # deduplication recognises it.
    populateFirmwareCommands =
      let
        toplevel = config.system.build.toplevel;
        kernel = "${config.boot.kernelPackages.kernel}/${config.system.boot.loader.kernelFile}";
        initrd = "${config.system.build.initialRamdisk}/${config.system.boot.loader.initrdFile}";
        dtb = "${config.hardware.deviceTree.package}/${config.hardware.deviceTree.name}";
        kernelName = "${baseNameOf config.boot.kernelPackages.kernel}-${config.system.boot.loader.kernelFile}";
        initrdName = "${baseNameOf config.system.build.initialRamdisk}-${config.system.boot.loader.initrdFile}";
        dtbName = "${baseNameOf config.hardware.deviceTree.package}-${baseNameOf config.hardware.deviceTree.name}";
        params = lib.concatStringsSep " " ([ "init=${toplevel}/init" ] ++ config.boot.kernelParams);
        loaderConf = pkgs.writeText "loader.conf" ''
          timeout ${toString config.boot.loader.timeout}
          default nixos-generation-1.conf
          console-mode keep
        '';
        entry = pkgs.writeText "nixos-generation-1.conf" ''
          title NixOS
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
    # The WiFi key is supplied out of band and never enters the flake. A
    # flake build sees only git-tracked files, so a gitignored path inside
    # the repo is invisible to it by design. Instead the build reads
    # $ODIN2PORTAL_WIFI_ENV from the build host's environment (requires
    # --impure), a file of the form `WIFI_PSK=...` that wifi.nix consumes.
    # When unset the image still builds and boots; the WiFi profile stays
    # inactive until /etc/korri/wifi.env is placed on the device.
    populateRootCommands =
      let
        wifiEnvPath = builtins.getEnv "ODIN2PORTAL_WIFI_ENV";
      in
      lib.optionalString (wifiEnvPath != "") ''
        mkdir -p ./files/etc/korri
        install -m 0600 ${/. + wifiEnvPath} ./files/etc/korri/wifi.env
      '';

    # The shared builder writes an MBR table with the boot partition typed
    # 0x0b (W95 FAT32). Two things want GPT instead. The UEFI spec defines
    # an EFI System Partition as a GPT partition with a specific type GUID;
    # U-Boot tolerated the MBR shim, but bootctl (which manages the ESP on
    # every nixos-rebuild) refuses to touch a non-GPT disk. So convert the
    # table in place after the image is assembled: sgdisk --mbrtogpt keeps
    # every partition boundary byte-identical and only rewrites the table,
    # then the partitions get their proper GPT type GUIDs. The 1 MiB of
    # slack makes room for the GPT backup header at the end of the image;
    # expand-root moves it to the true end of the card on first boot.
    postBuildCommands = ''
      truncate -s +1M "$img"
      ${pkgs.gptfdisk}/bin/sgdisk --mbrtogpt "$img"
      ${pkgs.gptfdisk}/bin/sgdisk \
        --typecode=1:ef00 --change-name=1:${config.sdImage.firmwarePartitionName} \
        --typecode=2:8305 --change-name=2:${config.sdImage.rootVolumeLabel} \
        "$img"
      ${pkgs.gptfdisk}/bin/sgdisk --verify "$img"
    '';
  };

  # systemd-boot expects the ESP at /boot. sd-image.nix declares
  # /boot/firmware for the same partition but leaves it noauto, so only this
  # mount is live.
  fileSystems."/boot" = {
    device = "/dev/disk/by-label/ODIN2P_ESP";
    fsType = "vfat";
    options = [ "nofail" ];
  };

  networking = {
    hostName = "odin2portal";
    networkmanager.enable = true;
  };

  services = {
    # Autologin on the panel keeps the device inspectable with no network and
    # no serial, which is the only way to read a failure on first boot.
    getty.autologinUser = "root";
    openssh = {
      enable = true;
      openFirewall = true;
      settings = {
        KbdInteractiveAuthentication = false;
        PasswordAuthentication = false;
        PermitRootLogin = "prohibit-password";
      };
    };
  };

  users.users.root = {
    initialHashedPassword = "";
    openssh.authorizedKeys.keys = [
      "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAACAQC/PwyhdbVKd6jcG55m/1sUgEf0x3LUeS9H4EK5vk9PKhvDsjOQOISyR1LBmmXUFamkpFo2c84ZgPMj33qaPfOF0VfmF79vdAIDdDt5bmsTU6IbT7tGJ1ocpHDqhqbDO3693RdbTt1jTQN/eo3AKOfnrMouwBZPbPVqoWEhrLUvUTuTq7VQ+lUqWkvGs4D6D8UeIlG9VVgVhad3gCohYsjGdzgOUy0V4c8t3BuHrIE6//+6YVJ9VWK/ImSWmN8it5RIREDgdSYujs1Uod+ovr8AvaGFlFC9GuYMsj7xDYL1TgaWhy5ojk6JcuuF0cmoqffoW/apYdYM6Vxi5Xe6aJUhVyguZDovWcqRdPv2q0xtZn6xvNkoElEkrb6t0CAbGKf++H4h8/v5MsMt9wUPJAJBa24v0MlU8mXTUwhFLP5YQ/A8AAb5Y3ty/6DaOlvvTzt5Om2SMrZ1XaL1II35dFNZ/Os3zRpqdWq9SnpisRA+Bpf0bPUjdi8D8rRJn8g3zO5EsldBlZg82PiJcRHANbydTSK6Jzw7A8S5gMyPoH80Pq5MbQPvPpevTfOKy14NyTYPHGj0j5y7EQP7yb6w70LtqdRLRLQSTCdF0qTjVWw/qdt9MXkS7cdQe4yBADmjwozwPuxAs/jNpxELcVPEWBK6DcAIFD0vv3Xaw7reXpXFTQ=="
    ];
  };

  documentation.enable = false;
  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];
  system.stateVersion = "25.11";
}
