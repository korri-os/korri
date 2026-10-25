# Negative proof for #240, not a successful-reset acceptance test.
# The real module supplies the state root, account, units and activation paths.
{
  pkgs,
  korridModule,
  korridPackage,
  cliPackage,
}:
pkgs.testers.runNixOSTest {
  name = "korri-identity-reset-refusal";
  nodes.machine = {
    imports = [ korridModule ];
    virtualisation.memorySize = 4096;
    users.users.person = {
      isNormalUser = true;
      uid = 1000;
      group = "users";
    };
    environment.systemPackages = [ cliPackage ];
    services.korridLinuxDevice = {
      enable = true;
      package = korridPackage;
      uid = 976;
      gid = 976;
      runtimeUser = "person";
      runtimeUid = 1000;
      runtimeGid = 100;
      inputdUid = 977;
      controlGid = 977;
      localSignerUid = 978;
      localSignerGid = 978;
      deviceConfig = pkgs.writeText "identity-reset-device.toml" ''
        label = "identity-reset-vm"
      '';
      relays = [ "ws://127.0.0.1:49000" ];
    };
    systemd.tmpfiles.rules = [ "d /var/lib/korri 0700 korrid korrid -" ];
  };
  testScript = ''
    import json
    import shlex

    start_all()
    machine.wait_for_unit("korrid.service")
    machine.wait_until_succeeds("korri identity show --json")
    original = json.loads(machine.succeed("korri identity show --json"))
    services = [
        "korrid.service", "korrid-identity.service",
        "korri-local-signer.service", "korri-local-signer-device-credential.service",
    ]
    sockets = ["korrid-cli.socket", "korrid-control.socket", "korri-local-signer.socket"]
    units = " ".join(services + sockets)
    machine.succeed("systemctl stop " + units)
    for unit in services + sockets:
        machine.fail("systemctl is-active --quiet " + unit)

    # Read the mapping actually used by PID 1. No HOME or guessed recovery path.
    environment = machine.succeed("systemctl show korrid-identity.service -p Environment --value")
    state = dict(entry.split("=", 1) for entry in shlex.split(environment))["KORRID_PRIVATE_STATE_ROOT"]
    account = machine.succeed("systemctl show korrid-identity.service -p User --value").strip()
    assert account == "korrid"
    root = shlex.quote(state)
    snapshot = "find " + root + " -type f -exec sha256sum {} + | sort; find " + root + " -printf '%p %U %G %m\\n' | sort"
    before = machine.succeed(snapshot)
    for user in ["root", "person"]:
        for yes in ["", "--yes"]:
            code, output = machine.execute("runuser -u " + user + " -- korri identity reset --json " + yes + " 2>&1")
            assert code == 1, output
            assert json.loads(output)["code"] == "IdentityRecoveryUnavailable", output
            assert machine.succeed(snapshot) == before

    # Stopped units do not establish an interval: a configured writer starts.
    machine.succeed("systemctl start korrid-identity.service")
    machine.succeed("systemctl stop korrid-identity.service")
    assert machine.succeed(snapshot) == before

    # A runtime mask is below NixOS's /etc units in systemd's search path.
    machine.succeed("systemctl mask --runtime " + units)
    machine.succeed("systemctl daemon-reload")
    machine.succeed("systemctl start korrid-identity.service")
    machine.succeed("systemctl stop korrid-identity.service")

    # Even masking named units cannot fence the shipped offline writer. Run it
    # as the installed account against the installed root, in this disposable VM.
    # Temporarily set aside the identity so status must actually create files.
    machine.succeed("mv " + root + "/identity /run/reset-vm-original-identity")
    old = machine.succeed("sha256sum /run/reset-vm-original-identity/device.key").split()[0]
    machine.succeed("runuser -u " + account + " -- env KORRID_PRIVATE_STATE_ROOT=" + root + " korrid identity status")
    new = machine.succeed("sha256sum " + root + "/identity/device.key").split()[0]
    assert old != new
    assert machine.succeed("stat -c '%U %G %a' " + root + "/identity").strip() == "korrid korrid 700"
    machine.succeed("rm -r " + root + "/identity; mv /run/reset-vm-original-identity " + root + "/identity")
    assert machine.succeed(snapshot) == before
    machine.succeed("systemctl unmask --runtime " + units)
    machine.succeed("systemctl start korrid.service")
    machine.wait_for_unit("korrid.service")
    machine.wait_until_succeeds("korri identity show --json")
    assert json.loads(machine.succeed("korri identity show --json")) == original
  '';
}
