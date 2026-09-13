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
      # No serial console, on purpose, and no earlycon.
      #
      # Two things were wrong with the previous line. `earlycon` writes to
      # UART5 before any clock driver runs, and this kernel hung on it: the
      # first boot that got past the regulator core was the first boot
      # without it. And `console=ttyS5` named a port this kernel cannot
      # have, because ROCKNIX's config sets SERIAL_8250_NR_UARTS=5, which
      # is ttyS0 through ttyS4. Neither parameter could pay for itself on a
      # board whose UART pins are internal pads.
      #
      # The console is ramoops. Every printk lands in the reserved region
      # the device tree declares, survives the panic reboot, and is read
      # back by the next boot's flight recorder.
      "console=tty0"
      # Store every kmsg dump reason, not only oops and panic, so a clean
      # shutdown and a watchdog reset are distinguishable afterwards.
      "printk.always_kmsg_dump=1"
      # A crash that resets in one second is unreadable. Give ramoops time
      # to flush, and give a human time to notice the rings stayed on.
      "panic=10"
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

    # 128 MiB, against a default of 30. Our own boot chain does not need any
    # of it -- U-Boot lives in the raw space before the first partition -- but
    # `hybrid-boot` boots this kernel through ROCKNIX's loader, and that
    # loader reads only the first partition. A 21 MB kernel and a 10 MB
    # initrd do not fit in 30 MiB alongside a device tree per name its
    # boot.scr can ask for.
    firmwareSize = 128;

    # Nothing is put here at build time; `hybrid-boot` fills it later when it
    # is needed. The shared NixOS image builder requires the partition to
    # exist either way.
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

  # --- flight recorder ---------------------------------------------------
  #
  # The panel does not work, the USB gadget has never appeared, and the UART
  # console exists only on internal pads. The device can still write to its
  # own boot partition, which we can read back in a card reader, so let it
  # keep a log of how far it got.
  #
  # Everything is appended, never overwritten, and each entry is stamped with
  # the boot id. Several entries from one power-on means the board is
  # restarting in a loop -- which is what a blinking charge LED suggests,
  # against the stock system's stable blue-then-red.
  #
  # VFAT, FAT, and the two needed NLS tables are built into this kernel, so
  # mounting works with no modules loaded.
  # pstore has to be mounted to be read, and the initrd does not do it for
  # us. PSTORE_RAM is built in, so the filesystem is there once mounted.
  boot.initrd.postDeviceCommands = lib.mkAfter ''
    mkdir -p /flight /pstore
    if mount -t vfat /dev/disk/by-label/NIXOS_BOOT /flight 2>/dev/null; then
      {
        echo "=== initrd $(cat /proc/sys/kernel/random/boot_id 2>/dev/null) ==="
        cat /proc/uptime
        cat /proc/cmdline
        dmesg || true
      } >> /flight/flight.log 2>&1

      # What the previous kernel wrote before it died. This is the only
      # console this board has, so copy every record and leave them in
      # place: the second read is the one a human does.
      if mount -t pstore pstore /pstore 2>/dev/null; then
        for f in /pstore/*; do
          [ -e "$f" ] || continue
          {
            echo "=== pstore $(basename "$f") ==="
            cat "$f"
          } >> /flight/pstore.log 2>&1
        done
        umount /pstore
      fi
      sync
      umount /flight
    fi
  '';

  systemd.services.flight-recorder = {
    description = "Append this boot's kernel log to the boot partition";
    wantedBy = [ "sysinit.target" ];
    after = [ "systemd-udev-settle.service" ];
    unitConfig.DefaultDependencies = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
    };
    script = ''
      mkdir -p /run/flight
      mount -t vfat /dev/disk/by-label/NIXOS_BOOT /run/flight
      {
        echo "=== stage2 $(cat /proc/sys/kernel/random/boot_id) ==="
        cat /proc/uptime
        ${pkgs.util-linux}/bin/dmesg || true
        echo "--- failed units ---"
        systemctl --failed --no-legend || true
      } >> /run/flight/flight.log 2>&1
      sync
      umount /run/flight
    '';
  };

  networking.hostName = "r36tmax";

  # Serial console only. Root login without a password is acceptable here and
  # nowhere else: this image has no network, exists to be watched over a wire,
  # and is replaced the moment it boots.
  users.users.root.initialHashedPassword = "";
  services.getty.autologinUser = "root";

  system.stateVersion = "25.05";
}
