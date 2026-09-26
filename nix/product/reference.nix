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
  # A complete, trusted replay, not exceptions to the service signature. Only
  # the Mini V2 product export uses these approved native producers. Never
  # derive the expected package or hooks from the configuration under test.
  deviceReferences = nixpkgs.lib.optionalAttrs (system == "aarch64-linux") {
    rpminiv2 = import ../devices/rpminiv2/product-reference.nix {
      inherit product bare;
    };
  };
}
