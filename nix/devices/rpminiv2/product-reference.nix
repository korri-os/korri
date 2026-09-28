# Only the approved native stop-thaw package, exact kiosk wiring and native
# audio-socket exposure differ from the shared product reference. No board
# hardware or candidate config may redefine this independent authority.
{ product, bare }:
let
  native = product.extendModules { modules = [ ./systemd/module.nix ]; };
  trusted = native.extendModules {
    modules = [
      ../../../clients/portal/nix/kiosk-freezer.nix
      (
        { config, lib, ... }:
        {
          # Independent authority for the approved Mini V2 exception. Do not
          # import portal.nix: broader candidate binds must still fail the gate.
          systemd.services.korri-inputd = {
            wants = [ "user@${toString config.services.korriLinuxHost.runtimeUid}.service" ];
            after = [ "user@${toString config.services.korriLinuxHost.runtimeUid}.service" ];
            serviceConfig = {
              ProtectHome = lib.mkForce "tmpfs";
              BindReadOnlyPaths = [
                "-/run/user/${toString config.services.korriLinuxHost.runtimeUid}/pipewire-0"
              ];
            };
          };
        }
      )
    ];
  };
  setting = name: path: {
    inherit name path;
    expected = product.pkgs.lib.getAttrFromPath path trusted.config;
  };
in
{
  inherit bare;
  product = trusted;
  requiredDbusPackages = product.pkgs.lib.subtractLists (map toString native.config.services.dbus.packages) (
    map toString trusted.config.services.dbus.packages
  );
  # The producer gates these on the patched package and enabled kiosk. Check
  # its exact authority and consumer wiring as well as every service field.
  settings = [
    (setting "native kiosk freezer polkit enabled" [
      "security"
      "polkit"
      "enable"
    ])
    (setting "native kiosk freezer policy" [
      "security"
      "polkit"
      "extraConfig"
    ])
    (setting "native kiosk freezer unit" [
      "systemd"
      "services"
      "korrid"
      "environment"
      "KORRID_PORTAL_UNIT"
    ])
    (setting "native kiosk freezer systemctl" [
      "systemd"
      "services"
      "korrid"
      "environment"
      "KORRID_SYSTEMCTL"
    ])
  ];
}
