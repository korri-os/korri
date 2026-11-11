# Exercise the production PrivatePIDs/KillMode contract, including native
# one-launch rollback and a server-only holder's game freezer authority.
{
  pkgs,
  korrid,
  module,
}:
let
  source = pkgs.writeText "game-stop-plugin.ts" ''
    export const name = "game-stop"
    export const handlers = {
      "settings.describe": () => ({ shared: ["video.render-scale"], schema: {type:"object",properties:{}}, revision:"1" }),
      "launch.prepare": input => {
        const scale = input.overrides?.settings?.video?.["render-scale"]
        return {
          command: input.program, args: ["-c", input.contentPath], env: {},
          tomlUpdates: input.files.config ? [{ path: input.files.config,
            content: scale === undefined ? "" : "[video]\nresolution_multiplier = " + scale + "\n" }] : [],
        }
      },
    }
  '';
  plugin = pkgs.runCommand "game-stop-plugin" { } ''
    mkdir -p $out
    cp ${source} $out/plugin.ts
    echo '{"entry":"plugin.ts","sources":["plugin.ts"]}' > $out/manifest.json
  '';
in
pkgs.testers.runNixOSTest {
  name = "korrid-game-stop";
  nodes.machine =
    { lib, config, ... }:
    {
      # The real korrid module owns game authority. No client or shell freezer
      # module is imported. Upstream PID 1 is enough for freeze and thaw; the
      # product patch changes stopping a still-frozen unit, not these methods.
      imports = [ module ];
      assertions = [
        {
          assertion = config.systemd.package == pkgs.systemd;
          message = "Test freeze/thaw on upstream systemd.";
        }
        {
          assertion = !(config.systemd.services ? korri-shell);
          message = "Test a server-only holder.";
        }
      ];
      virtualisation.memorySize = 2048;
      virtualisation.cores = 2;
      services.korridLinuxDevice = {
        enable = true;
        uid = 976;
        gid = 976;
        runtimeUser = "korri";
        runtimeUid = 1000;
        runtimeGid = 1000;
        inputdUid = 977;
        controlGid = 977;
        localSignerUid = 978;
        localSignerGid = 978;
        deviceConfig = pkgs.writeText "game-stop-host.toml" ''label = "game-stop"'';
        relays = [ "ws://127.0.0.1:7447/" ];
      };
      users.users.korri = {
        isNormalUser = true;
        uid = 1000;
        group = "korri";
      };
      users.groups.korri.gid = 1000;
      security.polkit.extraConfig = lib.mkBefore ''
        polkit.addRule(function(action, subject) {
          if (subject.user == "korrid" && action.lookup("unit") == "korri-game-0123456789abcdef0123456789abcdef.service") {
            polkit.log("game-freezer-test verb=" + action.lookup("verb") + " message=" + action.lookup("polkit.message"));
          }
        });
      '';
    };
  testScript = ''
    import json
    import shlex
    import time

    def unit(name):
        return f"systemd-run --unit={name} --uid=1000 --gid=1000 --service-type=exec -p PrivatePIDs=yes -p KillMode=control-group -p TimeoutStopSec=8"

    def start(name, script, native=False, temporary=False):
        request = {
            "runnerId": "@test:game-stop/sh",
            "program": "${pkgs.bash}/bin/sh", "contentPath": script,
            "accountRoot": "/home/korri", "saveRoot": "/home/korri/saves",
            "cacheRoot": "/home/korri/cache", "files": {}, "artifacts": {},
        }
        composition = {"modifiers": []}
        if native:
            request["files"]["config"] = "/home/korri/Config.toml"
        if temporary:
            request["overrides"] = {"settings": {"video": {"render-scale": 4}}}
            composition["persistent"] = {"settings": {}, "modifiers": []}
        args = ["${plugin}/plugin.ts", json.dumps(request), json.dumps(composition)]
        prepared = machine.succeed(shlex.join(["${korrid}/bin/korrid", "plugin-prepare", *args, "fixture"]))
        machine.succeed(unit(name) + " " + shlex.join(["${korrid}/bin/korrid", "plugin-launch", *args, prepared]))

    def stop_seconds(name):
        started = time.monotonic()
        machine.succeed(f"systemctl stop {name}")
        return time.monotonic() - started

    machine.wait_for_unit("multi-user.target")
    machine.succeed("systemctl start polkit.service")
    machine.wait_for_unit("polkit.service")

    def freezer(verb, name, user="korrid"):
        return f"runuser -u {user} -- systemctl --no-ask-password {verb} {name}"

    with subtest("server-only korrid freezes and thaws a game on upstream systemd"):
        game = "korri-game-0123456789abcdef0123456789abcdef.service"
        start(game, "exec ${pkgs.coreutils}/bin/sleep 1000")
        machine.succeed(freezer("freeze", game))
        assert machine.succeed(f"systemctl show -P FreezerState {game}").strip() == "frozen"
        machine.succeed(f"test $(cat /sys/fs/cgroup/system.slice/{game}/cgroup.freeze) = 1")
        machine.succeed(freezer("thaw", game))
        assert machine.succeed(f"systemctl show -P FreezerState {game}").strip() == "running"
        machine.succeed(f"test $(cat /sys/fs/cgroup/system.slice/{game}/cgroup.freeze) = 0")
        logs = machine.succeed("journalctl -u polkit.service --no-pager")
        message = "Authentication is required to freeze or thaw the processes of '$(unit)' unit."
        for verb in ("stop", "start"):
            assert f"game-freezer-test verb={verb} message={message}" in logs, logs
        # The runtime account must not inherit korrid's authority.
        machine.fail(freezer("freeze", game, user="korri"))
        assert stop_seconds(game) < 4

    with subtest("server freezer authority excludes the shell, other units, malformed games, hooks and acquisitions"):
        for name in (
            "korri-shell.service",
            "unrelated.service",
            "korri-game-0123456789abcdef0123456789abcde.service",
            "korri-game-0123456789abcdef0123456789abcdef0.service",
            "korri-game-0123456789ABCDEF0123456789ABCDEF.service",
            "korri-game-0123456789abcdef0123456789abcdef-extra.service",
            "korri-hook-0123456789abcdef0123456789abcdef-0.service",
            "korri-acquire-test-content.service",
        ):
            machine.succeed(unit(name) + " ${pkgs.coreutils}/bin/sleep 1000")
            machine.fail(freezer("freeze", name))
            assert machine.succeed(f"systemctl show -P FreezerState {name}").strip() == "running"
            machine.succeed(f"systemctl freeze {name}")
            machine.fail(freezer("thaw", name))
            assert machine.succeed(f"systemctl show -P FreezerState {name}").strip() == "frozen"
            machine.succeed(f"systemctl thaw {name}; systemctl kill --signal=KILL {name}")

    with subtest("control: a namespace init without a signal handler waits for the timeout"):
        machine.succeed(unit("bare") + " ${pkgs.coreutils}/bin/sleep 1000")
        machine.wait_until_succeeds("systemctl is-active bare")
        elapsed = stop_seconds("bare")
        result = machine.succeed("systemctl show bare -p Result --value").strip()
        assert elapsed >= 7 and result == "timeout", (elapsed, result)

    with subtest("plugin-launch stops on the first SIGTERM and reaps orphans"):
        start("game", "(${pkgs.coreutils}/bin/sleep 0.2 &); exec ${pkgs.coreutils}/bin/sleep 1000")
        machine.wait_until_succeeds("systemctl is-active game")
        machine.wait_until_succeeds("pgrep -f '^/nix/store/[^ ]*/bin/sleep 1000$'", timeout=30)
        time.sleep(1)
        main = machine.succeed("systemctl show game -p MainPID --value").strip()
        tree = machine.succeed("ps -eo pid=,ppid=,stat=,args=")
        children = [line.split(None, 3) for line in tree.splitlines() if line.split()[1] == main]
        assert "korrid plugin-launch" in machine.succeed(f"cat /proc/{main}/cmdline | tr '\\0' ' '")
        assert [c for c in children if c[3].endswith("sleep 1000")], children
        assert not [c for c in children if c[2].startswith("Z")], children
        elapsed = stop_seconds("game")
        assert elapsed < 4, elapsed
        machine.fail("pgrep -f '^/nix/store/[^ ]*/bin/sleep 1000$'")

    with subtest("the unit keeps the game's exit status"):
        start("status", "exit 7")
        machine.wait_until_succeeds("test \"$(systemctl show status -p ExecMainStatus --value)\" = 7")

    with subtest("temporary native values restore on stop while menu edits survive"):
        machine.succeed("printf '[video]\\nresolution_multiplier = 1\\n[audio]\\nvolume = 0.6\\n' > /home/korri/Config.toml; chown korri:korri /home/korri/Config.toml")
        script = "${pkgs.gnugrep}/bin/grep -q 'resolution_multiplier = 4' /home/korri/Config.toml || exit 91; ${pkgs.gnused}/bin/sed -i 's/volume = 0.6/volume = 0.75/' /home/korri/Config.toml; ${pkgs.coreutils}/bin/touch /home/korri/temporary-ready; exec ${pkgs.coreutils}/bin/sleep 1000"
        start("temporary", script, native=True, temporary=True)
        machine.wait_until_succeeds("test -f /home/korri/temporary-ready", timeout=30)
        assert stop_seconds("temporary") < 4
        machine.succeed("grep -q 'resolution_multiplier = 1' /home/korri/Config.toml")
        machine.succeed("grep -q 'volume = 0.75' /home/korri/Config.toml")
        machine.succeed("test -z \"$(find /home/korri/.korri-native-config -name '*.json')\"")

    with subtest("SIGKILL leaves a receipt and the next launch recovers before program start"):
        start("interrupted", "${pkgs.gnugrep}/bin/grep -q 'resolution_multiplier = 4' /home/korri/Config.toml || exit 92; ${pkgs.coreutils}/bin/touch /home/korri/crash-ready; exec ${pkgs.coreutils}/bin/sleep 1000", native=True, temporary=True)
        machine.wait_until_succeeds("test -f /home/korri/crash-ready", timeout=30)
        machine.succeed("systemctl kill --kill-whom=all --signal=KILL interrupted")
        machine.wait_until_fails("systemctl is-active interrupted")
        machine.wait_until_succeeds("test \"$(systemctl show interrupted -p MainPID --value)\" = 0")
        machine.succeed("grep -q 'resolution_multiplier = 4' /home/korri/Config.toml")
        machine.succeed("test -n \"$(find /home/korri/.korri-native-config -name '*.json')\"")
        start("recovery", "${pkgs.gnugrep}/bin/grep -q 'resolution_multiplier = 1' /home/korri/Config.toml || exit 93; ${pkgs.coreutils}/bin/touch /home/korri/recovered", native=True)
        machine.wait_until_succeeds("test -f /home/korri/recovered", timeout=30)
        machine.wait_until_fails("systemctl is-active recovery")
        machine.succeed("test -z \"$(find /home/korri/.korri-native-config -name '*.json')\"")
  '';
}
