# Workstation-only webOS M2 tasks. No installation, SSH or device access.
{ pkgs, proseql }:
let
  lib = pkgs.lib;
  media = import ./media.nix { inherit pkgs; };
  # constants.relays does not force the product's korri argument (#315).
  productRelays = pkgs.writeText "korri-product-relays.json" (
    builtins.toJSON (import ../../../nix/product/requirements.nix { korri = null; }).constants.relays
  );
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
    export CARGO_BUILD_JOBS="''${CARGO_BUILD_JOBS:-2}"
    export LD_LIBRARY_PATH="${
      lib.makeLibraryPath [
        pkgs.SDL2
        pkgs.fontconfig.lib
      ]
    }''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    mkdir -p "$CARGO_TARGET_DIR/release"
    cc -std=c11 -Wall -Wextra -Werror -O2 "$crate/webos/launcher.c" -o "$CARGO_TARGET_DIR/release/korri-webos"
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
          export KORRI_WEBOS_MEDIA="${media}"
          exec bash "$crate/../slint-spike/webos-cross.sh" shell-build "$@"
        '';
    shell-webos-package = makeApp "shell-webos-package" [ pkgs.SDL2 pkgs.fontconfig.lib ] ''
      ${desktop}
      exec python3 "$crate/webos/package.py" --verifier "$CARGO_TARGET_DIR/release/korri-shell" --product-relays "${productRelays}" "$@"
    '';
    # Hidden dev-only operator tool. Deliberately absent from product help.
    shell-webos-owner-sign = makeApp "shell-webos-owner-sign" [ ] ''
      nix develop "$KORRI_ROOT#korrid" --command cargo build --locked \
        --manifest-path "$crate/../../services/korrid/Cargo.toml" --bin webos-owner-sign >&2
      exec "$KORRI_ROOT/.cache/korrid-target/debug/webos-owner-sign" "$@"
    '';
    shell-webos-package-check = makeApp "shell-webos-package-check" [ pkgs.bubblewrap ] ''
      exec python3 "$crate/webos/package_test.py" --product-relays "${productRelays}" "$@"
    '';
    shell-webos-real-proof =
      makeApp "shell-webos-real-proof"
        [
          pkgs.SDL2
          pkgs.fontconfig.lib
          pkgs.xorg-server
          pkgs.xdotool
          pkgs.imagemagick
          pkgs.bubblewrap
          pkgs.diffutils
          pkgs.findutils
        ]
        ''
          ${desktop}
          export CARGO_BUILD_JOBS="''${CARGO_BUILD_JOBS:-1}"
          nix develop "$KORRI_ROOT#korrid" --command cargo build --locked \
            --manifest-path "$crate/../../services/korrid/Cargo.toml" --bin webos-korrid
          out="$(realpath -m "''${1:?output directory}")"
          mkdir -p "$out"
          # Preserve the real binary's fixed TV paths without touching host /media.
          # No network or host D-Bus access; fresh private state and real loopback.
          exec bwrap --unshare-user --unshare-net --unshare-pid --ro-bind / / \
            --proc /proc --dev /dev --tmpfs /tmp --tmpfs /run --tmpfs /media --dir /media/developer \
            --bind "$out" "$out" -- \
            bash "$crate/webos/proof.sh" "$CARGO_TARGET_DIR/release" "$out" \
              "$KORRI_ROOT/.cache/korrid-target/debug/webos-korrid" "${productRelays}"
        '';
    shell-webos-input-check =
      makeApp "shell-webos-input-check" [ pkgs.SDL2 pkgs.xorg-server pkgs.pkg-config ]
        ''
          export CARGO_BUILD_JOBS="''${CARGO_BUILD_JOBS:-2}"
          export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/shell-webos-desktop-target"
          export LD_LIBRARY_PATH="${
            lib.makeLibraryPath [
              pkgs.SDL2
              pkgs.fontconfig.lib
            ]
          }''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
          export PKG_CONFIG_PATH="${
            lib.makeSearchPath "lib/pkgconfig" [
              pkgs.fontconfig.dev
              pkgs.freetype.dev
              pkgs.expat.dev
              pkgs.zlib.dev
              pkgs.bzip2.dev
              pkgs.libpng.dev
              pkgs.brotli.dev
            ]
          }"
          work=$(mktemp -d)
          Xvfb -displayfd 3 -screen 0 640x480x24 -nolisten tcp 3>"$work/display" >"$work/xvfb.log" 2>&1 &
          xvfb=$!
          trap 'kill "$xvfb" 2>/dev/null || true; wait "$xvfb" 2>/dev/null || true; rm -rf "$work"' EXIT
          for _ in $(seq 100); do
            test ! -s "$work/display" || break
            sleep 0.1
          done
          test -s "$work/display"
          DISPLAY=":$(cat "$work/display")"
          export DISPLAY
          unset WAYLAND_DISPLAY
          cargo test --locked --release --manifest-path "$crate/window-sdl/Cargo.toml" \
            input_pump_tests::native_frame_and_sdl_key_reach_the_same_presented_frame -- --ignored --nocapture
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
        Test packaging or inspect the assembled M2 IPK.
    nix run .#shell-webos-real-proof -- OUTPUT
        Prove the real brain, shipped launcher and SDL shell reach Zone in an isolated workstation jail.
    nix run .#shell-webos-input-check
        Check that native frames and SDL keys reach the same presented frame under Xvfb.
    nix run .#shell-webos-proof -- OUTPUT
        Run the webOS SDL shell at 1920x1080 under Xvfb; capture Zone and Pico.
  '';
}
