# Off-device evidence for separate-fd injection into the exclusive evdev reader.
{ pkgs, probe }:
let
  name = "korri-native-grab-proof";
  guarded = pkgs.writeShellScript "korri-native-grab-proof" ''
    set -euo pipefail
    test "$(cat /etc/hostname)" = ${name}
    test "$(cat /sys/class/dmi/id/product_name)" = ${name}
    test "$(cat /etc/${name}-vm)" = isolated-kernel-proof-only
    exec ${probe.grabDeliveryTest}/bin/korri-native-grab-delivery-test --isolated-vm
  '';
in
pkgs.testers.runNixOSTest {
  inherit name;
  nodes.machine = {
    networking.hostName = name;
    environment.etc."${name}-vm".text = "isolated-kernel-proof-only\n";
    boot.kernelModules = [ "uinput" ];
    virtualisation = {
      memorySize = 1024;
      qemu.options = [ "-smbios type=1,product=${name}" ];
    };
  };
  testScript = ''
    machine.start()
    machine.wait_for_unit("multi-user.target")
    machine.succeed("test -c /dev/uinput")
    print(machine.succeed("${guarded}", timeout=30))
  '';
}
