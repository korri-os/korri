# THROWAWAY SPIKE. Workstation-only cross tasks; no deployment or device access.
{ pkgs, proseql }:
let
  lib = pkgs.lib;
  proseqlSource = import ../../services/korrid/proseql-source.nix { inherit pkgs proseql; };
  rust = pkgs.rust-bin.stable.latest.default.override {
    targets = [ "armv7-unknown-linux-gnueabi" ];
  };
  makeApp =
    action:
    let
      program = pkgs.writeShellApplication {
        name = "slint-spike-webos-${action}";
        runtimeInputs = [
          rust
          pkgs.stdenv.cc
          pkgs.llvmPackages.libclang
          pkgs.pkg-config
          pkgs.git
          pkgs.coreutils
          pkgs.gnugrep
          pkgs.gawk
          pkgs.findutils
          pkgs.file
          pkgs.patchelf
          pkgs.util-linux
        ];
        text = ''
          korri_root="''${KORRI_ROOT:-$PWD}"
          while [[ "$korri_root" != / && ! -f "$korri_root/clients/slint-spike/Cargo.toml" ]]; do
            korri_root="$(dirname "$korri_root")"
          done
          if [[ ! -f "$korri_root/clients/slint-spike/Cargo.toml" ]]; then
            echo "Run inside a Korri checkout with clients/slint-spike" >&2
            exit 1
          fi
          export KORRI_ROOT
          KORRI_ROOT="$(cd "$korri_root" && pwd -P)"
          ${proseqlSource.hydrateShell}
          export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
          # SDK binaries use the ordinary x86_64 loader. nix-ld is not required.
          export WEBOS_HOST_LOADER="${pkgs.stdenv.cc.libc}/lib/ld-linux-x86-64.so.2"
          export WEBOS_HOST_LIBRARIES="${
            lib.makeLibraryPath [
              pkgs.stdenv.cc.cc.lib
              pkgs.stdenv.cc.libc
              pkgs.zlib
              pkgs.expat
            ]
          }"
          exec bash "$KORRI_ROOT/clients/slint-spike/webos-cross.sh" ${action} "$@"
        '';
      };
    in
    {
      type = "app";
      program = "${program}/bin/slint-spike-webos-${action}";
    };
in
{
  apps = {
    slint-spike-webos-probe = makeApp "probe";
    slint-spike-webos-korrid = makeApp "korrid";
    slint-spike-webos-build = makeApp "build";
  };
  help = ''

    nix run .#slint-spike-webos-probe
        Cross-compile a tiny ARM ELF and inspect it, without target execution.
    nix run .#slint-spike-webos-korrid
        Cross-build the complete current korrid library, at most 2 jobs.
    nix run .#slint-spike-webos-build
        Cross-build the Slint+SDL spike after the backend adapter is present.'';
}
