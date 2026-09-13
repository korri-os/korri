# The builder must refuse a file that names a store output rather than a path
# inside one. The device's immutable_path rule refuses the same thing at
# inspection; catching it here keeps the failure on the author's machine.
{ pkgs }:
let
  mkPlugin = import ./builder.nix { inherit pkgs; };
  publisher.namespace = "@korri";
  source = pkgs.writeText "plugin.ts" "export const name = \"check\"\n";
  bareFile = pkgs.writeText "bare-evidence" "{}";
  bare = mkPlugin {
    inherit publisher source;
    plugin = _: { files.evidence = bareFile; };
  };
  # A builder assertion is an evaluation failure, which tryEval can observe.
  bareRefused = !(builtins.tryEval (builtins.seq bare.outPath true)).success;
  nested = mkPlugin {
    inherit publisher source;
    plugin = _: { files.evidence = "${pkgs.writeTextDir "settings.json" "{}"}/settings.json"; };
  };
in
assert pkgs.lib.assertMsg bareRefused "builder accepted a bare store output as a file";
pkgs.runCommand "korri-plugin-builder-check" { } ''
  test -f ${nested}/manifest.json
  grep -q '/settings.json' ${nested}/manifest.json
  touch "$out"
''
