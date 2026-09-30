# Fresh SD images carry selected plugin closures and approved receipts.
# The image proof service also retains those closures in the system generation.
# Host receipts and GC roots control selections, not their only byte retention.
{
  korri,
  device,
}:
{ pkgs, lib, ... }:
let
  system = pkgs.stdenv.hostPlatform.system;
  binding = (import ./requirements.nix { inherit korri; }).constants.publishers."@korri";
  selection = import ./plugin-selection.nix {
    plugins = import ./published-plugins.nix { inherit system; };
  };
  seeded = import ../../services/korrid/plugin-host/image-seed.nix {
    inherit pkgs;
    hostPackage = korri.packages.${system}.korri-plugin-host;
    cacheUrl = binding.cacheUrl;
    pluginPackages = selection.${device};
  };
  checkedMetadata = import ./published-plugin-metadata.nix {
    inherit pkgs system;
    pluginPackages = selection.${device};
    publicKey = binding.publicKey;
  };
in
{
  sdImage.storePaths = seeded.storePaths;
  sdImage.populateRootCommands = lib.mkAfter seeded.populateRootCommands;
  # This image-owned prerequisite leaves the shared product host unit intact.
  # RequiredBy makes recovery fail closed if local proof registration fails.
  systemd.services.korri-plugin-offline-proofs = import ./published-plugin-offline-unit.nix {
    inherit pkgs;
    pluginPackages = selection.${device};
    metadata = checkedMetadata;
    publicKey = binding.publicKey;
  };
}
