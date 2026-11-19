# Assertions read both exported systems. The console configuration must stay
# the tabs6 Linux console that boots on the tablet; the default configuration
# replaces its session, lifeline and network stack with the shared product
# and keeps the board support.
{
  pkgs,
  nixpkgs,
  korri,
  configuration,
  consoleConfiguration,
}:
let
  inherit (pkgs) lib;
  c = configuration.config;
  console = consoleConfiguration.config;
  productModule = import ../../product/nixos-module.nix { inherit korri; };
  productCheck = import ../../product/check-lib.nix {
    inherit korri;
    inherit (pkgs) lib;
    defaultSystem = pkgs.stdenv.hostPlatform.system;
    referenceForSystem =
      system:
      import ../../product/reference.nix {
        inherit
          nixpkgs
          korri
          productModule
          system
          ;
      };
  };
  productFailures = productCheck.validate "tabs6" configuration;
  productKernelLines = lib.splitString "\n" (builtins.readFile ./kernel/config/korri.config);
  resolvedProductConfig = lib.splitString "\n" (
    builtins.readFile c.boot.kernelPackages.kernel.configfile
  );
  # Board support both systems keep.
  common =
    config:
    config.nixpkgs.system == "aarch64-linux"
    && config.networking.hostName == "tabs6"
    && config.hardware.deviceTree.name == "qcom/sm8150-samsung-gts6lwifi.dtb"
    && !config.boot.initrd.enable
    && config.boot.loader.external.enable
    && lib.hasInfix "tabs6-esp-install-hook" config.boot.loader.external.installHook
    && config.fileSystems."/".device == "/dev/disk/by-partlabel/userdata"
    && lib.elem "root=PARTLABEL=userdata" config.boot.kernelParams
    && lib.elem "console=tty0" config.boot.kernelParams
    && lib.elem "clk_ignore_unused" config.boot.kernelParams
    && lib.elem "pd_ignore_unused" config.boot.kernelParams
    && !config.hardware.enableRedistributableFirmware
    && lib.elem "msm" config.boot.kernelModules
    && config.hardware.graphics.enable
    && config.hardware.bluetooth.enable
    && config.hardware.bluetooth.powerOnBoot
    && config.nix.settings.max-jobs == 0
    && lib.all (unit: config.systemd.services ? ${unit}) [
      "rmtfs"
      "tqftpserv"
      "tabs6-wifi"
      "tabs6-charge"
      "tabs6-bt-addr"
      "tabs6-ufs-write-through"
      "tabs6-grow-root"
      "tabs6-usb-role-boot"
      "tabs6-usb-role-boot-vbus"
    ]
    # rmtfs keeps every modem EFS write in RAM: modemst1/2 and fsg stay unwritten.
    && lib.hasInfix "rmtfs -P -r -s" config.systemd.services.rmtfs.serviceConfig.ExecStart
    && config.networking.wireless.iwd.settings.DriverQuirks.DefaultInterface == "ath10k_snoc"
    && config.networking.wireless.iwd.settings.General.AddressRandomization == "disabled"
    && config.zramSwap.enable
    # No automatic USB-C role policy: the port changes role only by hand.
    && !(config.systemd.services ? tabs6-usb-role-auto);
  roleBoot = c.systemd.services.tabs6-usb-role-boot;
in
assert common c;
assert common console;
assert lib.all (a: a.assertion) c.assertions;
assert lib.all (a: a.assertion) console.assertions;
# Use the shared product validator instead of duplicating its policy here.
assert productFailures == [ ];

# Console: unchanged tabs6 behaviour.
assert console.tabs6.console.enable;
assert console.services.getty.autologinUser == "simon";
assert console.users.users ? simon;
assert console.services.openssh.enable;
assert console.systemd.services ? tabs6-usb-gadget;
assert console.systemd.services ? tabs6-idled;
assert console.systemd.network.networks."10-usb-gadget".address == [ "172.16.42.1/24" ];
assert !console.networking.networkmanager.enable;
assert console.services.logind.settings.Login.HandlePowerKey == "ignore";
assert console.tabs6.usbRole.gadgetUnit == "tabs6-usb-gadget";
assert console.system.stateVersion == "26.11";
assert !(console.services.korriProduct.installed or false);
assert !(console.systemd.services ? korri-shell);
assert lib.hasSuffix "linux-gts6lwifi-aarch64-unknown-linux-gnu-7.3.4"
  console.boot.kernelPackages.kernel.name;

# Product: the shared product owns the session, gadget, network and SSH.
assert !c.tabs6.console.enable;
assert !(c.users.users ? simon);
assert !(c.systemd.services ? tabs6-usb-gadget);
assert !(c.systemd.services ? tabs6-idled);
assert !c.services.openssh.enable;
assert c.networking.networkmanager.enable;
assert c.networking.networkmanager.wifi.backend == "wpa_supplicant";
assert !c.networking.networkmanager.wifi.scanRandMacAddress;
assert !c.networking.wireless.iwd.enable;
assert console.networking.wireless.iwd.enable;
assert !(c.systemd.network.networks ? "20-wlan");
assert c.systemd.services ? korri-shell;
assert c.systemd.services.korri-shell.serviceConfig.User != "root";
assert c.services.korriProduct.sleep.states == [ ];
# Short press: panel off/on through the compositor; long press: power off.
assert c.services.logind.settings.Login.HandlePowerKey == "ignore";
assert c.services.logind.settings.Login.HandlePowerKeyLongPress == "poweroff";
assert !c.systemd.services.systemd-bless-boot.enable;
assert c.services.korriProduct.kernel.overrideReason != null;
assert lib.hasSuffix "linux-gts6lwifi-korri-aarch64-unknown-linux-gnu-7.3.4"
  c.boot.kernelPackages.kernel.name;
assert c.boot.kernelPackages.kernel.drvPath != console.boot.kernelPackages.kernel.drvPath;
# Every product kernel line survived olddefconfig (the build checks it; this
# reads the resolved file the product system boots).
assert lib.all (
  line: !(lib.hasPrefix "CONFIG_" line) || lib.elem line resolvedProductConfig
) productKernelLines;
# The product gadget, with this board's identity, is the lifeline.
assert c.systemd.services.usb-gadget.environment.KORRI_USB_GADGET_NAME == "tabs6";
assert c.systemd.network.networks."10-usb-gadget".address == [ "10.42.7.1/24" ];
assert c.tabs6.usbRole.gadgetUnit == "usb-gadget";
assert lib.elem "usb-gadget.service" roleBoot.before;
assert lib.elem "tabs6-usb-gadget.service" console.systemd.services.tabs6-usb-role-boot.before;
# Display and input facts.
assert c.services.korriLinuxHost.compositor.backend == "drm";
assert c.services.korriLinuxHost.compositor.mainDisplay == "DSI-1";
assert (builtins.head c.services.korriLinuxHost.compositor.displays).mode == "2560x1600@60Hz";
assert c.services.korriLinuxHost.compositor.localInput.enable;
assert c.services.korriLinuxInput.provider.enable;
assert c.services.korriLinuxInput.inputd.enable;
assert lib.elem "tabs6-bt-pair" (map lib.getName c.environment.systemPackages);
# Plugins: the selection is seeded for the image and the first deploy.
assert lib.hasInfix "korri-image-seed" c.sdImage.populateRootCommands;
assert console.sdImage.populateRootCommands == "";
pkgs.runCommand "tabs6-module-check" { } ''
  touch "$out"
''
