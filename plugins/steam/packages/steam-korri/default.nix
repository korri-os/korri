{ pkgs, proton }:
let
  guard = pkgs.stdenv.mkDerivation {
    pname = "korri-steam-input-guard";
    version = "1";
    src = ./src/steam-input-guard.c;
    dontUnpack = true;
    buildPhase = ''
      $CC -shared -fPIC -Wall -Wextra -Werror -O2 \
        -o libkorri-steam-input-guard.so "$src" -ldl
    '';
    installPhase = ''
      install -Dm755 libkorri-steam-input-guard.so "$out/lib/libkorri-steam-input-guard.so"
    '';
  };
  resources = pkgs.runCommand "steam-native-resources" { } ''
    mkdir -p "$out/share"
    cp -r ${./resources}/. "$out/share/"
  '';
  helpers = pkgs.stdenvNoCC.mkDerivation {
    pname = "steam-arm64-helpers";
    version = "1";
    src = ./scripts;
    dontConfigure = true;
    dontBuild = true;
    nativeBuildInputs = [ pkgs.makeWrapper ];
    installPhase = ''
      mkdir -p "$out/bin" "$out/share/steam-rocknix-bootstrap/resources"
      cp ${resources}/share/{compatibilitytool,registry}.vdf "$out/share/steam-rocknix-bootstrap/resources/"
      for helper in steam-arm64-seed steam-arm64-bootstrap steam-guest-run; do
        install -m755 "$helper" "$out/bin/$helper"
        patchShebangs "$out/bin/$helper"
      done
      substituteInPlace "$out/bin/steam-arm64-seed" \
        --replace-fail '@korriProtonCachyosArm64@' '${proton}/share/korri/proton-cachyos-arm64/dist'
      substituteInPlace "$out/bin/steam-guest-run" \
        --replace-fail '@inputGuard@' '${guard}/lib/libkorri-steam-input-guard.so'
      wrapProgram "$out/bin/steam-arm64-seed" \
        --prefix PATH : ${
          pkgs.lib.makeBinPath (
            with pkgs;
            [
              bash
              binutils
              coreutils
              curl
              findutils
              gnugrep
              gnutar
              gzip
              unzip
              xz
            ]
          )
        }
      wrapProgram "$out/bin/steam-arm64-bootstrap" \
        --prefix PATH : ${
          pkgs.lib.makeBinPath (
            with pkgs;
            [
              bash
              coreutils
              curl
              gnugrep
            ]
          )
        }
      wrapProgram "$out/bin/steam-guest-run" \
        --prefix PATH : ${
          pkgs.lib.makeBinPath (
            with pkgs;
            [
              bash
              coreutils
            ]
          )
        }
    '';
  };
  fhs = import ./fhs.nix { inherit pkgs helpers; };
in
{
  inherit
    guard
    resources
    helpers
    fhs
    ;
}
