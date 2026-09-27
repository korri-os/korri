{ pkgs }:
# This variant alone gets the CDP probe. The production shell stays unchanged.
(import ../package.nix { inherit pkgs; }).overrideAttrs (old: {
  pname = "korri-portal-input-probe";
  patches = (old.patches or [ ]) ++ [ ./shell.patch ];
  postPatch = (old.postPatch or "") + ''
    cp ${./input_diagnostic.rs} src/input_diagnostic.rs
  '';
  postInstall = (old.postInstall or "") + ''
    $CC -std=c11 -O2 -Wall -Wextra -Werror ${./replay.c} -o $out/bin/korri-replay-dpad
    mkdir -p $out/lib
    $CC -std=c11 -O2 -Wall -Wextra -Werror -fPIC -shared ${./input_trace.c} -ldl -o $out/lib/input-trace.so
    $CC -std=c11 -O2 -Wall -Wextra -Werror -DTRACE_TEST ${./input_trace.c} -ldl -o input-trace-test
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    $out/bin/korri-replay-dpad --self-test
    ./input-trace-test
  '';
})
