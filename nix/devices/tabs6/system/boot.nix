# Boot: no initrd, root on userdata by GPT label, ESP on cache written by
# tabs6-esp (not by NixOS's systemd-boot installer).
#
# Why no initrd: the port boots without one (UFS, ext4 and the USB gadget are
# built into its kernel), and the kernel resolves root=PARTLABEL= itself. An
# initrd would cost ESP space (two kernels must fit, see DAILY_DRIVER.md) and
# boot time for nothing it has to do.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.tabs6;
  rootDevice = "/dev/disk/by-partlabel/userdata";
  tabs6-esp = pkgs.callPackage ../esp/package.nix { };
  # udev makes the by-partlabel links after early boot; a unit that opens
  # them must wait for their device units (2026-11-18: tabs6-boot-good and
  # tabs6-grow-root both ran before the links existed).
  partlabelUnit = label: "dev-disk-by\\x2dpartlabel-${label}.device";
in
{
  options.tabs6 = {
    dtb = lib.mkOption {
      type = lib.types.str;
      description = "Device tree path, relative to the kernel's dtbs directory.";
    };
    placeholderKernel = lib.mkOption {
      type = lib.types.bool;
      description = "True while the system uses the nixpkgs kernel instead of ../kernel.";
    };
    bootGood = {
      units = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        description = ''
          Units that must all be active before a trial boot of a new default
          entry is blessed (tabs6-esp bless). They are this system's ways in:
          if one never comes up, the system reboots and the boot counter
          sends the next boot to LAST KNOWN GOOD.
        '';
      };
      timeout = lib.mkOption {
        type = lib.types.ints.positive;
        default = 300;
        description = "Seconds the units have, from the start of tabs6-boot-good, before the trial fails.";
      };
    };
  };

  config = {
    boot.initrd.enable = false;
    boot.loader.grub.enable = false;
    boot.loader.external = {
      enable = true;
      # switch-to-configuration boot|switch calls this with the new toplevel.
      installHook = "${tabs6-esp}/bin/tabs6-esp-install-hook";
    };

    # The ESP is never mounted (tabs6-esp opens it through a loop device only
    # while it writes), so bootctl cannot find it and this unit always fails.
    # systemd-boot boots without a seed file; the kernel seeds itself.
    systemd.suppressedSystemUnits = [ "systemd-boot-random-seed.service" ];
    # systemd-bless-boot-generator sees the boot counter in LoaderBootCountPath
    # and pulls in systemd-bless-boot, which cannot find the ESP either and
    # fails on every counted boot. Mask it: tabs6-boot-good blesses the entry
    # through tabs6-esp.
    systemd.services.systemd-bless-boot.enable = false;

    hardware.deviceTree = {
      enable = true;
      name = cfg.dtb;
    };

    boot.kernelParams = [
      # No serial console: a UART with no cable blocks every later printk (PORT.md).
      "console=tty0"
      "root=PARTLABEL=userdata"
      "rootfstype=ext4"
      "rootwait"
      "rw"
      # Firmware-owned suppliers and clocks (UFS.md).
      "fw_devlink=permissive"
      "clk_ignore_unused"
      "pd_ignore_unused"
    ];
    # Rendering hundreds of debug lines into the unaccelerated 2560x1600
    # framebuffer cost seconds of boot on the port.
    boot.consoleLogLevel = 4;

    fileSystems."/" = {
      device = rootDevice;
      fsType = "ext4";
      options = [ "noatime" ];
    };

    # UFS LUN 0 (userdata, cache) runs with the device write cache off. The
    # link logs a UFS error-handler run about once a second, and each run
    # resets the device, which drops whatever sat in its volatile write cache:
    # files written minutes before a reboot came back zero-filled or missing.
    # With the cache off a write is on flash before it completes. The kernel
    # saves the page (WCE sav 0), so this only repairs a reverted setting.
    # Unit attentions from the resets fail single attempts, hence the retry.
    systemd.services.tabs6-ufs-write-through = {
      description = "Turn off the UFS LUN 0 volatile write cache";
      wantedBy = [ "sysinit.target" ];
      before = [ "sysinit.target" ];
      unitConfig.DefaultDependencies = false;
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
      script = ''
        d=/sys/class/scsi_disk/0:0:0:0
        for _ in $(seq 1 300); do
          [ "$(cat $d/cache_type)" = "write through" ] && { echo "sda: write through"; exit 0; }
          echo "write through" > $d/cache_type 2>/dev/null || sleep 0.1
        done
        echo "sda: could not turn off the write cache" >&2
        exit 1
      '';
    };

    # The image is built to fit its contents. Grow it to the whole userdata
    # partition online on the first boot; resize2fs is a no-op afterwards.
    systemd.services.tabs6-grow-root = {
      description = "Grow the root filesystem to fill userdata";
      wantedBy = [ "multi-user.target" ];
      requires = [ (partlabelUnit "userdata") ];
      after = [
        "local-fs.target"
        (partlabelUnit "userdata")
      ];
      serviceConfig = {
        Type = "oneshot";
        ExecStart = "${pkgs.e2fsprogs}/bin/resize2fs ${rootDevice}";
      };
    };

    # Register the image's store paths and set the system profile once.
    boot.postBootCommands = ''
      if [ -f /nix-path-registration ]; then
        ${config.nix.package.out}/bin/nix-store --load-db < /nix-path-registration
        ${config.nix.package.out}/bin/nix-env -p /nix/var/nix/profiles/system --set /run/current-system
        rm -f /nix-path-registration
      fi
    '';

    # The ESP tool reads every boot fact from the toplevel, so the image builder
    # and the on-device hook write the same entry.
    system.systemBuilderCommands = ''
      ln -s ${config.hardware.deviceTree.package}/${cfg.dtb} $out/tabs6-dtb
      echo ${if cfg.placeholderKernel then "placeholder" else "device"} > $out/tabs6-kernel-kind
      ln -s ${config.systemd.package}/lib/systemd/boot/efi/systemd-bootaa64.efi $out/tabs6-systemd-boot.efi
    '';

    environment.systemPackages = [ tabs6-esp ];

    # Boot assessment (esp/tabs6-esp.sh). On a trial boot of a new default
    # entry, bless it once every way in is up; otherwise reboot, and
    # systemd-boot starts LAST KNOWN GOOD. A boot that is not a trial (a
    # blessed entry, or LAST KNOWN GOOD itself) is left alone, so a later
    # failure of one of these units never loops reboots.
    systemd.services.tabs6-boot-good = {
      description = "Bless the Tab S6 boot entry once the system is reachable";
      wantedBy = [ "multi-user.target" ];
      # Without the ESP link `tabs6-esp trial` fails and the trial was read
      # as "not a trial": never blessed, so the next boot fell back.
      requires = [ (partlabelUnit "cache") ];
      after = [
        "local-fs.target"
        (partlabelUnit "cache")
      ];
      path = [
        tabs6-esp
        config.systemd.package
      ];
      # exec, not oneshot: multi-user.target must not wait for the checks.
      serviceConfig.Type = "exec";
      script = ''
        if ! tabs6-esp trial; then
          echo "not a trial boot of the default entry; nothing to bless"
          exit 0
        fi
        deadline=$((SECONDS + ${toString cfg.bootGood.timeout}))
        while :; do
          missing=""
          for unit in ${lib.escapeShellArgs cfg.bootGood.units}; do
            systemctl is-active --quiet "$unit" || missing="$missing $unit"
          done
          [ -z "$missing" ] && break
          if [ "$SECONDS" -ge "$deadline" ]; then
            echo "trial boot failed:$missing not active after ${toString cfg.bootGood.timeout} s; rebooting into LAST KNOWN GOOD" >&2
            systemctl reboot
            exit 1
          fi
          sleep 5
        done
        tabs6-esp bless
      '';
    };

    # A hang that stops systemd stops the qcom_wdt pings, and the SoC resets
    # into the next boot (proven to keep the tablet alive with 30 s on
    # 2026-11-18). After a failed trial that is LAST KNOWN GOOD.
    systemd.settings.Manager.RuntimeWatchdogSec = "30s";
  };
}
