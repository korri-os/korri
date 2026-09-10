# Workshop WiFi profile with the SSID and key held outside the repository.
#
# NetworkManager's ensureProfiles substitutes `$VAR` references from
# environmentFiles at activation, so the profile below carries only
# placeholders. The values live on the device at /etc/korri/wifi.env,
# written by the image builder from a file on the build host that is never
# committed: `nix build --impure` reads $KORRI_WIFI_ENV, a file of the form
#
#   WIFI_SSID=...
#   WIFI_PSK=...
#
# When $KORRI_WIFI_ENV is unset the image still builds and boots; the profile
# is written with empty values and stays inactive until /etc/korri/wifi.env is
# placed on the device and NetworkManager-ensure-profiles.service restarts.
#
# base/default.nix imports this. Device modules add radio-specific settings
# through networking.networkmanager.ensureProfiles.profiles.korri.wifi.
# ../formats/sd-card.nix owns the optional build-time copy of wifi.env.
{ ... }:

{
  networking.networkmanager.ensureProfiles = {
    environmentFiles = [ "/etc/korri/wifi.env" ];
    profiles.korri = {
      connection = {
        id = "korri";
        type = "wifi";
        autoconnect = true;
        autoconnect-retries = 0;
      };
      wifi = {
        mode = "infrastructure";
        ssid = "$WIFI_SSID";
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
