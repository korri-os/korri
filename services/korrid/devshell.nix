# korrid toolchain: host Rust plus contract generation.
{ pkgs, proseql }:
let
  proseqlSource = import ./proseql-source.nix { inherit pkgs proseql; };
  rustToolchain = pkgs.rust-bin.stable.latest.default;
in
pkgs.mkShell {
  packages = with pkgs; [
    rustToolchain
    typeshare
    # rquickjs-sys needs the bindgen feature, which needs libclang at build
    # time. Only the library is wanted: putting clang itself on PATH shadows
    # the stdenv compiler wrapper and C dependencies then lose their libc
    # headers.
    llvmPackages.libclang
    bun
    curl
    git
    jq
    openssh
    systemd
    unzip
  ];

  LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

  # bindgen drives libclang directly and inherits none of the compiler
  # wrapper's include path, so it must be told where the host libc headers
  # live.
  BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.stdenv.cc.libc.dev}/include";

  shellHook = ''
    export KORRI_ROOT="''${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
    ${proseqlSource.hydrateShell}
    export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/korrid-target"
    echo "Korrid toolchain ready"
  '';
}
