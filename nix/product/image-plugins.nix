# Fresh SD images own the selected plugin closures and receipts. Installed
# systems keep no package reference; the plugin host's GC roots alone own
# selections after first boot.
{ korri, plugins, device }:
{ pkgs, lib, ... }:
let
  system = pkgs.stdenv.hostPlatform.system;
  selection = import ./plugin-selection.nix {
    # Use the publisher's actual producer with this exact Korri builder.
    # Source-only input avoids a Korri -> publisher -> Korri flake cycle.
    plugins = (import (plugins.outPath + "/nix") { inherit korri; }).packages.${system};
  };
  seeded = import ../../services/korrid/plugin-host/image-seed.nix {
    inherit pkgs;
    hostPackage = korri.packages.${system}.korri-plugin-host;
    cacheUrl = (import ./requirements.nix { inherit korri; }).constants.publishers."@korri".cacheUrl;
    pluginPackages = selection.${device};
  };
  packages = lib.escapeShellArgs (map toString selection.${device});
  # The publisher's immutable batch contains Nix's signed metadata for every
  # shipped plugin closure, including dependencies omitted from the public
  # cache. The hash pins the exact archive; it carries no NAR or private key.
  offlineMetadata = pkgs.fetchurl {
    url = "https://github.com/korri-os/plugins/releases/download/build-5e564b9cdceb/offline-metadata-${system}.tar.gz";
    hash = "sha256-MsLt13c55GVvPY44RHbpBUuv0VgUyqfhkFy0y46YViY=";
  };
in
{
  assertions = [
    {
      assertion = system == "aarch64-linux";
      message = "The pinned offline plugin metadata is signed for aarch64-linux only";
    }
  ];
  sdImage.storePaths = seeded.storePaths;
  sdImage.populateRootCommands = lib.mkAfter seeded.populateRootCommands;
  # This image-owned prerequisite leaves the shared product host unit intact.
  # A RequiredBy link makes restore-all fail closed if proof registration fails.
  # Boot registration has loaded the SD store before either service starts.
  systemd.services.korri-plugin-offline-proofs = {
    description = "Register signed metadata for shipped plugin closures";
    requiredBy = [ "korri-plugin-host.service" ];
    before = [ "korri-plugin-host.service" ];
    after = [ "systemd-tmpfiles-setup.service" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      RuntimeDirectory = "korri-plugin-offline-proofs";
      RuntimeDirectoryMode = "0700";
      UMask = "0077";
    };
    # Never ask a remote cache for proof or build on a device. Reverify the
    # already-signed closure on later boots without rehashing its contents.
    script = ''
      set -euo pipefail
      if ! ${pkgs.nix}/bin/nix --option substituters "" store verify \
        --no-contents --recursive --sigs-needed 1 ${packages} >/dev/null 2>&1; then
        cache="$(${pkgs.coreutils}/bin/mktemp -d /run/korri-plugin-offline-proofs/offline-cache.XXXXXXXX)"
        trap '${pkgs.coreutils}/bin/rm -rf -- "$cache"' EXIT
        ${pkgs.gzip}/bin/gzip -dc ${offlineMetadata} | \
          ${pkgs.gnutar}/bin/tar --no-same-owner --no-same-permissions \
            -xf - -C "$cache"
        ${pkgs.nix}/bin/nix --option substituters "" store copy-sigs \
          --recursive --substituter "file://$cache" ${packages}
        ${pkgs.nix}/bin/nix --option substituters "" store verify \
          --recursive --sigs-needed 1 ${packages} >/dev/null
      fi
    '';
  };
}
