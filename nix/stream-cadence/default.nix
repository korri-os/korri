{ pkgs }:
let
  source = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      # Baselines are measurement data. A new baseline must not rebuild the tool.
      type == "directory" && !(builtins.elem (builtins.baseNameOf path) [ "__pycache__" "baselines" ])
      || type == "regular" && !(pkgs.lib.hasSuffix ".pyc" path);
  };
  # korrid's own picture-path table. viewer.py holds a copy of it.
  korridViewer = ../../services/korrid/src/linux_viewer/production.rs;
  # The gate that the app build depends on: no device constant in production
  # source, and no difference between viewer.py's copy and korrid's table.
  sourceCheck =
    pkgs.runCommand "stream-cadence-source-check"
      {
        nativeBuildInputs = [ pkgs.python3 ];
        CADENCE_KORRID_VIEWER = korridViewer;
      }
      ''
        export PYTHONDONTWRITEBYTECODE=1
        python3 -m unittest discover -s ${source}/tests -p test_source.py -v
        python3 -m unittest discover -s ${source}/tests -p test_korrid_copy.py -v
        touch "$out"
      '';
  deviceSource = pkgs.lib.fileset.toSource {
    root = ./.;
    fileset = pkgs.lib.fileset.unions [
      ./device.py
      ./discovery.py
      ./heat.py
      ./sender.py
      ./viewer.py
      ./fixture_namespace.py
    ];
  };
  captureSource = pkgs.lib.fileset.toSource {
    root = ./.;
    fileset = pkgs.lib.fileset.unions [
      ./capture.c
      ./frame-bar.h
    ];
  };
  # The viewer-side half of the outside measure: a wlr-screencopy client.
  capture = pkgs.stdenv.mkDerivation {
    pname = "stream-cadence-capture";
    version = "1";
    src = captureSource;
    nativeBuildInputs = [ pkgs.wayland-scanner ];
    buildInputs = [ pkgs.wayland ];
    dontConfigure = true;
    buildPhase = ''
      runHook preBuild
      protocol=${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-screencopy-unstable-v1.xml
      wayland-scanner client-header "$protocol" wlr-screencopy-unstable-v1-client-protocol.h
      wayland-scanner private-code "$protocol" wlr-screencopy-unstable-v1-protocol.c
      # frame-bar.h supplies BAR_BITS; its painter belongs to the motion source.
      $CC -std=c11 -Wall -Wextra -Werror -Wno-unused-function -O2 -I. capture.c wlr-screencopy-unstable-v1-protocol.c \
        -o stream-cadence-capture -lwayland-client
      runHook postBuild
    '';
    installPhase = ''
      mkdir -p "$out/bin"
      cp stream-cadence-capture "$out/bin/"
    '';
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
          pkgs.alsa-utils
          pkgs.openssh
          pkgs.stdenv.cc
          pkgs.openssl
          pkgs.python3Packages.ruff
          pkgs.python3Packages.mypy
        ];
        # The same check runs in sourceCheck; here it runs with all other tests.
        CADENCE_KORRID_VIEWER = korridViewer;
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
    capture
    device
    tests
    sourceCheck
    ;
  runtime = {
    inherit motion capture;
    python = pkgs.python3;
    source = deviceSource;
    # aka is server-only and has no viewer plugin. The fixture copies this one.
    viewer = pkgs.moonlight-embedded;
    # The sender fixture's hidden compositor and its stock Sunshine. A host with
    # no Korri also takes this sway for the viewer fixture.
    sway = pkgs.sway-unwrapped;
    sunshine = pkgs.sunshine;
  };
}
