{
  nixpkgs,
  system,
  korri,
  productModule,
}:
let
  hardwareStub = {
    networking.hostName = "korri-product-reference";
    services.korriLinuxHost = {
      label = "korri-product-reference";
      compositor = {
        backend = "drm";
        drmDevice = "/dev/dri/card0";
        renderDevice = "/dev/dri/renderD128";
      };
    };
  };
  inherit (nixpkgs) lib;
  freezeSettingPaths = [
    [ "security" "polkit" "enable" ]
    [ "security" "polkit" "extraConfig" ]
    [ "systemd" "services" "korrid" "environment" "KORRID_PORTAL_UNIT" ]
    [ "systemd" "services" "korrid" "environment" "KORRID_SYSTEMCTL" ]
  ];
  # The same product with upstream systemd: the freeze producer stays inert,
  # so the difference is exactly the freeze D-Bus policy.
  unfrozen = product.extendModules {
    modules = [ { systemd.package = lib.mkForce product.pkgs.systemd; } ];
  };
  product = nixpkgs.lib.nixosSystem {
    inherit system;
    specialArgs = { inherit korri; };
    modules = [
      productModule
      hardwareStub
    ];
  };
  bare = nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [
      {
        networking.hostName = "korri-bare-reference";
        system.stateVersion = "25.11";
      }
    ];
  };
in
{
  inherit product bare;
  # The portal freeze is product behavior on every device since 2026-09-30.
  # Its D-Bus policy and exact authority come from this independent reference,
  # never from the configuration under test.
  # system-path also changes with the systemd package, so it is not freeze policy.
  requiredDbusPackages = lib.subtractLists (
    map toString (unfrozen.config.services.dbus.packages ++ [ product.config.system.path ])
  ) (map toString product.config.services.dbus.packages);
  settings = map (path: {
    name = "portal freeze ${lib.concatStringsSep "." path}";
    inherit path;
    expected = lib.getAttrFromPath path product.config;
    # The producer already uses mkForce for KORRID_SYSTEMCTL.
    locked = true;
  }) freezeSettingPaths;
}
