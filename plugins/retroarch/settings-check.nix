# The kind checks the caller's program, not a default RetroArch source. Source
# unpack/patch hooks are inherited from that exact derivation. The native cfg
# parser is compiled and executed on the build machine, never on the target.
# Plugin TS is evaluated for tests only; installed plugin code stays source.
{ pkgs, program }:
program.overrideAttrs (old: {
  pname = "retroarch-source-checked-settings";
  outputs = [ "out" ];
  nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [
    pkgs.buildPackages.python3
    pkgs.buildPackages.bun
  ];
  phases = [
    "unpackPhase"
    "patchPhase"
    "installPhase"
  ];
  installPhase = ''
    CC=${pkgs.buildPackages.stdenv.cc}/bin/cc bash ${./build-config-parser.sh} \
      "$PWD" ${./config-parser-probe.c} "$PWD/config-parser-probe"
    cp ${./plugin.ts} plugin.ts
    cp ${./plugin.test.ts} plugin.test.ts
    KORRI_TEST_RETROARCH_CONFIG_PARSER="$PWD/config-parser-probe" bun test ./plugin.test.ts
    cp ${./check-settings.py} check-settings.py
    cp ${./test_check_settings.py} test_check_settings.py
    python3 test_check_settings.py
    python3 ${./check-settings.py} \
      --configuration configuration.c \
      --table ${./settings-types.json} \
      --callback ${./plugin.ts} \
      --program ${pkgs.lib.escapeShellArg "${program}/bin/retroarch"} \
      --version ${pkgs.lib.escapeShellArg program.version} \
      --output "$out"
  '';
})
