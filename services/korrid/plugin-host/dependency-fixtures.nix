# Build-machine fixtures, not a shipped catalogue or emulator. The runtime
# owns its executable and source; the pack requires that independent plugin.
{ pkgs, mkPlugin }:
let
  runnerSource = pkgs.runCommand "dependency-runner-source" { } ''
    mkdir -p "$out"
    cat > "$out/plugin.ts" <<'EOF'
    export { handlers } from "./launch.ts";
    export const name = "fake08";
    export const runners = { fake08: { id: "@runtime:fake08/fake08", program: "fake08" } };
    EOF
    echo 'export const handlers = { "launch.prepare": (input: unknown) => input };' > "$out/launch.ts"
  '';
  packSource = pkgs.writeTextDir "plugin.ts" ''export const name = "starter-pack";'';
  program = pkgs.writeShellScriptBin "fixture-fake08" ''exit 0'';
  runner = mkPlugin {
    publisher.namespace = "@runtime";
    source = runnerSource;
    plugin = _: { files.fake08 = "${program}/bin/fixture-fake08"; };
  };
  secondRunner = mkPlugin {
    publisher.namespace = "@runtime";
    source = runnerSource;
    plugin = _: {
      files.fake08 = "${program}/bin/fixture-fake08";
      files.evidence = "${pkgs.writeTextDir "evidence" "different exact build"}/evidence";
    };
  };
  pack = mkPlugin {
    publisher.namespace = "@games";
    source = packSource;
    plugin = _: { requires = [ runner ]; };
  };
  transitive = mkPlugin {
    publisher.namespace = "@games";
    source = pkgs.writeTextDir "plugin.ts" ''export const name = "collection";'';
    plugin = _: { requires = [ pack ]; };
  };
  conflict = mkPlugin {
    publisher.namespace = "@games";
    source = packSource;
    plugin = _: {
      requires = [
        runner
        secondRunner
      ];
    };
  };
  cycle = pkgs.runCommand "self-requiring-plugin" { nativeBuildInputs = [ pkgs.jq ]; } ''
    mkdir -p "$out"
    cp ${pack}/plugin.ts "$out/plugin.ts"
    jq --arg output "$out" '.requires = [$output]' ${pack}/manifest.json > "$out/manifest.json"
  '';
  native = mkPlugin {
    publisher.namespace = "@runtime";
    source = pkgs.writeTextDir "plugin.ts" ''export const name = "native"; export const services = ["first", "second"];'';
    plugin = _: {
      services.first = pkgs.writeText "first.service" ''
        [Service]
        Type=exec
        ExecStart=${pkgs.coreutils}/bin/sleep infinity
        CapabilityBoundingSet=CAP_NET_RAW
      '';
      services.second = pkgs.writeText "second.service" ''
        [Service]
        Type=exec
        ExecStart=${pkgs.coreutils}/bin/sleep infinity
        User=root
      '';
    };
  };
  nativePack = mkPlugin {
    publisher.namespace = "@games";
    source = packSource;
    plugin = _: { requires = [ native ]; };
  };
in
{
  inherit
    runner
    secondRunner
    pack
    transitive
    conflict
    cycle
    native
    nativePack
    ;
}
