{
  config,
  modulesPath,
  pkgs,
  ...
}:

let
  firmwarePartitionOffsetMiB = 16;
  uboot = pkgs.callPackage ./uboot.nix { };
  ubootStartSector = 64;
in
{
  imports = [
    "${modulesPath}/installer/sd-card/sd-image.nix"
    ./expand-root.nix
    ./browser-bench.nix
    ./gpu.nix
    ./usb-gadget.nix
    ./wifi.nix
  ];

  nixpkgs.hostPlatform = "aarch64-linux";

  boot = {
    consoleLogLevel = 7;
    # The stock kernel builds the RG353 panel path as modules. Load the DSI
    # PHY, DSI bridge, panel, and backlight in the initrd so the framebuffer
    # console appears before the root filesystem mounts.
    #
    # U-Boot reads the panel ID over DSI and rewrites the panel compatible
    # before Linux starts. This RG353M reports ID 0x3821, which U-Boot maps to
    # "anbernic,rg353v-panel-v2", a Sitronix ST7703 panel. Load both panel
    # drivers so either revision binds.
    initrd.kernelModules = [
      "mmc_block"
      "phy-rockchip-inno-dsidphy"
      "dw-mipi-dsi"
      "rockchipdrm"
      "panel-sitronix-st7703"
      "panel-newvision-nv3051d"
      "pwm_bl"
    ];
    # Start with the stock mainline kernel. It contains the RG353P device tree
    # and the RK3566 storage, display, RK817 audio, and RTL8821CS WiFi drivers.
    kernelPackages = pkgs.linuxPackages_latest;
    # The stock ST7703 driver draws nothing on the rg353v-panel-v2 glass. Ship
    # the corrected driver as an out-of-tree module in updates/, which depmod
    # prefers over the in-tree copy, instead of patching and rebuilding the
    # whole kernel.
    extraModulePackages = [
      (config.boot.kernelPackages.callPackage ./st7703-panel-module.nix { })
    ];
    kernelParams = [
      "console=ttyS2,1500000n8"
      "console=tty0"
    ];
    loader = {
      grub.enable = false;
      timeout = 3;
      generic-extlinux-compatible = {
        enable = true;
        configurationLimit = 3;
      };
    };
  };

  hardware = {
    # Upstream RGXX3 U-Boot detects the RG353M and selects the RG353P DTB.
    deviceTree = {
      enable = true;
      filter = "rk3566-anbernic-rg353p.dtb";
      name = "rockchip/rk3566-anbernic-rg353p.dtb";
      # The panel node names its supply "vdd" but the ST7703 driver asks for
      # "vcc" and "iovcc". Nothing claims vcc3v3_lcd0_n, so the regulator
      # core switches it off 30 s after boot and the panel goes dark. Keep it
      # on until a driver owns it.
      overlays = [
        {
          name = "rg353m-lcd-regulator-always-on";
          dtsText = ''
            /dts-v1/;
            /plugin/;
            / {
              compatible = "anbernic,rg353p";
            };
            &{/regulator-vcc3v3-lcd0} {
              regulator-always-on;
            };
          '';
        }
      ];
    };
    enableRedistributableFirmware = true;
  };

  image.baseName = "nixos-rg353m";

  sdImage = {
    compressImage = true;
    firmwarePartitionOffset = firmwarePartitionOffsetMiB;
    firmwarePartitionName = "NIXOS_BOOT";
    rootVolumeLabel = "NIXOS_RG353M";

    # U-Boot lives in the raw space before the first partition. The otherwise
    # unused FAT partition remains because the shared NixOS SD image builder
    # requires it.
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

  networking = {
    hostName = "rg353m";
    networkmanager.enable = true;
  };

  services = {
    # Root autologin on tty1 and ttyGS0 keeps the device recoverable without
    # a network. SSH still rejects passwords and accepts only the pinned key.
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

  environment.systemPackages = with pkgs; [
    iw
    networkmanager
  ];

  documentation.enable = false;
  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];
  system.stateVersion = "25.11";
}
