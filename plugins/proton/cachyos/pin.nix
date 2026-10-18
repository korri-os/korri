# The pinned Proton-CachyOS arm64 build, moved from
# plugins/steam/packages/proton-cachyos-arm64/default.nix. cachyos-prepare
# downloads and verifies it on the device. toolName is the single definition
# that every consumer substitutes at build time. The SHA-256 is the old SRI
# pin in hex (`nix hash convert --to base16`).
rec {
  # The cache directory name under <cache-root>/proton/cachyos/.
  version = "11.0-20260601-slr-arm64";
  toolName = "proton-cachyos-${version}";
  url = "https://github.com/CachyOS/proton-cachyos/releases/download/cachyos-11.0-20260601-slr/proton-cachyos-11.0-20260601-slr-arm64.tar.xz";
  sha256 = "6793a697c8153626a9c01fcd212aae972bd1596a1d13d482620f64abf5fc69d9";
}
