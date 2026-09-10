# Evaluate the real base without any device or image module, then check that
# every exported device keeps the shared access and WiFi contract.
{
  pkgs,
  nixpkgs,
  korri,
}:
let
  inherit (pkgs) lib;
  base = nixpkgs.lib.nixosSystem {
    system = pkgs.stdenv.hostPlatform.system;
    modules = [ ./default.nix ];
  };
  shared = config: {
    inherit (config.services.openssh) enable openFirewall;
    ssh = lib.getAttrs [
      "KbdInteractiveAuthentication"
      "PasswordAuthentication"
      "PermitRootLogin"
    ] config.services.openssh.settings;
    rootKeys = config.users.users.root.openssh.authorizedKeys.keys;
    rootPassword = config.users.users.root.initialHashedPassword;
    autologin = config.services.getty.autologinUser;
    networkmanager = config.networking.networkmanager.enable;
    inherit (config.system) stateVersion;
    documentation = config.documentation.enable;
    nixFeatures = config.nix.settings.experimental-features;
  };
  samePolicy = device: shared device.config == shared base.config;
  profile = base.config.networking.networkmanager.ensureProfiles;
in
assert !(base.options ? sdImage);
assert !(base.options ? isoImage);
assert base.config.boot.postBootCommands == "";
assert base.config.fileSystems == { };
assert lib.all samePolicy (lib.attrValues korri.nixosConfigurations);
assert profile.profiles.korri.wifi.ssid == "$WIFI_SSID";
assert profile.profiles.korri.wifi-security.psk == "$WIFI_PSK";
assert profile.environmentFiles == [ "/etc/korri/wifi.env" ];
assert !base.config.services.openssh.enable;
assert !base.config.services.openssh.openFirewall;
assert base.config.users.users.root.openssh.authorizedKeys.keys == [ ];
pkgs.runCommand "korri-base-module-check" { } ''
  touch "$out"
''
