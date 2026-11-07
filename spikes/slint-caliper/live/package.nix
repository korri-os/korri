# THROWAWAY SPIKE `live` (README.md here). Workstation tool; the output is
# portable through nix copy.
#
#   nix build .#slint-caliper-live
#   korri-live [--surface zone|pico] [--device rg35xxsp|odin2portal|...]
#   korri-live-proof OUT_DIR          (inside a Korri checkout, under Xvfb)
#
# Inside a Korri checkout korri-live edits that checkout's packages. Anywhere
# else it copies the packages it was built with to
# ${XDG_STATE_HOME:-~/.local/state}/korri-live/<build>/ and watches that copy.
#
# Three layers, so that a package or proof edit does not rebuild the Rust.
{ pkgs }:
let
  lib = pkgs.lib;
  root = ../../..;
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
  fonts = pkgs.makeFontsConf { fontDirectories = [ pkgs.dejavu_fonts ]; };
  proofTools = with pkgs; [
    xorg-server
    xdotool
    xwd
    imagemagick
    python3
    bun
    diffutils
    gnused
    gnugrep
    coreutils
  ];

  binaries = pkgs.rustPlatform.buildRustPackage {
    pname = "slint-caliper-live-bin";
    version = "0.0.0";
    src = lib.fileset.toSource {
      inherit root;
      fileset = lib.fileset.unions [
        ./Cargo.toml
        ./Cargo.lock
        ./src
        ./ui
        ../../../clients/shell/Cargo.toml
        ../../../clients/shell/src
        ../../../clients/shell/examples
        ../../../clients/shell/tests
        ../../../clients/shell/fixtures
        ../../../clients/shell/vendor
        ../../../clients/shell/window-sdl
        ../../../contracts/surface/Cargo.toml
        ../../../contracts/surface/src
      ];
    };
    cargoRoot = "spikes/slint-caliper/live";
    buildAndTestSubdir = "spikes/slint-caliper/live";
    cargoLock.lockFile = ./Cargo.lock;
    doCheck = false;
    nativeBuildInputs = [ pkgs.patchelf ];
    postFixup = ''
      patchelf --add-rpath "${lib.makeLibraryPath runtimeLibraries}" $out/bin/korri-live
    '';
  };

  # What korri-live needs beside it when no checkout is near: the packages, the
  # transpile scripts (package.json), the shell's fixture model and the treaty
  # types the packages import.
  share = pkgs.runCommand "slint-caliper-live-share" { } ''
    mkdir -p $out/clients/shell/fixtures $out/contracts/generated
    cp -r ${../../../clients/shell/packages} $out/clients/shell/packages
    cp ${../../../clients/shell/package.json} $out/clients/shell/package.json
    cp ${../../../clients/shell/fixtures/model.json} $out/clients/shell/fixtures/model.json
    cp ${../../../contracts/generated/surface.ts} $out/contracts/generated/surface.ts
  '';
in
pkgs.runCommand "slint-caliper-live"
  {
    nativeBuildInputs = [ pkgs.makeWrapper ];
    meta = {
      description = "THROWAWAY SPIKE: a Korri surface package live, with fixture data and live reload";
      mainProgram = "korri-live";
      platforms = [
        "x86_64-linux"
        "aarch64-linux"
      ];
    };
  }
  ''
    mkdir -p $out/bin $out/libexec
    makeWrapper ${binaries}/bin/korri-live $out/libexec/korri-live \
      --set KORRI_LIVE_FIXTURE ${binaries}/bin/fixture-korrid \
      --set-default FONTCONFIG_FILE ${fonts} \
      --prefix PATH : ${lib.makeBinPath [ pkgs.bun ]}
    cat > $out/bin/korri-live <<EOF
    #!${pkgs.runtimeShell}
    # Use the checkout above the working directory, or a writable copy of the built packages.
    set -eu
    for arg in "\$@"; do [ "\$arg" = --packages ] && exec $out/libexec/korri-live "\$@"; done
    dir=\$PWD
    while [ "\$dir" != / ] && [ ! -d "\$dir/clients/shell/packages" ]; do dir=\$(dirname "\$dir"); done
    [ -d "\$dir/clients/shell/packages" ] && exec $out/libexec/korri-live "\$@"
    copy=\''${XDG_STATE_HOME:-\$HOME/.local/state}/korri-live/$(basename ${share})
    if [ ! -d "\$copy" ]; then
      mkdir -p "\$copy"
      cp -r ${share}/. "\$copy/"
      chmod -R u+w "\$copy"
    fi
    echo "korri-live: no checkout here; editing the copy in \$copy/clients/shell/packages" >&2
    exec $out/libexec/korri-live --packages "\$copy/clients/shell/packages" "\$@"
    EOF
    chmod +x $out/bin/korri-live
    install -m755 ${./proof.sh} $out/libexec/proof.sh
    install -m755 ${./probe.py} $out/libexec/probe.py
    makeWrapper $out/libexec/proof.sh $out/bin/korri-live-proof \
      --prefix PATH : ${lib.makeBinPath proofTools} \
      --add-flags $out/bin/korri-live
  ''
