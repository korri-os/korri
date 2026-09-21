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
in
{
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
}
