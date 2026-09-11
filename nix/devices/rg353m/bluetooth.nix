# Live UART1 already declares realtek,rtl8821cs-bt. Its firmware is retained.
# Linux 6.18.2 lacks the two bool protocol selections in the current image;
# SERIAL_DEV_BUS, BT_HCIUART_SERDEV and the modular HCI_UART/BT_RTL are present.
# Enable protocol support inside hci_uart, not a replacement DT or vendor daemon.
{ lib, ... }:
{
  boot.kernelPatches = [
    {
      name = "rg353m-bt-hciuart-rtl";
      patch = null;
      structuredExtraConfig = {
        BT_HCIUART_3WIRE = lib.kernel.yes;
        BT_HCIUART_RTL = lib.kernel.yes;
      };
    }
  ];
  hardware.bluetooth.enable = true;
}
