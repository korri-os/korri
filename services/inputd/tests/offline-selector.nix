# Standalone CLI test: deliberately not part of host activation or deployment.
{ pkgs, inputdPackage }:
let
  components = pkgs.runCommand "offline-selector-components" { } ''
    mkdir -p "$out/bin" "$out/share/inputplumber/profiles"
    for name in inputplumber korri-inputd korri-input-seat-receiver korrid; do
      cp ${pkgs.coreutils}/bin/true "$out/bin/$name"
    done
    echo fixture > "$out/share/inputplumber/profiles/korri-60-xbox_one_gamepad.yaml"
  '';
  bundle =
    name:
    pkgs.runCommand "offline-selector-${name}" { } ''
      mkdir -p "$out/bin" "$out/share"
      for name in inputplumber korri-inputd korri-input-seat-receiver korrid; do
        ln -s ${components}/bin/$name "$out/bin/$name"
      done
      ln -s ${components}/share/inputplumber "$out/share/inputplumber"
      ln -s ${components}/share/inputplumber/profiles/korri-60-xbox_one_gamepad.yaml "$out/share/korri-input-profile"
    '';
  invalid = pkgs.runCommand "offline-selector-invalid" { } ''mkdir "$out"'';
  checks = pkgs.writeText "offline-selector-cli.py" (builtins.readFile ./offline-selector-cli.py);
in
pkgs.testers.runNixOSTest {
  name = "korri-bundle-offline-selector";
  nodes.machine =
    { ... }:
    {
      virtualisation.memorySize = 1024;
      environment.etc."offline-selector-test-vm".text = "isolated-selector-fixture-only\n";
      environment.systemPackages = [
        pkgs.python3
        pkgs.strace
        pkgs.util-linux
      ];
    };
  testScript = ''
    machine.start()
    machine.wait_for_unit("multi-user.target")
    print(machine.succeed("python3 ${checks} ${inputdPackage}/bin/korri-bundle-select ${bundle "first"} ${bundle "second"} ${invalid}"))
    for trace in ["root-mode-775", "root-mode-757", "root-mode-777", "root-owner", "hard-linked-previous", "hard-linked-alias", "success", "rename", "fsync", "race", "reopen"]:
        machine.copy_from_vm(f"/tmp/{trace}.trace")
    machine.reboot()
    machine.wait_for_shutdown()
    machine.start()
    machine.wait_for_unit("multi-user.target")
    machine.succeed("test $(readlink /nix/var/nix/gcroots/korri-bundle/active) = ${bundle "second"}")
    machine.succeed("test $(readlink /nix/var/nix/gcroots/korri-bundle/previous) = ${bundle "second"}")
  '';
}
