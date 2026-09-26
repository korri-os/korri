{
  pkgs,
  configuration,
  consoleConfiguration,
}:
let
  kiosk = configuration.config.systemd.services.korri-chromium-kiosk;
  korrid = configuration.config.systemd.services.korrid;
  systemd = pkgs.callPackage ./package.nix { };
  python = pkgs.python3.withPackages (ps: [ ps.dbus-next ]);
  fusePython = pkgs.python3.withPackages (ps: [ ps.fusepy ]);
  upstream = pkgs.runCommand "systemd-freezer-upstream-tests" { } ''
    mkdir -p $out
    cp ${systemd.src}/test/units/{TEST-38-FREEZER.sh,test-control.sh} $out/
    chmod u+w $out/TEST-38-FREEZER.sh
    # Intentional downstream semantic change: first stop now succeeds. Drop
    # the subsequent thaw and duplicate stop of the collected transient unit.
    ${pkgs.python3}/bin/python - "$out/TEST-38-FREEZER.sh" <<'PY'
    import pathlib, sys
    path = pathlib.Path(sys.argv[1])
    text = path.read_text()
    old = """    echo -n "  - can't stop a frozen unit: "
        (! systemctl -q stop "$unit" )
        echo "[ OK ]"
        systemctl thaw "$unit"

        systemctl stop "$unit"
    """.rstrip('\n')
    new = """    echo -n "  - stop thaws a frozen unit: "
        systemctl -q stop "$unit"
        echo "[ OK ]"
    """.rstrip('\n')
    assert text.count(old) == 1
    path.write_text(text.replace(old, new))
    PY
  '';
  service = name: {
    serviceConfig = {
      Type = "notify";
      ExecStart = "${python}/bin/python ${./service.py} ${name}";
      TimeoutStopSec = "10s";
    };
  };
in
assert builtins.elem ./stop-thaws-unit.patch configuration.config.systemd.package.patches;
assert !(builtins.elem ./stop-thaws-unit.patch consoleConfiguration.config.systemd.package.patches);
pkgs.testers.runNixOSTest {
  name = "rpminiv2-systemd-freezer-stop";
  nodes.machine =
    { lib, ... }:
    {
      imports = [ ./module.nix ];
      virtualisation.memorySize = 2048;
      virtualisation.cores = 4;
      environment.systemPackages = [
        python
        pkgs.procps
      ];
      boot.kernelModules = [ "fuse" ];
      systemd.services = {
        kiosk = lib.recursiveUpdate (service "kiosk") {
          serviceConfig.ExecReload = "${pkgs.coreutils}/bin/true";
        };
        other = service "other";
        shutdown-marker = service "shutdown";
        watchdog = lib.recursiveUpdate (service "watchdog") {
          serviceConfig.WatchdogSec = "3s";
        };
        held = lib.recursiveUpdate (service "held") {
          serviceConfig.ExecStop = "${pkgs.writeShellScript "hold-stop" ''
            read -r token < /run/freezer-release
          ''}";
        };
        child = lib.recursiveUpdate (service "child") { serviceConfig.Slice = "parent.slice"; };
        sibling = lib.recursiveUpdate (service "sibling") { serviceConfig.Slice = "parent.slice"; };
        inflight = service "inflight";
        blocked-fuse = {
          wantedBy = [ "multi-user.target" ];
          serviceConfig.ExecStart = "${fusePython}/bin/python ${./blocked-fuse.py}";
        };
        TEST-38-FREEZER-sleep.serviceConfig.ExecStart = "${pkgs.coreutils}/bin/sleep infinity";
      }
      # Use the product's actual dependency lists, with test processes instead of
      # Chromium, korrid, nginx and the compositor. This tests PID1 recovery wiring,
      # not the Rust daemon or the browser's own recovery logic.
      // lib.genAttrs (map (lib.removeSuffix ".service") kiosk.requires) service
      // {
        korri-chromium-kiosk = (service "recovery-kiosk") // {
          inherit (kiosk)
            wantedBy
            requires
            after
            partOf
            ;
        };
        korrid = lib.recursiveUpdate (service "korrid") {
          serviceConfig = lib.getAttrs [ "Restart" "RestartSec" ] korrid.serviceConfig;
        };
      };
      systemd.slices.parent = { };
      users.users.freezer-client = {
        isNormalUser = true;
      };
      security.polkit.enable = true;
      security.polkit.extraConfig = ''
        polkit.addRule(function(action, subject) {
          if (subject.user === "freezer-client" &&
              action.id === "org.freedesktop.systemd1.manage-units" &&
              action.lookup("unit") === "kiosk.service" &&
              (action.lookup("verb") === "stop" || action.lookup("verb") === "restart"))
            return polkit.Result.YES;
        });
      '';
    };
  testScript = ''
    machine.start()
    machine.wait_for_unit("multi-user.target")
    machine.wait_until_succeeds("test -f /run/freezer-fuse-ready")
    # Check the actual running PID1, not merely the installed package.
    assert machine.succeed("readlink /proc/1/exe").strip() == "${systemd}/lib/systemd/systemd"
    print(machine.succeed("${python}/bin/python ${./regression.py}", timeout=240))
    print(machine.succeed("su -s /bin/sh freezer-client -c '${python}/bin/python ${./regression.py} unprivileged restart'", timeout=60))
    machine.succeed("busctl call org.freedesktop.systemd1 /org/freedesktop/systemd1 org.freedesktop.systemd1.Manager FreezeUnit s kiosk.service")
    print(machine.succeed("su -s /bin/sh freezer-client -c '${python}/bin/python ${./regression.py} unprivileged stop'", timeout=60))
    machine.succeed("bash ${upstream}/TEST-38-FREEZER.sh > /var/log/upstream-freezer.log 2>&1 || { cat /var/log/upstream-freezer.log; exit 1; }", timeout=180)
    machine.succeed("test -f /testok")
    completed = machine.succeed("grep 'testcase_.* END' /var/log/upstream-freezer.log").splitlines()
    assert len(completed) == 6, completed
    print("PASS: all six upstream TEST-38-FREEZER cases: " + ", ".join(completed))
    machine.copy_from_vm("/var/log/upstream-freezer.log")
    # shutdown() initiates ordinary poweroff. The frozen process itself must
    # receive SIGTERM and fsync its marker before the virtual disk goes away.
    machine.shutdown()
    machine.start()
    machine.wait_for_unit("multi-user.target")
    machine.succeed("grep -xE '[0-9]+ SIGTERM' /var/lib/freezer-test/shutdown")
    print("PASS: orderly shutdown thawed the unit and delivered SIGTERM")
  '';
}
