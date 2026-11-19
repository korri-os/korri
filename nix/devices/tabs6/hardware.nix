# Galaxy Tab S6 hardware facts and recorded limits for the shared Korri
# product. The console configuration in default.nix stays the tabs6 Linux
# console that boots on the tablet today.
{
  config,
  lib,
  pkgs,
  tabs6KorriKernel,
  ...
}:
let
  # Power key, short press: panel off or on. Both transitions go through
  # korrid's display RPCs, as korri-display-idle's do (product/controls.nix),
  # so korrid keeps the say over watched outputs. The compositor only reports
  # whether the panel is on.
  powerToggle = pkgs.writeShellScript "tabs6-power-key" ''
    set -euo pipefail
    action=idle
    if ! ${pkgs.sway-unwrapped}/bin/swaymsg -t get_outputs -r \
      | ${pkgs.jq}/bin/jq --exit-status 'any(.[]; .name == "DSI-1" and .power)' >/dev/null; then
      action=wake
    fi
    ${pkgs.curl}/bin/curl --silent --show-error --fail --max-time 10 \
      --unix-socket ${lib.escapeShellArg config.services.korridLinuxDevice.cliSocket} \
      --header 'content-type: application/json' \
      --data "{\"_tag\":\"app.displays.$action\",\"payload\":{}}" http://localhost/rpc \
      | ${pkgs.jq}/bin/jq --exit-status '.outcome._tag == "Ok"' >/dev/null
  '';

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
  # s2idle idle suspend stays a console feature.
  services.korriProduct.sleep.states = [ ];
  # Power key: a short press turns the panel off or on (the compositor
  # binding below); holding it for logind's long press (5 s) powers off.
  # The product's default, power off on any press, read as the screen
  # flashing and dying (2026-11-18).
  services.logind.settings.Login = {
    HandlePowerKey = lib.mkForce "ignore";
    HandlePowerKeyLongPress = "poweroff";
  };

  # A trial boot is good once every way in is up: SSH on 2222 (USB and
  # Wi-Fi), NetworkManager and korrid (system/boot.nix, tabs6-esp).
  tabs6.bootGood.units = [
    "korri-ssh.service"
    "NetworkManager.service"
    "korrid.service"
  ];

  # The plugin host opens plugin ports (SSH on 2222) with nftables-backed
  # iptables, as on every product device. The board's legacy iptables
  # firewall (system/network.nix, for the port's 6.12 kernel) put nixos-fw in
  # a separate legacy table that dropped 2222 on usb0 and wlan0 alike. The
  # product kernel has nf_tables.
  networking.firewall.package = lib.mkForce pkgs.iptables;

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

  # The GameSir-G8+ answers some HID-over-GATT reads with Attribute Not Found;
  # stock BlueZ then never reads the Report Map and makes no input device.
  hardware.bluetooth.package = pkgs.bluez.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ./bluetooth/bluez-hog-read-report-map-after-failed-reads.patch ];
  });

  # sway runs `exec` bindings through `sh -c`; the compositor's PATH has no sh.
  systemd.services.korri-compositor.path = [ pkgs.bash ];

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
      # --inhibited: the fullscreen shell and games inhibit shortcuts.
      extraConfig = ''
        output DSI-1 scale 1
        bindsym --locked --inhibited --no-repeat XF86PowerOff exec ${powerToggle}
      '';
    };
  };
}
