# WiFi profile for the workshop LAN, with the secret held outside the repo.
#
# NetworkManager's ensureProfiles substitutes `$VAR` references from
# environmentFiles at activation, so the profile below carries the SSID and
# a placeholder and never the key. The key lives on the device at
# /etc/korri/wifi.env, written by the image builder from a file on the build
# host that is not committed (see sd-image.nix, populateRootCommands).
#
# The WCN7850 negotiated HE80 at 1080 Mbit/s TX on this network during
# first boot, so no power-save or rate workarounds are needed.
{ ... }:

{
  networking.networkmanager.ensureProfiles = {
    environmentFiles = [ "/etc/korri/wifi.env" ];
    profiles.vrackie = {
      connection = {
        id = "vrackie";
        type = "wifi";
        autoconnect = true;
        autoconnect-retries = 0;
      };
      wifi = {
        mode = "infrastructure";
        ssid = "vrackie";
      };
      wifi-security = {
        key-mgmt = "wpa-psk";
        psk = "$WIFI_PSK";
      };
      ipv4.method = "auto";
      ipv6.method = "auto";
    };
  };
}
