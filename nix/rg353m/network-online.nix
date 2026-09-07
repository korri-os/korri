# NetworkManager owns this device's ethernet and Wi-Fi; networkd configures
# only the USB gadget. Its wait-online unit still times out after two minutes
# and fails network-online.target, which intermittently took the Sunshine
# stream host down with a dependency failure at boot. NetworkManager's own
# wait-online unit remains the gate for that target.
{ lib, ... }:
{
  systemd.services.systemd-networkd-wait-online.enable = lib.mkForce false;
}
