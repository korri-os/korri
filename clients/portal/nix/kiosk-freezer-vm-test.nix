# This gate requires the product systemd patch. Never substitute an
# unpatched package or accept a second stop attempt as success.
{
  pkgs,
  korrid,
  systemd ? pkgs.callPackage ../../../nix/product/systemd/package.nix { },
}:
let
  runtime = korrid.overrideAttrs (old: {
    pname = "korrid-portal-freezer-runtime";
    preConfigure = (old.preConfigure or "") + ''
      # Cargo integration tests build every auto-discovered binary. Unrelated
      # review probes require fixtures outside the packaged crate.
      rm -r src/bin
    '';
    buildPhase = ''
      runHook preBuild
      cargo test --release --offline --locked --test portal_freezer_runtime --no-run
    '';
    doCheck = false;
    postInstall = "";
    installPhase = ''
      mkdir -p $out/bin
      find target -type f -name 'portal_freezer_runtime-*' -executable \
        -exec cp {} $out/bin/portal-freezer-runtime \;
      test -x $out/bin/portal-freezer-runtime
    '';
  });
  swayConfig = pkgs.writeText "freezer-sway.conf" ''
    output HEADLESS-1 resolution 800x600
    seat seat0 fallback true
    focus_follows_mouse no
    xwayland force
  '';
  delayedGame = pkgs.writeShellScript "delayed-game-window" ''
    read -r token < /run/freezer-test/map-game
    exec ${pkgs.xterm}/bin/xterm -class FreezerGame -e ${pkgs.coreutils}/bin/sleep infinity
  '';
  kioskProcess = pkgs.writeText "kiosk-process.py" ''
    import os
    import signal

    def terminate(signum, frame):
        with open("/var/lib/freezer-test/terminated", "a") as marker:
            marker.write(str(os.getpid()) + "\n")
            marker.flush()
            os.fsync(marker.fileno())
        raise SystemExit(0)

    signal.signal(signal.SIGTERM, terminate)
    while True:
        signal.pause()
  '';
