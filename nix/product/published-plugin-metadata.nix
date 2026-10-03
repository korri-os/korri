# The publisher's offline-metadata-SYSTEM.tar.gz producer supplies signed
# proofs for every closure path, including dependencies served by upstream.
{
  pkgs,
  system,
  pluginPackages,
  publicKey,
}:
let
  hashes = {
    aarch64-linux = [
      "sha256-OlWpH0izwxlLfoKp4iPMv62WA35qrZhuHhcseOzXOso="
      "sha256-vgSXTEWZ0QhFi27/Z0tvcDBhYcGE+ya3zZHsj23xMb4="
      "sha256-cp2Kao3nNfe3jONB1nGfDZgl2KJOG1PGZanHB42lmzg="
    ];
    x86_64-linux = [
      "sha256-hEG1+tYVEXRLAM0qsI8tn+V89d/wmXdc8YLNWh/2GSQ="
      "sha256-i8dr+p67hQv8/60UzKA233PZSE7XE98smlmouSLKDHg="
      "sha256-8H0SKgcB80XLf8Rrw66n8JdVsnGYpoxIYSmMhpbF9xM="
    ];
  };
  batches = [
    "build-e5e27ed406f2"
    "build-9f946f33bd18"
    "build-0febdc65ce51"
  ];
  archives = pkgs.lib.imap0 (
    index: batch:
    pkgs.fetchurl {
      url = "https://github.com/korri-os/plugins/releases/download/${batch}/offline-metadata-${system}.tar.gz";
      hash = builtins.elemAt hashes.${system} index;
    }
  ) batches;
  closure = pkgs.closureInfo { rootPaths = pluginPackages; };
  packages = pkgs.lib.escapeShellArgs pluginPackages;
in
pkgs.runCommand "korri-image-plugin-offline-metadata"
  {
    passthru = { inherit archives; };
    nativeBuildInputs = [
      pkgs.buildPackages.python3
      pkgs.buildPackages.nix
    ];
  }
  ''
    python3 ${./published-plugin-proofs.py} "$out" ${closure}/store-paths ${pkgs.lib.escapeShellArgs archives}
    # Image store registration has no signatures. Reproduce it in a private
    # Nix database, never borrowing signatures from the build machine's store.
    store="local?state=$TMPDIR/proof-state"
    nix-store --store "$store" --load-db < ${closure}/registration
    nix --store "$store" --offline --extra-experimental-features nix-command \
      --option substituters "" --option extra-substituters "" \
      --option trusted-public-keys ${pkgs.lib.escapeShellArg publicKey} \
      --option extra-trusted-public-keys "" \
      store copy-sigs --recursive --substituter "file://$out" ${packages}
    nix --store "$store" --offline --extra-experimental-features nix-command \
      --option substituters "" --option extra-substituters "" \
      --option trusted-public-keys ${pkgs.lib.escapeShellArg publicKey} \
      --option extra-trusted-public-keys "" \
      store verify --recursive --sigs-needed 1 ${packages}
  ''
