{ pkgs }:
let
  source = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      type == "directory" && builtins.baseNameOf path != "__pycache__"
      || type == "regular" && !(pkgs.lib.hasSuffix ".pyc" path);
  };
  sourceCheck =
    pkgs.runCommand "stream-cadence-source-check" { nativeBuildInputs = [ pkgs.python3 ]; }
      ''
        export PYTHONDONTWRITEBYTECODE=1
        python3 -m unittest discover -s ${source}/tests -p test_source.py -v
        touch "$out"
      '';
  deviceSource = pkgs.lib.fileset.toSource {
    root = ./.;
    fileset = pkgs.lib.fileset.unions [
      ./device.py
      ./discovery.py
    ];
  };
  python = pkgs.python3.withPackages (p: [ p.numpy ]);
  weston = pkgs.weston.overrideAttrs (old: {
    pname = "stream-cadence-weston";
    postPatch = (old.postPatch or "") + ''
      cp ${./motion-source.c} clients/presentation-shm.c
      cp ${./frame-bar.h} clients/frame-bar.h
    '';
  });
  motion = pkgs.runCommand "stream-cadence-motion" { } ''
    mkdir -p "$out/bin"
    cp ${weston}/bin/weston-presentation-shm "$out/bin/stream-cadence-motion"
  '';
  shimSource = pkgs.lib.fileset.toSource {
    root = ./.;
    fileset = pkgs.lib.fileset.unions [
      ./client-shim.c
      ./receive-clock.h
    ];
  };
  shim = pkgs.stdenv.mkDerivation {
    pname = "stream-cadence-client-shim";
    version = "1";
    src = pkgs.moonlight-embedded.src;
    dontConfigure = true;
    buildPhase = ''
      runHook preBuild
      $CC -std=c11 -Wall -Wextra -Werror -fPIC -shared \
        -Ithird_party/moonlight-common-c/src -I${shimSource} ${shimSource}/client-shim.c \
        -o client-shim.so -ldl
      runHook postBuild
    '';
    installPhase = ''
      mkdir -p "$out/lib"
      cp client-shim.so "$out/lib/"
    '';
  };
  device = pkgs.writeShellScriptBin "stream-cadence-device" ''
    export PYTHONDONTWRITEBYTECODE=1
    exec ${pkgs.python3}/bin/python3 -u ${deviceSource}/device.py "$@"
  '';
  app = pkgs.writeShellApplication {
    name = "stream-cadence";
    derivationArgs.postCheck = "test -f ${sourceCheck}";
    runtimeInputs = [
      python
      pkgs.openssh
      pkgs.nix
      pkgs.openssl
      pkgs.ffmpeg
      pkgs.moonlight-embedded
    ];
    text = ''
      export PYTHONDONTWRITEBYTECODE=1
      export CADENCE_SHIM=${shim}/lib/client-shim.so
      export CADENCE_NIXPKGS=${pkgs.path}
      exec ${python}/bin/python3 ${source}/cli.py "$@"
    '';
  };
  tests =
    pkgs.runCommand "stream-cadence-tests"
      {
        nativeBuildInputs = [
          python
          pkgs.ffmpeg
          pkgs.openssh
          pkgs.stdenv.cc
          pkgs.python3Packages.ruff
          pkgs.python3Packages.mypy
        ];
      }
      ''
            export PYTHONDONTWRITEBYTECODE=1
            cp -r ${source} source
            chmod -R u+w source
            cd source
            cc -std=c11 -Wall -Wextra -Werror tests/bar-render.c -o bar-render
            cc -std=c11 -Wall -Wextra -Werror tests/receive-clock.c -o receive-clock
        ./receive-clock
        export CADENCE_BAR_RENDER="$PWD/bar-render"
            python3 -m unittest discover -s tests -v
            ruff check --select E9,F .
            mypy --follow-untyped-imports ./*.py
            touch "$out"
      '';
in
{
  inherit
    app
    shim
    motion
    device
    tests
    sourceCheck
    ;
  runtime = {
    inherit motion;
    python = pkgs.python3;
    source = deviceSource;
  };
}
