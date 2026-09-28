# Focused off-device proof. The binary includes the actual fully patched
# input.cpp and upstream task pool; only platform OS injection is stubbed.
# Tests require real root SO_PEERCRED and run in a VM, not under fakeroot.
{ pkgs }:
let
  approved = import ./approved-patches.nix;
  records = approved.patches;
  material =
    builtins.concatStringsSep "\n" (map (record: "${record.name} ${record.sha256}") records) + "\n";
  source = pkgs.applyPatches {
    name = "sunshine-korri-input-presence-source";
    src = pkgs.sunshine.src;
    patches = map (record: record.path) records;
  };
  binary =
    pkgs.runCommand "sunshine-input-seat-presence-regression"
      {
        nativeBuildInputs = [ pkgs.stdenv.cc ];
        buildInputs = [
          pkgs.boost
          pkgs.nlohmann_json
          pkgs.ffmpeg.dev
        ];
      }
      ''
        mkdir -p "$out/bin"
        c++ -std=c++23 -O1 -g -pthread -DBOOST_LOG_DYN_LINK \
          -ffunction-sections -fdata-sections -Wl,--gc-sections \
          -I${source} -I${source}/third-party \
          ${./test-input-seat-presence.cpp} \
          -lboost_log -lboost_thread -lboost_filesystem \
          -o "$out/bin/sunshine-input-seat-presence-regression"
      '';
  vm = pkgs.testers.runNixOSTest {
    name = "sunshine-input-seat-presence";
    nodes.machine = {
      environment.systemPackages = [ binary ];
      virtualisation.memorySize = 1024;
    };
    testScript = ''
      machine.start()
      machine.wait_for_unit("multi-user.target")
      machine.succeed("sunshine-input-seat-presence-regression", timeout=60)
    '';
  };
in
assert pkgs.sunshine.version == approved.baseSunshineVersion;
assert pkgs.sunshine.src.outputHash == approved.approvedBaseSourceHash;
assert builtins.all (
  record:
  record.name == builtins.baseNameOf record.path
  && record.sha256 == builtins.hashFile "sha256" record.path
) records;
assert builtins.hashString "sha256" material == approved.patchSetSha256;
{
  inherit source binary vm;
}
