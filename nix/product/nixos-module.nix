# Complete shared Korri product composition. Device modules import this once and
# provide hardware facts and recorded limits only.
{ korri }:
{
  lib,
  ...
}:
let
  requirements = import ./requirements.nix { inherit korri; };
  inherit (requirements) constants;
in
{
  imports = [
    ../base
    ../device-cache/nixos-module.nix
    (import ../../services/inputd/nix/korri-linux-host-core.nix { inherit korri; })
    (import ../../clients/portal/nix/nixos-module.nix {
      inherit korri;
      chromiumArgs = [ "--force-prefers-reduced-motion" ];
    })
    (import ../../services/korrid/plugin-host/nixos-module.nix { inherit korri; })
  ];

  options.services.korriProduct.installed = lib.mkOption {
    type = lib.types.bool;
    default = true;
    readOnly = true;
    internal = true;
    description = "Diagnostic marker for the shared Korri product composition; the gate verifies the complete resulting contract.";
  };

  config = {
    # The NixOS default cache is already in the base list. Keep one exact
    # product list rather than appending the default a second time through the
    # composed module stack.
    nix.settings.substituters = lib.mkForce constants.substituters;

    users.groups.${constants.runtimeAccount.group}.gid = constants.runtimeAccount.gid;
    users.users.${constants.runtimeAccount.user} = {
      isNormalUser = true;
      uid = constants.runtimeAccount.uid;
      group = constants.runtimeAccount.group;
      home = constants.runtimeAccount.home;
      createHome = true;
    };

    services.korriLinuxHost = {
      enable = true;
      runtimeUser = constants.runtimeAccount.user;
      runtimeUid = constants.runtimeAccount.uid;
      runtimeGroup = constants.runtimeAccount.group;
      runtimeGid = constants.runtimeAccount.gid;
      # Product discovery endpoints do not authorize publication. Ticket 05's
      # automatic first-boot identity still publishes nothing and does not join
      # federation until that separate identity behavior permits it.
      relays = lib.mkForce constants.relays;
      validation.enable = true;
      audio.enable = true;
      # The shared portal's real Wayland app id has not been accepted on a
      # device yet. Product ownership starts with no guessed or legacy id;
      # device modules cannot carry the old browser-specific exclusion.
      compositor.neverFocusAppIds = lib.mkForce constants.browserNeverFocusAppIds;
    };

    services.korri.webSurfaceHost = {
      enable = true;
      surfaceId = lib.mkForce constants.surfaceId;
    };
    services.korri.compositor.kiosk.enable = true;
    services.korridLinuxDevice.address = lib.mkForce constants.korridAddress;

    services.korri.pluginHost = {
      enable = true;
      publishers = constants.publishers;
    };
  };
}
