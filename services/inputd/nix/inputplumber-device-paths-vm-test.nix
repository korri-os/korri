# Off-device kernel/DBus/polkit proof. No host device is passed into the guest.
{
  pkgs,
  module,
  inputdPackage,
  inputplumberKorri,
}:
let
  proofName = "korri-inputplumber-device-paths";
  python = pkgs.python3.withPackages (ps: [ ps.dbus-next ]);
  proof = ../tests/inputplumber-device-paths-vm.py;
in
pkgs.testers.runNixOSTest {
  name = proofName;
  nodes.machine =
    { lib, ... }:
    {
      imports = [ module ];
      networking.hostName = proofName;
      environment.etc."${proofName}-vm".text = "isolated-kernel-proof-only\n";
      environment.systemPackages = [ python ];
      virtualisation = {
        memorySize = 1024;
        cores = 2;
        qemu.options = [ "-smbios type=1,product=${proofName}" ];
      };
      users.groups.korri.gid = 1000;
      users.users.korri = {
        isNormalUser = true;
        uid = 1000;
        group = "korri";
      };
      users.users.unrelated = {
        isNormalUser = true;
        uid = 1001;
      };
      services.korriLinuxInput = {
        provider = {
          enable = true;
          package = inputplumberKorri;
        };
        inputd = {
          enable = true;
          package = inputdPackage;
          requireProvider = true;
          uid = 977;
          controlGid = 977;
          actionUser = "korri";
          actionUid = 1000;
          actionGid = 1000;
        };
      };
      # Install the actual module's identities, ACL helper, DBus policy and
      # polkit rules. There is no korrid here, so do not start its consumer.
      systemd.services.korri-inputd.wantedBy = lib.mkForce [ ];
    };
  testScript = ''
    machine.start()
    machine.wait_for_unit("multi-user.target")
    machine.wait_for_unit("dbus.service")
    machine.wait_for_unit("polkit.service")
    machine.wait_for_unit("inputplumber.service")
    machine.succeed("test -c /dev/uinput")
    machine.fail("systemctl is-active --quiet korri-inputd.service")
    # Explicit prebuilt interpreter: the guest never evaluates the Nix shebang.
    print(machine.succeed("${python}/bin/python3 ${proof}", timeout=180))
    machine.succeed("systemctl is-active --quiet inputplumber.service")
  '';
}
