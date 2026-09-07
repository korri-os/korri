# Odin 2 Portal radio settings for the shared workshop WiFi profile
# (../wifi.nix).
#
# The WCN7850 negotiated HE80 at 1080 Mbit/s TX on this network during
# first boot, so no power-save or rate workarounds are needed.
{ ... }:

{
  imports = [ ../wifi.nix ];
}
