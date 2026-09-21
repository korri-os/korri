{ korri }:
let
  constants = {
    substituters = [
      "https://cache.nixos.org/"
      korri.cache.url
    ];
    runtimeAccount = {
      user = "korri";
      uid = 1000;
      group = "korri";
      gid = 1000;
      home = "/home/korri";
    };
    relays = [
      "wss://relay.nostr.band"
      "wss://relay.primal.net"
    ];
    browserNeverFocusAppIds = [ ];
    surfaceId = "shift";
    korridAddress = "127.0.0.1:39217";
    publishers = {
      "@korri" = {
        publicKey = "korri-plugins-1:qlK5Mgb3dYhF76WC4jGhrvL+CHsU93De7GpBFtrXb98=";
        cacheUrl = "https://github.com/korri-os/plugins/releases/download/cache/";
      };
    };
  };
  setting = name: path: expected: { inherit name path expected; };
  settingWithTestValue =
    name: path: expected: testValue:
    (setting name path expected) // { inherit testValue; };
  lockedSetting =
    name: path: expected:
    (setting name path expected) // { locked = true; };
  lockedSettingWithTestValue =
    name: path: expected: testValue:
    (settingWithTestValue name path expected testValue) // { locked = true; };
in
{
  inherit constants;

  # This is the public product-setting boundary, not a snapshot of native
  # implementation units. Required units, package contributions, and their
  # liveness/emission signatures are derived from the real product composition
  # for each system.
  # Removable plugins and their settings are absent.
  settings = [
    (setting "NetworkManager base policy" [ "networking" "networkmanager" "enable" ] true)
    (setting "network SSH disabled" [ "services" "openssh" "enable" ] false)
    (setting "network SSH firewall disabled" [ "services" "openssh" "openFirewall" ] false)
    (setting "keyboard-interactive SSH disabled" [
      "services"
      "openssh"
      "settings"
      "KbdInteractiveAuthentication"
    ] false)
    (setting "password SSH disabled" [
      "services"
      "openssh"
      "settings"
      "PasswordAuthentication"
    ] false)
    (settingWithTestValue "root SSH login policy" [
      "services"
      "openssh"
      "settings"
      "PermitRootLogin"
    ] "prohibit-password" "no")
    (setting "physical root autologin" [ "services" "getty" "autologinUser" ] "root")
    (setting "empty initial root password" [
      "users"
      "users"
      "root"
      "initialHashedPassword"
    ] "")
    (setting "documentation disabled" [ "documentation" "enable" ] false)
    (settingWithTestValue "NixOS state version" [ "system" "stateVersion" ] "25.11" "25.05")

    (lockedSetting "device local builds disabled" [ "nix" "settings" "max-jobs" ] 0)
    (lockedSetting "device builders disabled" [ "nix" "settings" "builders" ] "")
    (lockedSetting "distributed builds disabled" [ "nix" "distributedBuilds" ] false)
    (lockedSetting "substitution fallback disabled" [ "nix" "settings" "fallback" ] false)
    (lockedSetting "signed substitutions required" [ "nix" "settings" "require-sigs" ] true)
    (setting "small outputs must substitute" [
      "nix"
      "settings"
      "always-allow-substitutes"
    ] true)
    (lockedSettingWithTestValue "product substituters" [
      "nix"
      "settings"
      "substituters"
    ] constants.substituters (constants.substituters ++ [ "https://invalid.example/" ]))

    (setting "korri group gid" [ "users" "groups" "korri" "gid" ] constants.runtimeAccount.gid)
    (setting "korri normal user" [ "users" "users" "korri" "isNormalUser" ] true)
    (setting "korri uid" [ "users" "users" "korri" "uid" ] constants.runtimeAccount.uid)
    (setting "korri primary group" [
      "users"
      "users"
      "korri"
      "group"
    ] constants.runtimeAccount.group)
    (setting "korri home" [ "users" "users" "korri" "home" ] constants.runtimeAccount.home)
    (setting "korri home creation" [ "users" "users" "korri" "createHome" ] true)

    (setting "Linux host enabled" [ "services" "korriLinuxHost" "enable" ] true)
    (setting "runtime user" [
      "services"
      "korriLinuxHost"
      "runtimeUser"
    ] constants.runtimeAccount.user)
    (setting "runtime uid" [
      "services"
      "korriLinuxHost"
      "runtimeUid"
    ] constants.runtimeAccount.uid)
    (setting "runtime group" [
      "services"
      "korriLinuxHost"
      "runtimeGroup"
    ] constants.runtimeAccount.group)
    (setting "runtime gid" [
      "services"
      "korriLinuxHost"
      "runtimeGid"
    ] constants.runtimeAccount.gid)
    (lockedSettingWithTestValue "product relays" [
      "services"
      "korriLinuxHost"
      "relays"
    ] constants.relays [ "wss://invalid.example" ])
    (setting "host validation enabled" [
      "services"
      "korriLinuxHost"
      "validation"
      "enable"
    ] true)
    (setting "product audio enabled" [
      "services"
      "korriLinuxHost"
      "audio"
      "enable"
    ] true)
    (lockedSetting "product browser focus exclusion" [
      "services"
      "korriLinuxHost"
      "compositor"
      "neverFocusAppIds"
    ] constants.browserNeverFocusAppIds)

    (setting "bundle selection enabled" [ "services" "korriBundle" "enable" ] true)
    (setting "input provider enabled" [
      "services"
      "korriLinuxInput"
      "provider"
      "enable"
    ] true)
    (setting "input policy daemon enabled" [
      "services"
      "korriLinuxInput"
      "inputd"
      "enable"
    ] true)
    (setting "input provider required" [
      "services"
      "korriLinuxInput"
      "inputd"
      "requireProvider"
    ] true)
    (setting "broad raw input denied" [
      "services"
      "korriLinuxInput"
      "inputd"
      "allowBroadRawInput"
    ] false)
    (setting "Linux korrid enabled" [ "services" "korridLinuxDevice" "enable" ] true)

    (setting "shared portal enabled" [
      "services"
      "korri"
      "webSurfaceHost"
      "enable"
    ] true)
    (lockedSetting "product surface" [
      "services"
      "korri"
      "webSurfaceHost"
      "surfaceId"
    ] constants.surfaceId)
    (setting "shared portal browser enabled" [
      "services"
      "korri"
      "compositor"
      "kiosk"
      "enable"
    ] true)
    (lockedSettingWithTestValue "loopback korrid bind address" [
      "services"
      "korridLinuxDevice"
      "address"
    ] constants.korridAddress "127.0.0.1:39218")
    (setting "plugin host enabled" [ "services" "korri" "pluginHost" "enable" ] true)
    (settingWithTestValue "official plugin publisher binding"
      [
        "services"
        "korri"
        "pluginHost"
        "publishers"
      ]
      constants.publishers
      (constants.publishers // { "@review-test" = constants.publishers."@korri"; })
    )
  ];
}
