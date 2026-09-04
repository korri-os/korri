# Bring-up WiFi profile so the device joins the workshop LAN without a console.
#
# The RG353M uses an RTL8821CS on SDIO. The first native boots logged SDIO
# timeouts from rtw88_8821cs, so this path is best effort until the SDIO
# clock is tuned. The USB gadget is the reliable path.
{ ... }:

{
  networking.networkmanager.ensureProfiles.profiles.vrackie = {
    connection = {
      id = "vrackie";
      type = "wifi";
      autoconnect = true;
      autoconnect-retries = 0;
    };
    wifi = {
      mode = "infrastructure";
      ssid = "vrackie";
      # RTL8821CS deep power saving caused pairing failures and dropped
      # connections during the Sunshine feasibility run.
      powersave = 2;
    };
    wifi-security = {
      key-mgmt = "wpa-psk";
      psk = "REDACTED-WIFI-PSK";
    };
    ipv4.method = "auto";
    ipv6.method = "auto";
  };
}
