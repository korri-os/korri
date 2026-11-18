# Galaxy Tab S6 hardware facts and recorded limits for the shared Korri
# product. The console configuration in default.nix stays the tabs6 Linux
# console that boots on the tablet today.
{
  lib,
  pkgs,
  tabs6KorriKernel,
  ...
}:
let
  pair = pkgs.writeShellApplication {
    name = "tabs6-bt-pair";
    runtimeInputs = [
      pkgs.bluez
      pkgs.coreutils
      pkgs.gnugrep
      pkgs.gnused
    ];
    text = builtins.readFile ./bluetooth/tabs6-bt-pair.sh;
  };
in
{
  # The product supplies the session, the gadget, NetworkManager and SSH.
  tabs6.console.enable = false;

  # The product kernel: the console kernel plus kernel/config/korri.config.
  boot.kernelPackages = lib.mkForce (pkgs.linuxPackagesFor tabs6KorriKernel);
  services.korriProduct.kernel.overrideReason = ''
    The Tab S6 port runs on Linux 7.3.4. Its board patches were rebased to 7.3
    (rpmh-regulator without read-back, the SM8150 Secondary TDM sound card)
    and the native bonded-DSI display was brought up and proven on 7.3.4 on
    2026-11-17. No 7.2 build of this board exists or has been tested.
  '';

  # USB gadget identity. One dwc3 controller (usb@a600000, dr_mode otg with
  # a userspace role switch), so exactly one UDC appears in device role.
  services.korriProduct.usbGadget = {
    name = "tabs6";
    product = "Galaxy Tab S6 Korri";
    # The next free /24 after the H700 boards (10.42.5 and 10.42.6).
    # "TS6" in ASCII after the shared 02:52 prefix.
    address = "10.42.7.1";
    hostMac = "02:52:54:53:36:01";
    deviceMac = "02:52:54:53:36:02";
  };
  # tabs6-usb-role rebinds the product gadget when the port returns to device role.
  tabs6.usbRole.gadgetUnit = "usb-gadget";

  # No sleep state is verified with Korri's session freeze. The console's
  # s2idle idle suspend stays a console feature; here the power key powers
  # off, as on every product device without a sleep state.
  services.korriProduct.sleep.states = [ ];

  # Wi-Fi: NetworkManager with wpa_supplicant, the product's required
  # supplicant (the console uses standalone iwd). The modem chain and the efs
  # address are the console's (system/network.nix). Scans keep that address,
  # as iwd's AddressRandomization = disabled did.
  networking.networkmanager.wifi.scanRandMacAddress = false;
  # tabs6.local on the LAN, as on the console.
  networking.networkmanager.connectionConfig."connection.mdns" = 2;

  # Bluetooth is the shell's controller path on this tablet. BlueZ, the
  # efs address (tabs6-bt-addr) and power-on at boot come from
  # system/device.nix. A paired, trusted controller reconnects itself when it
  # is switched on; InputPlumber adopts its evdev device and inputd hands it
  # to korrid like a wired one. tabs6-bt-pair pairs the first time
  # (README.md, "Pair a Bluetooth controller").
  environment.systemPackages = [ pair ];

  services.korriLinuxHost = {
    label = "tabs6";
    compositor = {
      backend = "drm";
      # The msm KMS device (DPU at ae01000). simpledrm holds a DRM card until
      # msm takes the display over, so name msm's card by its platform device
      # instead of guessing its number. Unverified on the tablet.
      drmDevice = "/dev/dri/by-path/platform-ae01000.display-controller-card";
      renderDevice = "/dev/dri/renderD128";
      mainDisplay = "DSI-1";
      displays = [
        {
          # ANA38401: two bonded 1280x1600 DSI links, one 2560x1600 mode.
          name = "DSI-1";
          mode = "2560x1600@60Hz";
          x = 0;
          y = 0;
          touch = null;
        }
      ];
      renderer = "gles2";
      # Touch (fts1ba90a) reaches the shell through the compositor.
      localInput.enable = true;
      # sway would pick scale 2 from the panel's 227x142 mm, as on the console.
      extraConfig = ''
        output DSI-1 scale 1
      '';
    };
  };
}
