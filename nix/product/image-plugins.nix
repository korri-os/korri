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
  sdImage.storePaths = seeded.storePaths;
  sdImage.populateRootCommands = lib.mkAfter seeded.populateRootCommands;
  # Run before restore-all, after the SD image's first-boot --load-db. Never
  # ask a remote cache for proof, and never build on the device. Once the
  # closure has authenticated metadata, later boots skip the content rehash.
  systemd.services.korri-plugin-host.preStart = lib.mkAfter ''
    if ! ${pkgs.nix}/bin/nix --option substituters "" store verify \
      --no-contents --recursive --sigs-needed 1 ${packages} >/dev/null 2>&1; then
      cache="$(${pkgs.coreutils}/bin/mktemp -d /run/korri-plugin-host/offline-cache.XXXXXXXX)"
      trap '${pkgs.coreutils}/bin/rm -rf -- "$cache"' EXIT
      ${pkgs.gnutar}/bin/tar --no-same-owner --no-same-permissions \
        -xzf ${offlineMetadata} -C "$cache"
      ${pkgs.nix}/bin/nix --option substituters "" store copy-sigs \
        --recursive --substituter "file://$cache" ${packages}
      ${pkgs.nix}/bin/nix --option substituters "" store verify \
        --recursive --sigs-needed 1 ${packages} >/dev/null
    fi
  '';
}
