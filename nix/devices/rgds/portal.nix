# RG DS hardware facts and recorded hardware limits.
#
# Connector and device names come from the running candidate, not assumption:
# /dev/dri/by-path/platform-display-subsystem-card resolves to card1
# (display-subsystem) with connectors DSI-1 and DSI-2, while the Panfrost GPU
# owns card0 and renderD128. Card numbers follow probe order, so the display
# controller is named by hardware path.
{
  config,
  korri,
  pkgs,
  ...
}:
let
  system = pkgs.stdenv.hostPlatform.system;
in
{
  # Linux 7.2's RG DS DTS declares realtek,rtl8821cs-bt on UART1, like the
  # RG353M. Reuse its hci_uart protocol selections and BlueZ enablement.
  imports = [ ../rg353m/bluetooth.nix ];

  # This package contains the controller profile observed for this board. The
  # product module owns InputPlumber and inputd themselves.
  services.korriLinuxInput.provider.extraDataPackages = [
    korri.packages.${system}.rgds-inputplumber-data
  ];

  # RTL8821CS deep power saving caused pairing failures and dropped links on
  # the RG353M; the RG DS also logged a Deep Power mode firmware-ack failure.
  networking.networkmanager.wifi.powersave = false;

  # Linux 7.2 rk356x-base.dtsi names gpu@fde60000; Panfrost registers devfreq
  # with that device's name. Share this node between the governor and limits.
  services.korri.clockGovernor.gpuDevfreqNodes = [ "fde60000.gpu" ];

  # The two Goodix touch controllers. This load used to sit in the removed
  # device gadget module.
  boot.kernelModules = [ "goodix_ts" ];

  # USB gadget identity. The board's device tree selects peripheral mode on
  # usb_host0_xhci, so this port is a gadget and never a host.
  services.korriProduct.usbGadget = {
    name = "rgds";
    product = "RG DS NixOS";
    address = "10.42.1.1";
    hostMac = "02:52:47:52:47:01";
    deviceMac = "02:52:47:52:47:02";
  };

  services.korriLinuxHost = {
    label = "rgds";
    compositor = {
      backend = "drm";
      localInput.enable = true;
      drmDevice = "/dev/dri/by-path/platform-display-subsystem-card";
      renderDevice = "/dev/dri/renderD128";
      mainDisplay = "DSI-1";
      displays = [
        {
          name = "DSI-1";
          mode = "640x480@60Hz";
          x = 0;
          y = 0;
          touch = null;
        }
      ];
      renderer = "gles2";
    };
  };

  # Recorded limit: Chromium's GPU path has not been accepted on the RG DS.
  # Keep software rendering local to this device rather than product policy.
  services.korri.compositor.kiosk.extraChromiumArgs = [ "--disable-gpu" ];

  assertions = [
    {
      assertion =
        builtins.match "/dev/dri/card[0-9]+" config.services.korriLinuxHost.compositor.drmDevice == null;
      message = "The RG DS compositor must name its KMS card by hardware path, because card numbers follow probe order.";
    }
  ];
}
