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
# Device modules import this and add radio-specific settings through
# `networking.networkmanager.ensureProfiles.profiles.korri.wifi`.
{ lib, ... }:

let
  wifiEnvPath = builtins.getEnv "KORRI_WIFI_ENV";
in
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

  # A flake build sees only git-tracked files, so a gitignored path inside
  # the repo is invisible to it by design. The env file is copied from the
  # build host instead.
  sdImage.populateRootCommands = lib.optionalString (wifiEnvPath != "") ''
    mkdir -p ./files/etc/korri
    install -m 0600 ${/. + wifiEnvPath} ./files/etc/korri/wifi.env
  '';
}
