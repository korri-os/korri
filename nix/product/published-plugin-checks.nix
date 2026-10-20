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
      inherit published;
      roots = [
        "korri-plugin-ssh"
        "korri-plugin-mgba"
        "korri-plugin-starter-pack"
      ];
      # The pinned Starter pack's exact requirements, which image selection
      # must also select (plugin-selection.nix). Pure evaluation cannot read
      # its manifest; the VM test reads it and requires exactly this list.
      requires.korri-plugin-starter-pack = [
        "korri-plugin-fake08"
        "korri-plugin-solarus"
        "korri-plugin-zquest-classic"
      ];
      metadata = import ./published-plugin-metadata.nix {
        inherit pkgs system;
        pluginPackages = builtins.attrValues published;
        inherit (binding) publicKey;
      };
    };
}