in
assert builtins.any (patch: baseNameOf (toString patch) == "stop-thaws-unit.patch") (
  systemd.patches or [ ]
);
pkgs.testers.runNixOSTest {
  name = "korri-portal-freezer";
  nodes.machine =
    { lib, ... }:
    {
      imports = [ ./kiosk-freezer.nix ];
      # Only the existing kiosk option is needed; this test exercises the native
      # freezer module without installing Chromium or the rest of the product.
      options.services.korri.compositor.kiosk.enable = lib.mkEnableOption "kiosk";
      config = {
        services.korri.compositor.kiosk.enable = true;
        systemd.package = systemd;
        environment.systemPackages = [
          systemd
          pkgs.curl
          pkgs.jq
          pkgs.sway-unwrapped
          pkgs.xterm
        ];
        virtualisation.memorySize = 2048;
        virtualisation.cores = 4;
        fonts.packages = [ pkgs.dejavu_fonts ];
        boot.kernelModules = [ "uinput" ];
        users.users.gameplay = {
          isNormalUser = true;
          uid = 1000;
        };
        environment.etc."freezer-host.toml".text = ''
          label = "freezer-vm"
          [[games]]
          id = "delayed"
          title = "Delayed window"
          command = ["${delayedGame}"]
          [environment]
          DISPLAY = ":0"
        '';
        # Game-unit authority is independent of the exact kiosk freezer grant.
        security.polkit.extraConfig = lib.mkAfter ''
          polkit.addRule(function(action, subject) {
            if (subject.user == "korrid" &&
                action.id == "org.freedesktop.systemd1.manage-units" &&
                /^korri-game-[0-9a-f]{32}\.service$/.test(action.lookup("unit")))
              return polkit.Result.YES;
            // A deterministic native denial tests Leave rollback and watcher
            // retry. It is installed before the production grant at runtime.
          });
        '';
        users.groups.korrid = { };
        users.users.korrid = {
          isSystemUser = true;
          group = "korrid";
        };
        users.users.visitor.isNormalUser = true;
        systemd.tmpfiles.rules = [
          "d /var/lib/freezer-test 0755 root root -"
          "d /run/freezer-test 0777 root root -"
          "p /run/freezer-test/map-game 0666 root root -"
          "d /run/freezer-compositor 0777 gameplay users -"
          "d /run/korrid 0755 root root -"
          "d /run/korrid-browser 0755 root root -"
          "d /run/korrid-control 0755 root root -"
          "f /run/korrid-control/control.sock 0600 root root -"
          "d /var/lib/freezer-stream 0700 root root -"
          "d /run/korri-compositor 0700 root root -"
          "d /run/korri-certificate-control 0700 root root -"
          "d /run/user/1000 0700 gameplay users -"
          "d /dev/inputplumber/sources 0755 root root -"
        ];
        systemd.services = {
          korri-chromium-kiosk = {
            wantedBy = [ "multi-user.target" ];
            after = [ "systemd-tmpfiles-setup.service" ];
            serviceConfig = {
              ExecStart = "${pkgs.python3}/bin/python ${kioskProcess}";
              TimeoutStopSec = 10;
            };
          };
          freezer-compositor = {
            path = [ pkgs.xwayland ];
            wantedBy = [ "multi-user.target" ];
            after = [ "systemd-tmpfiles-setup.service" ];
            environment = {
              WLR_BACKENDS = "headless";
              WLR_RENDERER = "pixman";
              WLR_LIBINPUT_NO_DEVICES = "1";
              XDG_RUNTIME_DIR = "/run/freezer-compositor";
              SWAYSOCK = "/run/freezer-compositor/sway.sock";
            };
            serviceConfig = {
              User = "gameplay";
              ExecStart = "${pkgs.sway-unwrapped}/bin/sway --config ${swayConfig}";
            };
          };
          korrid = {
            wantedBy = [ "multi-user.target" ];
            after = [ "dbus.service" ];
            environment = {
              KORRID_SYSTEMD_RUN = "${systemd}/bin/systemd-run";
              KORRID_RUNTIME_UID = "1000";
              KORRID_RUNTIME_GID = "100";
              KORRID_PRIVATE_STATE_ROOT = "/var/lib/korrid";
              KORRID_STREAM_PRIVATE_STATE_ROOT = "/var/lib/freezer-stream";
              KORRID_SWAYMSG = "${pkgs.sway-unwrapped}/bin/swaymsg";
              KORRID_COMPOSITOR_CONTROL_SOCKET = "/run/freezer-compositor/sway.sock";
              KORRID_NEVER_FOCUS_APP_IDS = "FreezerPortal";
            };
            serviceConfig = {
              ExecStart = "${runtime}/bin/portal-freezer-runtime --ignored --nocapture";
              User = "korrid";
              StateDirectory = "korrid";
              StateDirectoryMode = "0700";
            };
          };
          bystander = {
            wantedBy = [ "multi-user.target" ];
            serviceConfig.ExecStart = "${pkgs.coreutils}/bin/sleep infinity";
          };
        };
      };
    };
  testScript = ''
    import json
    import shlex

    kiosk = "korri-chromium-kiosk.service"
    manager = "org.freedesktop.systemd1 /org/freedesktop/systemd1 org.freedesktop.systemd1.Manager"

    def native(user, method, signature, arguments):
        return f"runuser -u {user} -- busctl --system --allow-interactive-authorization=no call {manager} {method} {signature} {arguments}"

    def settle_runtime():
        machine.wait_for_open_port(43117)
        body = shlex.quote(json.dumps({"_tag": "app.session.status", "payload": {}}))
        machine.succeed("curl --fail -s -H 'Authorization: Bearer freezer-vm-only' -H 'Origin: http://127.0.0.1:8099' -H 'Content-Type: application/json' --data " + body + " http://127.0.0.1:43117/rpc")

    def freezer():
        return machine.succeed(f"systemctl show -P FreezerState {kiosk}").strip()

    def pid():
        return machine.succeed(f"systemctl show -P MainPID {kiosk}").strip()

    def terminated(old_pid):
        machine.succeed(f"grep -x {old_pid} /var/lib/freezer-test/terminated")

    def freeze():
        machine.succeed(native("korrid", "FreezeUnit", "s", kiosk))
        assert freezer() == "frozen"
        machine.succeed(f"grep -x 'frozen 1' /sys/fs/cgroup/system.slice/{kiosk}/cgroup.events")

    machine.wait_for_unit(kiosk)
    assert machine.succeed("readlink /proc/1/exe").strip() == "${systemd}/lib/systemd/systemd"
    machine.wait_for_unit("korrid.service")
    machine.wait_for_unit("bystander.service")
    settle_runtime()
    # Signal-handler readiness, not just service activation.
    machine.wait_until_succeeds(f"grep -E 'SigCgt:.*[4-7c-f][0-9a-f]{{3}}$' /proc/$(systemctl show -P MainPID {kiosk})/status")

    with subtest("native calls settle before reply, with no RefUnit grant"):
        freeze()
        machine.succeed(native("korrid", "ThawUnit", "s", kiosk))
        assert freezer() == "running"
        machine.succeed(native("korrid", "ThawUnit", "s", kiosk))

    with subtest("other methods, units, and users have no authority"):
        for method in ["StartUnit", "StopUnit", "RestartUnit", "TryRestartUnit", "ReloadUnit", "ReloadOrRestartUnit"]:
            machine.fail(native("korrid", method, "ss", f"{kiosk} replace"))
        machine.fail(native("korrid", "KillUnit", "ssi", f"{kiosk} all 15"))
        machine.fail(native("korrid", "RefUnit", "s", kiosk))
        machine.fail(native("korrid", "FreezeUnit", "s", "bystander.service"))
        machine.fail(native("korrid", "ThawUnit", "s", "bystander.service"))
        machine.fail(native("visitor", "FreezeUnit", "s", kiosk))
        machine.fail(native("visitor", "ThawUnit", "s", kiosk))
        assert freezer() == "running"

    with subtest("first restart handles SIGTERM on the frozen kiosk"):
        old = pid()
        freeze()
        machine.succeed(f"systemctl restart {kiosk}")
        terminated(old)
        assert pid() != old
        assert freezer() == "running"

    with subtest("first stop handles SIGTERM on the frozen kiosk"):
        machine.wait_until_succeeds(f"grep -E 'SigCgt:.*[4-7c-f][0-9a-f]{{3}}$' /proc/$(systemctl show -P MainPID {kiosk})/status")
        old = pid()
        freeze()
        machine.succeed(f"systemctl stop {kiosk}")
        terminated(old)
        machine.fail(f"systemctl is-active {kiosk}")
        machine.succeed(f"systemctl start {kiosk}")

    with subtest("stopping and crashing korrid thaw without browser polling"):
        freeze()
        machine.succeed("systemctl stop korrid.service")
        assert freezer() == "running"
        machine.succeed("systemctl start korrid.service")
        # Do not mistake the startup thaw for crash-hook recovery.
        settle_runtime()
        freeze()
        machine.succeed("kill -KILL $(systemctl show -P MainPID korrid.service)")
        machine.wait_until_succeeds(f'test "$(systemctl show -P FreezerState {kiosk})" = running')

    with subtest("production korrid waits for mapped focus, acknowledges Leave, and watches exit"):
        machine.succeed("systemctl start korrid.service")
        machine.wait_for_open_port(43117)
        machine.wait_for_unit("freezer-compositor.service")
        machine.wait_until_succeeds("test -S /tmp/.X11-unix/X0")
        machine.succeed("chmod 0666 /run/freezer-compositor/sway.sock")
        # Keep an excluded portal window behind the delayed game. Production
        # game units expose only X11, not the compositor control socket.
        machine.succeed("systemd-run --unit=freezer-portal-window --uid=gameplay --setenv=DISPLAY=:0 ${pkgs.xterm}/bin/xterm -class FreezerPortal -e ${pkgs.coreutils}/bin/sleep infinity")
        sway = "swaymsg -s /run/freezer-compositor/sway.sock"
        machine.wait_until_succeeds(sway + " -t get_tree | jq -e '.. | objects | select(.window_properties?.class == \"FreezerPortal\")'")

        def rpc(method, payload):
            body = shlex.quote(json.dumps({"_tag": method, "payload": payload}))
            result = machine.succeed("curl --fail -s -H 'Authorization: Bearer freezer-vm-only' -H 'Origin: http://127.0.0.1:8099' -H 'Content-Type: application/json' --data " + body + " http://127.0.0.1:43117/rpc")
            return json.loads(result)["outcome"]

        def launch():
            reply = rpc("app.session.prepare", {"gameId": "delayed"})
            assert reply["_tag"] == "Ok", reply
            return reply["payload"]["launchId"]

        launch_id = launch()
        game = f"korri-game-{launch_id}.service"
        assert freezer() == "running", "an unmapped game must not freeze the portal"
        # Status observes the live process but must not mistake it for focus.
        for _ in range(3):
            rpc("app.session.status", {})
            assert freezer() == "running"
        machine.succeed("echo map > /run/freezer-test/map-game")
        # A live game cgroup does not prove its window mapped. Keep process
        # state beside the compositor tree when this asynchronous gate fails.
        try:
            machine.wait_until_succeeds(f'test "$(systemctl show -P FreezerState {kiosk})" = frozen', timeout=30)
        except Exception:
            print(machine.succeed(sway + " -t get_tree"))
            print(machine.succeed(f"cat /sys/fs/cgroup/system.slice/{game}/cgroup.procs"))
            print(machine.succeed(f"journalctl -u {game} -u korrid --no-pager"))
            print(machine.succeed(f"systemctl show {game} -p ExecStart -p Environment -p ControlGroup -p FreezerState -p MainPID"))
            print(machine.succeed("ps -eo pid,ppid,uid,stat,wchan:24,args"))
            print(machine.succeed("stat /run/freezer-test/map-game"))
            raise
        reply = rpc("app.session.freeze", {"expectedLaunchId": launch_id})
        assert reply["_tag"] == "Ok", reply
        assert freezer() == "running", "acknowledged Leave includes portal thaw"
        assert machine.succeed(f"systemctl show -P FreezerState {game}").strip() == "frozen"
        machine.succeed(sway + " '[class=FreezerPortal] focus'")
        # A real compositor refusal must leave the portal active, even after
        # the exact game has thawed. Restoring authority then permits Return.
        machine.succeed("chmod 0600 /run/freezer-compositor/sway.sock")
        reply = rpc("app.session.thaw", {"expectedLaunchId": launch_id})
        assert reply["_tag"] == "Err", reply
        assert reply["payload"]["code"] == "HostFocusFailed", reply
        assert machine.succeed(f"systemctl show -P FreezerState {game}").strip() == "running"
        assert freezer() == "running"
        machine.succeed("chmod 0666 /run/freezer-compositor/sway.sock")
        reply = rpc("app.session.thaw", {"expectedLaunchId": launch_id})
        assert reply["_tag"] == "Ok", reply
        assert machine.succeed(f"systemctl show -P FreezerState {game}").strip() == "running"
        machine.succeed(sway + " -t get_tree | jq -e '.. | objects | select(.window_properties?.class == \"FreezerGame\" and .focused == true)'")
        assert freezer() == "frozen"

        # Count real native thaw requests, not repeated application logs.
        machine.succeed("systemd-run --unit=freezer-monitor --property=StandardOutput=append:/run/freezer-test/calls busctl --system --match=\"type='method_call',member='ThawUnit'\" monitor org.freedesktop.systemd1")
        machine.wait_until_succeeds("grep -q 'Monitoring bus message stream' /run/freezer-test/calls || journalctl -u freezer-monitor --no-pager | grep -q 'Monitoring bus message stream'", timeout=10)
        # Deny only native portal ThawUnit. Failed Leave must thaw/refocus the
        # game before returning an existing transition error to inputd.
        denial = """polkit.addRule(function(action, subject) {
          if (subject.user == "korrid" && action.lookup("unit") == "korri-chromium-kiosk.service" && action.lookup("verb") == "start") {
            return polkit.Result.NO;
          }
        });"""
        machine.succeed("printf %s " + shlex.quote(denial) + " > /etc/polkit-1/rules.d/00-deny-portal-thaw.rules")
        machine.succeed("systemctl restart polkit.service")
        machine.fail(native("korrid", "ThawUnit", "s", kiosk))
        reply = rpc("app.session.freeze", {"expectedLaunchId": launch_id})
        assert reply["_tag"] == "Err", reply
        assert reply["payload"]["code"] == "HostFreezerFailed", reply
        assert machine.succeed(f"systemctl show -P FreezerState {game}").strip() == "running"
        machine.succeed(sway + " -t get_tree | jq -e '.. | objects | select(.window_properties?.class == \"FreezerGame\" and .focused == true)'")
        # Natural exit, no session status polling: watcher must retry the
        # refused thaw and recover once native permission returns.
        machine.succeed(f"kill -TERM $(systemctl show -P MainPID {game})")
        machine.wait_until_succeeds("test $(grep -c 'Member=ThawUnit' /run/freezer-test/calls) -ge 5", timeout=30)
        machine.succeed("test $(journalctl -u korrid --no-pager | grep -c 'portal freezer failed') -eq 1")
        machine.succeed("rm /etc/polkit-1/rules.d/00-deny-portal-thaw.rules")
        machine.wait_until_succeeds(f'test "$(systemctl show -P FreezerState {kiosk})" = running')
        machine.succeed("journalctl -u korrid --no-pager | grep 'portal freezer recovered'")

    with subtest("orderly shutdown delivers SIGTERM without korrid or a thaw companion"):
        machine.succeed("systemctl stop korrid.service")
        machine.wait_until_succeeds(f"grep -E 'SigCgt:.*[4-7c-f][0-9a-f]{{3}}$' /proc/$(systemctl show -P MainPID {kiosk})/status")
        old = pid()
        machine.succeed("rm -f /var/lib/freezer-test/terminated")
        freeze()
        machine.shutdown()
        machine.start()
        machine.wait_for_unit(kiosk)
        terminated(old)
  '';
}
