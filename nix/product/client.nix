# The client part of the Korri product: what people see and touch on the
# device itself (ADR 0051). The native shell, local controller capture
# (InputPlumber and inputd), local hardware controls and the shell freezer.
# It needs the server part, which ./nixos-module.nix imports beside it.
{ korri }:
{
  lib,
  config,
  ...
}:
let
  requirements = import ./requirements.nix { inherit korri; };
  inherit (requirements) constants;
in
{
  imports = [
    (import ../../clients/shell/nix/nixos-module.nix { inherit korri; })
    ./controls.nix
    ../../clients/shell/nix/freezer.nix
  ];

  config = {
    # No device has verified suspend support yet. logind handles the physical
    # controls, so a device with no declared sleep state shuts down cleanly.
    services.logind.settings.Login = lib.mkIf (config.services.korriProduct.sleep.states == [ ]) {
      HandlePowerKey = "poweroff";
      HandleLidSwitch = "poweroff";
      HandleLidSwitchExternalPower = "poweroff";
      HandleLidSwitchDocked = "poweroff";
    };

    # Controllers attached to this device. Streamed controllers reach games
    # through the server part's seat receiver instead. Local capture rides on
    # the server part's Linux host, which supplies its identities.
    services.korriLinuxInput = lib.mkIf config.services.korriLinuxHost.enable {
      provider.enable = true;
      inputd.enable = true;
    };

    services.korri.shell = {
      enable = true;
      startSurface = lib.mkForce constants.surfaceId;
    };
  };
}
