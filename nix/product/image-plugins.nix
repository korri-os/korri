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
  catalog = import ./catalog-plugins.nix { inherit pkgs system; };
  selection = import ./plugin-selection.nix { plugins = catalog.plugins; };
  seeded = import ../../services/korrid/plugin-host/image-seed.nix {
    disabled = [ "@korri:starter-pack" ];
    inherit pkgs catalog;
    hostPackage = korri.packages.${system}.korri-plugin-host;
    publishers = (import ./requirements.nix { inherit korri; }).constants.publishers;
    pluginPackages = selection.${device};
  };
  checkedMetadata = import ./published-plugin-metadata.nix {
    inherit pkgs system;
    hostPackage = korri.packages.${system}.korri-plugin-host;
    publishers = (import ./requirements.nix { inherit korri; }).constants.publishers;
    pluginPackages = selection.${device};
  };
in
{
  # Owner decision 2026-10-18: every launch runs inside Gamescope by default
  # (ADR 0008). Only where the image seeds the Gamescope plugin, because an
  # enabled modifier without its plugin refuses every launch. korrid writes
  # this into a new device.yaml only; the person turns it off there, or per
  # system, runner or game.
  services.korridLinuxDevice.initialModifiers =
    lib.mkIf
      (lib.elem (toString catalog.plugins.korri-plugin-gamescope) (map toString selection.${device}))
      {
        "@korri:gamescope/gamescope".enabled = true;
      };
  services.korri.pluginHost.officialCatalogUrl = catalog.catalogUrl;
  sdImage.storePaths = seeded.storePaths;
  sdImage.populateRootCommands = lib.mkAfter seeded.populateRootCommands;
  # This image-owned prerequisite leaves the shared product host unit intact.
  # RequiredBy makes recovery fail closed if local proof registration fails.
  systemd.services.korri-plugin-offline-proofs = import ./published-plugin-offline-unit.nix {
    metadata = checkedMetadata;
  };
}
