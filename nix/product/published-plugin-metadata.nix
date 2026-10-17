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
      "sha256-BjC9CnvTh+ODlO7S1/V8HhSyhpuGUlJamctrzd5Yayo="
      "sha256-n/gfhZ4CWD30VvXquUBQkTW6JTuKXT/f/CzldOPi35Y="
      "sha256-Jil4OmZoHllMkBjT2bnYZpvVXudlbCBisgTdKyo7w5Y="
    ];
    x86_64-linux = [
      "sha256-nJZ+40Y/QRYy2ke3Th22b8g872uYFjH/c4ps2/IBrAE="
      "sha256-qgeu2GC3SdJNtJ+BZkpEwIn28Gjvp2ZQb3o1rQ+Lj+o="
      "sha256-O6/jNWL9SfZHqfnl0hSedYj/IV4JAdFKmQbtiPIyleI="
    ];
  };
  batches = [
    "build-9514b1b382d3"
    "build-9514b1b382d3-solarus"
    "build-ac8fae593e42-zquest"
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
