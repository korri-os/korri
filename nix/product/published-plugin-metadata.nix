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
      "sha256-7CrkIcu1L41VepSHbIoMyC9X0N/n3ScYr/2s522gJUw="
      "sha256-9G4kQ1/yT262GDM6Nh76hsfxusqHO/kpkbr1HjzqtBo="
      "sha256-HK9NUfO2QdAVH0ML/25w1jVv0/iRHoboqzRlOiZNQKI="
      "sha256-MjY/ot46w90e0yYuuCLFUqCSFm/mfaosoS2iNr1euSs="
      "sha256-11aKuvntwJ/32O0KphGciaRs5vQAwm+N5/LdMcLpuVU="
      "sha256-c/xUGPEJ8oe1DNy3QoWGzOjQQX8vXO1lScLon9HK5oU="
      "sha256-hsweWv+nG36pGzvKOQvkRP/yMEfUMrthG+07kOQiqMg="
      "sha256-zosjFjOdMPXs/QwwMBivU1gl1y0Rx86U/h/KoyBoxl8="
      "sha256-eHZS2qvDZaR3vgwhgohdwvBkpHemcZshNIAyksjj4Ug="
      "sha256-SKaXMxwHcvObG38d6O8y0zmt8uJpvZiCj2tYB4B5KGY="
    ];
    x86_64-linux = [
      "sha256-nJZ+40Y/QRYy2ke3Th22b8g872uYFjH/c4ps2/IBrAE="
      "sha256-qgeu2GC3SdJNtJ+BZkpEwIn28Gjvp2ZQb3o1rQ+Lj+o="
      "sha256-O6/jNWL9SfZHqfnl0hSedYj/IV4JAdFKmQbtiPIyleI="
      "sha256-6mhzwXHpjZlL/bMXGrXXym3/pPH2S2BOuh5q63SYmdw="
      "sha256-10pYQrNFZ2UWKaJ9mqGYRKiYmFfiQFGuZgrz3T63l4g="
      "sha256-nxXI2PNF3mRQrpNTPPZdTwWDwpw9DWVCwqI009ZzXjM="
      "sha256-xaDcRqu8GYh949bu0ucbCiuuH5bH5M6g2YzpLHT0Fow="
      "sha256-+UJDoH/DEvxb6wEU1rrykmHp7TJ22yQx1+UFmKZ+Hkg="
      "sha256-2+W2lN3ULacUqn5ea2OoyKqg3URm1K8VQy+goBLTJ9o="
      "sha256-qbs9q3vQv2rabVxnKDic8HiFsP00x029yi+YiSzmBVw="
      "sha256-bLex0Fnc7SYpX7NOacQi/f2WHiEar1eJkJskKvRXvDM="
      "sha256-gT0iyalfKM89uPRsS86BH6wJEsfqoiJbIBrkqhsqt3I="
      "sha256-WVwyQHyMefoqcT39u2cqAJsmszeTH6tmhfS6WkNRT8A="
    ];
  };
  batches = [
    "build-9514b1b382d3"
    "build-9514b1b382d3-solarus"
    "build-ac8fae593e42-zquest"
    "build-ac8fae593e42"
    "build-dea5fd453fc6"
    "build-617e858e4daa"
    "build-75eb41b75907"
    "build-824050bfa4c3"
    "build-365ce4e054d5"
    "build-30594ae66c81"
    "build-adaccd5610ce"
    "build-bd4bfcfe4e9a"
    "build-bd4bfcfe4e9a-starter-pack"
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
