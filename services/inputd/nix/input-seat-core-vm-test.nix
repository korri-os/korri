# Real uinput/udev/systemd proof in a guest. No host input device is passed in.
# Uses the evaluated production receiver and selector, not a second unit model.
{
  pkgs,
  inputdPackage,
  receiver,
  selector,
  seatRules,
  sunshinePlugin,
}:
let
  setupUnit = pkgs.runCommand "sunshine-seat-vm-native-unit" { nativeBuildInputs = [ pkgs.jq ]; } ''
    mkdir -p "$out/lib/systemd/system"
    setup="$(jq -er '.services["korri-sunshine-input-setup"]' ${sunshinePlugin}/manifest.json)"
    ln -s "$setup" "$out/lib/systemd/system/korri-sunshine-input-setup.service"
  '';
  inspect = pkgs.writeText "inspect-core-seats.py" ''
    import json, os, pathlib, stat
    seats = {}
    for path in pathlib.Path('/sys/class/input').glob('event*'):
        name = (path / 'device/name').read_text().strip()
        if not name.startswith('Korri Seat P'):
            continue
        slot = int(name.removeprefix('Korri Seat P'))
        assert name == f'Korri Seat P{slot}' and 1 <= slot <= 255
        assert (path / 'device/phys').read_text().strip() == f'korri/input-seat/p{slot}'
        node = '/dev/input/' + path.name
        info = os.stat(node)
        assert info.st_uid == 0 and info.st_gid == 1000
        assert stat.S_IMODE(info.st_mode) == 0o660
        seats[str(slot)] = [node, info.st_ino, info.st_rdev]
    print(json.dumps(seats, sort_keys=True))
  '';
  coordinator = pkgs.writeText "seat-vm-coordinator.py" ''
    import json, socket, struct, time
    from pathlib import Path
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET)
    connection.settimeout(15)
    connection.connect('/run/korri-input-seat/control.sock')
    _, uid, gid = struct.unpack('3i', connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
    assert (uid, gid) == (0, 0)
    def request(operation, **fields):
        connection.sendall(bytes([2]) + json.dumps(dict(operation=operation, **fields)).encode())
        reply = connection.recv(65536)
        assert reply[0] == 2
        value = json.loads(reply[1:])
        assert value['failure'] is None, value
        return value
    request('hello')
    assert request('applyCount', count=6)['count'] == 6
    Path('/run/seat-vm-coordinator/ready').touch()
    while True:
        request('poll')
        time.sleep(0.02)
  '';
in
pkgs.testers.runNixOSTest {
  name = "korri-input-seat-core";
  nodes.machine =
    { ... }:
    {
      boot.kernelModules = [
        "uinput"
        "uhid"
      ];
      users.groups = {
        korri.gid = 1000;
        korrid.gid = 976;
        uinput = { };
        browser = { };
      };
      users.users = {
        korri = {
          isNormalUser = true;
          uid = 1000;
          group = "korri";
        };
        korrid = {
          isSystemUser = true;
          uid = 976;
          group = "korrid";
        };
        browser = {
          isSystemUser = true;
          group = "browser";
        };
      };
      services.udev.packages = [ seatRules ];
      services.udev.extraRules = ''
        KERNEL=="uinput", SUBSYSTEM=="misc", OWNER="root", GROUP="uinput", MODE="0660", OPTIONS+="static_node=uinput"
      '';
      systemd.packages = [ setupUnit ];
      systemd.services.korri-bundle-selector = {
        inherit (selector)
          description
          wantedBy
          before
          environment
          serviceConfig
          ;
      };
      systemd.services.korri-input-seat-receiver = {
        inherit (receiver)
          description
          wantedBy
          requires
          after
          before
          environment
          serviceConfig
          ;
      };
      # Only a launch-order probe: this is not a substitute for korrid integration.
      systemd.services.korrid = {
        wantedBy = [ "multi-user.target" ];
        requires = [ "korri-input-seat-receiver.service" ];
        after = [ "korri-input-seat-receiver.service" ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          ExecStart = "${pkgs.coreutils}/bin/test -S /run/korri-input-seat/control.sock";
        };
      };
      systemd.services.seat-vm-coordinator = {
        requires = [ "korri-input-seat-receiver.service" ];
        after = [ "korri-input-seat-receiver.service" ];
        serviceConfig = {
          User = "korrid";
          Group = "korrid";
          RuntimeDirectory = "seat-vm-coordinator";
          ExecStart = "${pkgs.python3}/bin/python3 ${coordinator}";
        };
      };
      environment.systemPackages = [
        pkgs.python3
        pkgs.util-linux
        inputdPackage
      ];
      system.stateVersion = "26.05";
    };
  testScript = ''
    import json, time
    start_all()
    machine.wait_for_unit("korrid.service")
    machine.wait_for_unit("korri-input-seat-receiver.service")
    machine.succeed("udevadm settle")
    def seats():
        return json.loads(machine.succeed("python3 ${inspect}"))
    baseline = seats()
    assert set(baseline) == {"1", "2", "3", "4"}, baseline
    pid = machine.succeed("systemctl show korri-input-seat-receiver -p MainPID --value").strip()
    machine.fail("systemctl is-active korri-sunshine-input-setup.service")
    machine.succeed("test ! -e /run/korri-input-seat/sunshine-active-launch.json")
    node = baseline["1"][0]
    machine.succeed("runuser -u korri -- python3 -c 'import os; os.close(os.open(\"" + node + "\", os.O_RDONLY | os.O_NONBLOCK))'")
    machine.fail("runuser -u browser -- python3 -c 'import os; os.open(\"" + node + "\", os.O_RDONLY | os.O_NONBLOCK)'")
    machine.fail("runuser -u browser -- python3 -c 'import socket; s=socket.socket(socket.AF_UNIX,socket.SOCK_SEQPACKET); s.connect(\"/run/korri-input-seat/control.sock\")'")

    # Plugin-owned group/UHID setup can start and stop without changing seats.
    machine.succeed("systemctl start korri-sunshine-input-setup.service; udevadm settle")
    machine.succeed("test $(getent group korri-sunshine-input-seat | cut -d: -f3) = 980")
    machine.succeed("test $(stat -c %g /dev/uhid) = 980")
    assert seats() == baseline
    machine.succeed("systemctl stop korri-sunshine-input-setup.service; udevadm settle")
    machine.fail("getent group korri-sunshine-input-seat")
    machine.succeed("test ! -e /run/udev/rules.d/99-z-korri-sunshine-input.rules")
    assert seats() == baseline
    assert machine.succeed("systemctl show korri-input-seat-receiver -p MainPID --value").strip() == pid

    # Real private coordinator grows beyond the obsolete P1..P4 permissions.
    machine.succeed("systemctl start seat-vm-coordinator.service")
    machine.wait_for_file("/run/seat-vm-coordinator/ready")
    machine.succeed("udevadm settle")
    six = seats()
    assert set(six) == {str(i) for i in range(1, 7)}, six
    machine.succeed("systemctl stop seat-vm-coordinator.service")
    time.sleep(2)
    assert seats() == six
    machine.succeed("systemctl stop korri-input-seat-receiver.service")
    assert seats() == {}
  '';
}
