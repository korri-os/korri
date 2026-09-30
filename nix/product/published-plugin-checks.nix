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
        inherit pkgs system hostPackage;
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
      published = import ./published-plugins.nix { inherit system; };
    in
    import ../../services/korrid/plugin-host/published-vm-test.nix {
      inherit pkgs hostPackage korridPackage;
      hostModule = korri.nixosModules.korri-plugin-host;
      publishedPaths = {
        ssh = published.korri-plugin-ssh;
        mgba = published.korri-plugin-mgba;
      };
      metadata = import ./published-plugin-metadata.nix {
        inherit pkgs system;
        pluginPackages = builtins.attrValues published;
        inherit (binding) publicKey;
      };
    };
}
