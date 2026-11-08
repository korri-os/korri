# Nix tasks for the native shell (clients/shell). Like nix/tasks.nix, each app
# runs cargo in the checkout, with the shell's own target directory.
{ pkgs }:
let
  lib = pkgs.lib;
  package = import ./package.nix { inherit pkgs; };
  # winit's Wayland/X11 backends and fontconfig load these at run time.
  runtimeLibraries = with pkgs; [
    wayland
    libxkbcommon
    libx11
    libxcursor
    libxrandr
    libxi
    libxcb
    fontconfig.lib
  ];
  buildInputs = [
    pkgs.cargo
    pkgs.rustc
    pkgs.clippy
    pkgs.rustfmt
    pkgs.stdenv.cc
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
          while [[ "$korri_root" != / && ! -f "$korri_root/clients/shell/Cargo.toml" ]]; do
            korri_root="$(dirname "$korri_root")"
          done
          if [[ ! -f "$korri_root/clients/shell/Cargo.toml" ]]; then
            echo "Korri checkout with clients/shell not found; run inside it" >&2
            exit 1
          fi
          KORRI_ROOT="$(cd "$korri_root" && pwd -P)"
          export KORRI_ROOT
          export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/shell-target"
          export CARGO_BUILD_JOBS="''${CARGO_BUILD_JOBS:-4}"
          export LD_LIBRARY_PATH="${lib.makeLibraryPath runtimeLibraries}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
          crate="$KORRI_ROOT/clients/shell"
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
  inherit package;
  # The surface packages, verified at build time (nix/installed.nix).
  surfaces = import ./nix/installed.nix { inherit pkgs; };
  moduleCheck =
    { nixpkgs, korri }:
    import ./nix/module-check.nix {
      inherit pkgs nixpkgs korri;
    };
  freezerCheck = korrid: import ./nix/freezer-vm-test.nix { inherit pkgs korrid; };

  help = ''

    nix run .#shell-check
        Native shell: fmt, clippy (desktop and webos builds), all tests with the fixture korrid.
    nix run .#shell-run -- ARGS
        Run korri-shell from the checkout (needs korrid's credential and address; see clients/shell/README.md).
    nix run .#shell-proof -- SCENARIO OUT_DIR [GAMES]
        The real window under Xvfb over the fixture korrid: cpu, switch, large, tour or perf.'';

  apps = {
    shell-check = makeApp {
      name = "shell-check";
      description = "Check the native shell.";
      script = ''
        cd "$crate"
        cargo fmt --check
        (cd window-sdl && cargo fmt --check)
        trace="$KORRI_ROOT/packages/input-trace/Cargo.toml"
        cargo fmt --manifest-path "$trace" --check
        cargo clippy --manifest-path "$trace" --locked --all-targets -- -D warnings
        cargo test --manifest-path "$trace" --locked
        cargo clippy --locked --release --all-targets --features fixture -- -D warnings
        cargo clippy --locked --release --no-default-features --features webos -- -D warnings
        cargo test --locked --release --features fixture
      '';
    };
    shell-run = makeApp {
      name = "shell-run";
      description = "Run the native shell from the checkout.";
      script = ''
        exec cargo run --locked --release --bin korri-shell --manifest-path "$crate/Cargo.toml" -- "$@"
      '';
    };
    shell-proof = makeApp {
      name = "shell-proof";
      description = "Workstation proof of the native shell under Xvfb.";
      extraInputs = [
        pkgs.xorg-server
        pkgs.xdotool
        pkgs.imagemagick
        pkgs.gnugrep
        pkgs.gawk
        pkgs.gnused
        pkgs.perf
      ];
      script = ''
        if [[ $# -lt 2 ]]; then
          echo "usage: shell-proof SCENARIO OUT_DIR [GAMES]" >&2
          exit 2
        fi
        cargo build --locked --release --manifest-path "$crate/Cargo.toml" --features fixture --bins --examples
        exec bash "$crate/proof/run.sh" "$CARGO_TARGET_DIR/release/korri-shell" \
          "$CARGO_TARGET_DIR/release/examples/fixture-korrid" "$2" "$1" "''${3:-12}"
      '';
    };
  };
}
