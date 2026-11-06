# Workstation-only webOS M2 tasks. No installation, SSH or device access.
{ pkgs, proseql }:
let
  lib = pkgs.lib;
  proseqlSource = import ../../../services/korrid/proseql-source.nix { inherit pkgs proseql; };
  rust = pkgs.rust-bin.stable.latest.default.override {
    targets = [ "armv7-unknown-linux-gnueabi" ];
  };
  makeApp =
    name: extraInputs: script:
    let
      program = pkgs.writeShellApplication {
        inherit name;
        runtimeInputs = [
          rust
          pkgs.stdenv.cc
          pkgs.git
          pkgs.coreutils
          pkgs.python3
          pkgs.binutils
          pkgs.gnugrep
          pkgs.gnused
        ]
        ++ extraInputs;
        text = ''
          root="''${KORRI_ROOT:-$PWD}"
          while [[ "$root" != / && ! -f "$root/clients/shell/Cargo.toml" ]]; do
            root="$(dirname "$root")"
          done
          test -f "$root/clients/shell/Cargo.toml" || { echo 'Run inside Korri' >&2; exit 1; }
          export KORRI_ROOT
          KORRI_ROOT="$(cd "$root" && pwd -P)"
          crate="$KORRI_ROOT/clients/shell"
          ${script}
        '';
      };
    in
    {
      type = "app";
      program = "${program}/bin/${name}";
    };
  desktop = ''
    export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/shell-webos-desktop-target"
    export CARGO_BUILD_JOBS="''${CARGO_BUILD_JOBS:-4}"
    export LD_LIBRARY_PATH="${
      lib.makeLibraryPath [
        pkgs.SDL2
        pkgs.fontconfig.lib
      ]
    }''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    cargo build --locked --release --manifest-path "$crate/Cargo.toml" \
      --no-default-features --features webos,fixture --bin korri-shell --bin fixture-korrid
  '';
in
{
  apps = {
    shell-webos-build =
      makeApp "shell-webos-build"
        [
          pkgs.llvmPackages.libclang
          pkgs.pkg-config
          pkgs.gawk
          pkgs.findutils
          pkgs.file
          pkgs.patchelf
          pkgs.util-linux
        ]
        ''
          ${proseqlSource.hydrateShell}
          export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
          export WEBOS_HOST_LOADER="${pkgs.stdenv.cc.libc}/lib/ld-linux-x86-64.so.2"
          export WEBOS_HOST_LIBRARIES="${
            lib.makeLibraryPath [
              pkgs.stdenv.cc.cc.lib
              pkgs.stdenv.cc.libc
              pkgs.zlib
              pkgs.expat
            ]
          }"
          exec bash "$crate/../slint-spike/webos-cross.sh" shell-build "$@"
        '';
    shell-webos-package = makeApp "shell-webos-package" [ pkgs.SDL2 pkgs.fontconfig.lib ] ''
      ${desktop}
      exec python3 "$crate/webos/package.py" --verifier "$CARGO_TARGET_DIR/release/korri-shell" "$@"
    '';
    shell-webos-package-check = makeApp "shell-webos-package-check" [ ] ''
      exec python3 "$crate/webos/package_test.py" "$@"
    '';
    shell-webos-proof =
      makeApp "shell-webos-proof"
        [
          pkgs.SDL2
          pkgs.fontconfig.lib
          pkgs.xorg-server
          pkgs.xdotool
          pkgs.imagemagick
        ]
        ''
          ${desktop}
          exec bash "$crate/webos/proof.sh" "$CARGO_TARGET_DIR/release" "$@"
        '';
  };
  help = ''

    nix run .#shell-webos-build
        Cross-build the shell, real korrid and explicit M1 fixture fallback for LG B7 (clean checkout).
    nix run .#shell-webos-package -- OUTPUT --source-sha SHA
        Verify surfaces on the workstation, package pinned ARM binaries as org.korri.shell.
    nix run .#shell-webos-package-check -- [--check-ipk IPK]
        Test packaging or inspect the assembled M1 IPK.
    nix run .#shell-webos-proof -- OUTPUT
        Run the webOS SDL shell at 1920x1080 under Xvfb; capture Zone and Pico.
  '';
}
