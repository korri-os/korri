{
  pkgs,
  korri,
  hostPackage,
  korridPackage,
}:
let
  binding = (import ./requirements.nix { inherit korri; }).constants.publishers."@korri";
in
builtins.listToAttrs (
  map
    (system: {
      name = "korri-published-plugins-${system}";
      value = import ./published-plugin-check.nix {
        inherit system;
        pkgs =
          if system == pkgs.stdenv.hostPlatform.system then
            pkgs
          else
            import korri.inputs.nixpkgs { inherit system; };
        hostPackage = korri.packages.${system}.korri-plugin-host;
        inherit (binding) publicKey cacheUrl;
      };
    })
    [
      "aarch64-linux"
      "x86_64-linux"
    ]
)
// pkgs.lib.optionalAttrs (pkgs.stdenv.hostPlatform.system == "x86_64-linux") {
  korri-published-plugin-lifecycle =
    let
      system = "x86_64-linux";
      catalog = import ./catalog-plugins.nix { inherit pkgs system; };
      published = catalog.plugins;
    in
    import ../../services/korrid/plugin-host/published-vm-test.nix {
      inherit
        pkgs
        hostPackage
        korridPackage
        catalog
        ;
      hostModule = korri.nixosModules.korri-plugin-host;
      publishedPaths = {
        ssh = published.korri-plugin-ssh;
        mgba = published.korri-plugin-mgba;
        fake08 = published.korri-plugin-fake08;
        solarus = published.korri-plugin-solarus;
        zquestClassic = published.korri-plugin-zquest-classic;
        starterPack = published.korri-plugin-starter-pack;
      };
      metadata = import ./published-plugin-metadata.nix {
        inherit pkgs system hostPackage;
        pluginPackages = [
          published.korri-plugin-ssh
          published.korri-plugin-mgba
          published.korri-plugin-fake08
          published.korri-plugin-solarus
          published.korri-plugin-zquest-classic
          published.korri-plugin-starter-pack
        ];
        publishers."@korri" = binding;
      };
    };
}
