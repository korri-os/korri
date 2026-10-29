# THROWAWAY SPIKE: Nix apps for the native Slint inspector over real korrid.
# Like nix/tasks.nix, each app runs cargo in the checkout. The target dir is
# the spike's own, so the shared .cache/korrid-target is never written.
{ pkgs, proseql }:
let
  lib = pkgs.lib;
  proseqlSource = import ../../services/korrid/proseql-source.nix { inherit pkgs proseql; };
  rustToolchain = pkgs.rust-bin.stable.latest.default;
  # winit's X11 backend and Slint's system fonts load these at run time.
  runtimeLibraries = with pkgs; [
    libx11
    libxcursor
    libxrandr
    libxi
    libxcb
    libxkbcommon
    fontconfig.lib
    freetype
  ];
  # Build-time native inputs: the linker, bindgen for rquickjs, and
  # pkg-config for yeslogic-fontconfig-sys (Slint system fonts).
  pkgConfigPath = lib.makeSearchPathOutput "dev" "lib/pkgconfig" [
    pkgs.fontconfig
    pkgs.freetype
  ];
  buildInputs = [
    rustToolchain
    pkgs.stdenv.cc
    pkgs.llvmPackages.libclang
    pkgs.pkg-config
    pkgs.git
    pkgs.coreutils
    pkgs.bash
  ];
  makeApp =
    {
      name,
      description,
      extraInputs ? [ ],
      script,
    }:
    let
      program = pkgs.writeShellApplication {
        inherit name;
        runtimeInputs = buildInputs ++ extraInputs;
        text = ''
          # ${description}
          korri_root="''${KORRI_ROOT:-$PWD}"
          while [[ "$korri_root" != / && ! -f "$korri_root/clients/slint-spike/Cargo.toml" ]]; do
            korri_root="$(dirname "$korri_root")"
          done
          if [[ ! -f "$korri_root/clients/slint-spike/Cargo.toml" ]]; then
            echo "Korri checkout with clients/slint-spike not found; run inside it" >&2
            exit 1
          fi
          KORRI_ROOT="$(cd "$korri_root" && pwd -P)"
          export KORRI_ROOT
          ${proseqlSource.hydrateShell}
          export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/slint-spike-target"
          export CARGO_BUILD_JOBS="''${CARGO_BUILD_JOBS:-2}"
          export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
          export BINDGEN_EXTRA_CLANG_ARGS="-isystem ${pkgs.stdenv.cc.libc.dev}/include"
          export PKG_CONFIG_PATH="${pkgConfigPath}"
          export LD_LIBRARY_PATH="${lib.makeLibraryPath runtimeLibraries}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
          # Existing CLI stamp name (clients/cli/package.nix); no secret in it.
          rev="$(git -C "$KORRI_ROOT" rev-parse --short=12 HEAD)"
          if [[ -n "$(git -C "$KORRI_ROOT" status --porcelain)" ]]; then rev="$rev-dirty"; fi
          export KORRI_BUILD_ID="$rev"
          crate="$KORRI_ROOT/clients/slint-spike"
          ${script}
        '';
      };
    in
    {
      type = "app";
      program = "${program}/bin/${name}";
    };
in
{
  help = ''

    nix run .#slint-spike
        THROWAWAY SPIKE: native Slint window over a scratch embedded korrid (needs DISPLAY).
    nix run .#slint-spike-build
        Build the spike and its tests (2 jobs unless CARGO_BUILD_JOBS is set).
    nix run .#slint-spike-check
        Spike fmt, clippy, unit tests, and the Xvfb self-test under a network-syscall audit.
    nix run .#slint-spike-proof -- OUTPUT_DIR
        Spike screenshots of the real X11 window under Xvfb at the size ladder.'';

  apps = {
    slint-spike-build = makeApp {
      name = "slint-spike-build";
      description = "Build the spike and its tests without running them.";
      script = ''
        cargo build --locked --manifest-path "$crate/Cargo.toml" --all-targets
        exec cargo test --locked --manifest-path "$crate/Cargo.toml" --no-run
      '';
    };
    slint-spike = makeApp {
      name = "slint-spike";
      description = "Run the native Slint inspector window.";
      script = ''
        exec cargo run --locked --manifest-path "$crate/Cargo.toml" -- "$@"
      '';
    };
    slint-spike-check = makeApp {
      name = "slint-spike-check";
      description = "Check the spike: fmt, clippy, tests, Xvfb self-test with syscall audit.";
      extraInputs = [
        pkgs.xvfb-run
        pkgs.strace
        pkgs.gnugrep
        pkgs.gawk
      ];
      script = ''
        exec bash "$crate/check.sh"
      '';
    };
    slint-spike-proof = makeApp {
      name = "slint-spike-proof";
      description = "Screenshot the real X11 window under Xvfb.";
      extraInputs = [
        pkgs.xorg-server
        pkgs.xdotool
        pkgs.imagemagick
        pkgs.gnugrep
      ];
      script = ''
        exec bash "$crate/proof.sh" "$@"
      '';
    };
  };
}
