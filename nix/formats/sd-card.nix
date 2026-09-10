# The RG353M card uses MBR; the Odin card is converted to GPT by its device
# module. Keep that choice explicit without making the base depend on SD media.
{ gpt }:
{ lib, modulesPath, ... }:

let
  wifiEnvPath = builtins.getEnv "KORRI_WIFI_ENV";
in
{
  imports = [
    "${modulesPath}/installer/sd-card/sd-image.nix"
    (import ./expand-root.nix { inherit gpt; })
  ];

  # Preserve the existing out-of-band WiFi provisioning. NixOS merges this
  # fragment with device-specific root population, such as the extlinux files.
  sdImage.populateRootCommands = lib.optionalString (wifiEnvPath != "") ''
    mkdir -p ./files/etc/korri
    install -m 0600 ${/. + wifiEnvPath} ./files/etc/korri/wifi.env
  '';
}
