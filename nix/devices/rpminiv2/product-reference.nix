# Only the approved native stop-thaw package and exact kiosk wiring differ
# from the shared product reference. No board hardware or candidate config is
# imported here: either would let device edits redefine their own authority.
{ product, bare }:
let
  native = product.extendModules { modules = [ ./systemd/module.nix ]; };
  trusted = native.extendModules {
    modules = [ ../../../clients/portal/nix/kiosk-freezer.nix ];
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
