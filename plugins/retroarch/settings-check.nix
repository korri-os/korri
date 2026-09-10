# The kind checks the caller's program, not a default RetroArch source. Source
# unpack/patch hooks are inherited from that exact derivation. No C compilation
# and no TS transpilation occur in this check.
{ pkgs, program }:
program.overrideAttrs (old: {
  pname = "retroarch-source-checked-settings";
  outputs = [ "out" ];
  nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ pkgs.buildPackages.python3 ];
  phases = [
    "unpackPhase"
    "patchPhase"
    "installPhase"
  ];
  installPhase = ''
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
